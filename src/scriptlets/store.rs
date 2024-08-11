use std::{collections::BTreeMap, fs, io::ErrorKind, iter, ops::Deref};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use dupe::Dupe;
use log::{info, log_enabled};
use starlark::{environment::FrozenModule, eval::FileLoader, values::FrozenHeap};

use crate::{
    context::Context,
    error::{Error, IOAction},
    result::Result,
    scriptlets::{
        scriptlet::{InitingScriptlet, PreinitingScriptlet},
        ObserverData,
    },
    source_path::{PrettyPath, SourcePath},
};

type StoreIndex = usize;

#[derive(Debug)]
pub struct PreinitingStore {
    vex_dir: Utf8PathBuf,
    path_indices: BTreeMap<PrettyPath, StoreIndex>,
    store: Vec<PreinitingScriptlet>,
}

impl PreinitingStore {
    pub fn new(ctx: &Context) -> Result<Self> {
        let mut ret = Self {
            vex_dir: ctx.vex_dir(),
            path_indices: BTreeMap::new(),
            store: Vec::new(),
        };
        ret.load_dir(ctx, ctx.vex_dir())?;
        Ok(ret)
    }

    fn load_dir(&mut self, ctx: &Context, path: Utf8PathBuf) -> Result<()> {
        let dir = fs::read_dir(&path).map_err(|err| match err.kind() {
            ErrorKind::NotFound => Error::NoVexesDir(path.clone()),
            _ => Error::IO {
                path: PrettyPath::new(&path),
                action: IOAction::Read,
                cause: err,
            },
        })?;
        for entry in dir {
            let entry = entry.map_err(|cause| Error::IO {
                path: PrettyPath::new(&path),
                action: IOAction::Read,
                cause,
            })?;
            let entry_path = Utf8PathBuf::try_from(entry.path())?;
            let metadata = fs::symlink_metadata(&entry_path).map_err(|cause| Error::IO {
                path: PrettyPath::new(&entry_path),
                action: IOAction::Read,
                cause,
            })?;

            if metadata.is_symlink() {
                if log_enabled!(log::Level::Info) {
                    let symlink_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                    info!("ignoring /{symlink_path} (symlink)");
                }
                continue;
            }

            if metadata.is_dir() {
                self.load_dir(ctx, entry_path)?;
                continue;
            }

            if !metadata.is_file() {
                panic!("unreachable");
            }
            if !entry_path.extension().is_some_and(|ext| ext == "star") {
                if log_enabled!(log::Level::Info) {
                    let unknown_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                    info!("ignoring /{unknown_path} (expected `.star` extension)");
                }
                continue;
            }
            let scriptlet_path = SourcePath::new(&entry_path, &self.vex_dir);
            self.load_file(scriptlet_path)?;
        }

        Ok(())
    }

    fn load_file(&mut self, path: SourcePath) -> Result<()> {
        if self.path_indices.get(&path.pretty_path).is_some() {
            return Ok(());
        }

        let scriptlet = PreinitingScriptlet::new(path.dupe())?;
        self.store.push(scriptlet);
        self.path_indices
            .insert(path.pretty_path.dupe(), self.store.len() - 1);

        Ok(())
    }

    pub fn preinit(mut self, opts: PreinitOptions) -> Result<InitingStore> {
        self.check_loads()?;
        self.sort();
        self.linearise_store()?;

        let Self { store, .. } = self;

        let frozen_heap = FrozenHeap::new();
        let mut initing_store = Vec::with_capacity(store.len());
        let mut loader = Loader::new();
        for scriptlet in store.into_iter() {
            loader.set_current_file(scriptlet.path.pretty_path.dupe());
            let preinited_scriptlet = scriptlet.preinit(&opts, &loader, &frozen_heap)?;
            loader.store(
                preinited_scriptlet.path.pretty_path.dupe(),
                preinited_scriptlet.preinited_module.dupe(),
            );
            initing_store.push(preinited_scriptlet);
        }

        Ok(InitingStore {
            store: initing_store,
            frozen_heap,
        })
    }

    fn check_loads(&self) -> Result<()> {
        // TODO(kcza): use relative loads
        let mut unknown_loads = self.store.iter().flat_map(|s| {
            s.loads()
                .iter()
                .filter(|l| self.path_indices.get(l).is_none())
        });
        if let Some(unknown_module) = unknown_loads.next() {
            return Err(Error::NoSuchModule(unknown_module.dupe()));
        }
        Ok(())
    }

    fn sort(&mut self) {
        self.store
            .sort_by(|s, t| s.path.pretty_path.cmp(&t.path.pretty_path));
        self.path_indices = self
            .store
            .iter()
            .enumerate()
            .map(|(i, s)| (s.path.pretty_path.dupe(), i))
            .collect();
    }

    /// Topographically order the store
    fn linearise_store(&mut self) -> Result<()> {
        fn directed_dfs(
            linearised: &mut Vec<StoreIndex>,
            explored: &mut Vec<bool>,
            loads: &[Vec<StoreIndex>],
            loaded_by: &[Vec<StoreIndex>],
            node: StoreIndex,
        ) {
            if !loads[node].iter().all(|n| explored[*n]) {
                return;
            }

            explored[node] = true;
            linearised.push(node);
            for m in &loaded_by[node] {
                directed_dfs(linearised, explored, loads, loaded_by, *m);
            }
        }

        let load_edges = self.get_load_edges();
        let loaded_by_edges = self.get_loaded_by_edges(&load_edges);
        let n = self.store.len();
        let mut explored = vec![false; n];
        let mut linearised = Vec::with_capacity(n);
        for node in 0..n {
            if explored[node] {
                continue;
            }

            directed_dfs(
                &mut linearised,
                &mut explored,
                &load_edges,
                &loaded_by_edges,
                node,
            );
        }
        // Presence of an import cycle will prevent some nodes entering the
        // linearisation.
        if linearised.len() != self.store.len() {
            return Err(Error::ImportCycle(self.find_cycle()));
        }
        linearised
            .into_iter()
            .enumerate()
            .filter(|(i, j)| i < j)
            .for_each(|(i, j)| self.store.swap(i, j));

        Ok(())
    }

    fn find_cycle(&self) -> Vec<PrettyPath> {
        fn undirected_dfs(
            stack: &mut Vec<StoreIndex>,
            explored: &mut Vec<bool>,
            edges: &[Vec<StoreIndex>],
            node: StoreIndex,
        ) -> Option<Vec<StoreIndex>> {
            if stack.contains(&node) {
                return Some(
                    stack
                        .iter()
                        .copied()
                        .skip_while(|n| *n != node)
                        .chain([node])
                        .collect(),
                );
            }

            if explored[node] {
                return None;
            }
            explored[node] = true;

            stack.push(node);
            for next in &edges[node] {
                let r = undirected_dfs(stack, explored, edges, *next);
                if r.is_some() {
                    return r;
                }
            }
            stack.pop();

            None
        }

        let mut stack = vec![];
        let edges = {
            let mut edges = self.get_load_edges();
            self.get_loaded_by_edges(&edges)
                .into_iter()
                .enumerate()
                .for_each(|(n, g)| g.iter().for_each(|m| edges[*m].push(n)));
            edges
        };
        let n = self.store.len();
        let mut explored = vec![false; n];
        let mut cycle = None;
        for node in 0..n {
            let c = undirected_dfs(&mut stack, &mut explored, &edges, node);
            if c.is_some() {
                cycle = c;
                break;
            }
        }
        cycle
            .unwrap()
            .into_iter()
            .map(|idx| self.store[idx].path.pretty_path.dupe())
            .collect()
    }

    fn get_load_edges(&self) -> Vec<Vec<StoreIndex>> {
        self.store
            .iter()
            .map(|s| {
                let mut adjacent = s
                    .loads()
                    .iter()
                    .map(|m| *self.path_indices.get(m).unwrap())
                    .collect::<Vec<_>>();
                adjacent.sort();
                adjacent
            })
            .collect()
    }

    fn get_loaded_by_edges(&self, load_edges: &[Vec<StoreIndex>]) -> Vec<Vec<StoreIndex>> {
        let mut ret = iter::repeat_with(Vec::new)
            .take(self.store.len())
            .collect::<Vec<_>>();
        load_edges
            .iter()
            .enumerate()
            .rev()
            .for_each(|(n, a)| a.iter().for_each(|m| ret[*m].push(n)));
        ret
    }
}

#[derive(Debug, Default)]
pub struct PreinitOptions {
    pub lenient: bool,
}

#[derive(Debug, Default)]
pub struct Loader {
    current_path: Option<PrettyPath>,
    module_store: BTreeMap<PrettyPath, FrozenModule>,
}

impl Loader {
    fn new() -> Self {
        Self::default()
    }

    fn set_current_file(&mut self, current_dir: PrettyPath) {
        self.current_path = Some(current_dir);
    }

    fn store(&mut self, path: PrettyPath, module: FrozenModule) {
        self.module_store.insert(path, module);
    }
}

impl FileLoader for Loader {
    fn load(&self, path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
        // Preconditions:
        // - path is not empty
        // - path starts with one ./, many ../ or has no path operators
        // - path only has path operators at the start.

        let path = Utf8Path::new(path);
        let mut components = path.components();
        let current_path = self
            .current_path
            .as_ref()
            .expect("internal error: current_dir not set");
        let abs_path = match components.next().expect("internal error: load path empty") {
            Utf8Component::CurDir => {
                let mut abs_path = Utf8PathBuf::with_capacity(
                    current_path.as_str().len() + path.as_str().len() - 1,
                );
                if let Some(current_dir) = current_path.parent() {
                    abs_path.push(current_dir);
                }
                abs_path.push(&path.as_str()[2..]);
                PrettyPath::new(&abs_path)
            }
            Utf8Component::ParentDir => {
                let parents = 1 + components
                    .take_while(|component| matches!(component, Utf8Component::ParentDir))
                    .count();
                let Some(parent_dir) = current_path.ancestors().nth(1 + parents) else {
                    return Err(Error::PathOutOfBounds(path.to_owned()).into());
                };
                let abs_path = {
                    let max_capacity = current_path.as_str().len() + 1 + path.as_str().len();
                    let mut abs_path = Utf8PathBuf::with_capacity(max_capacity);
                    abs_path.push(parent_dir);
                    abs_path.extend(path.components().skip(parents));
                    abs_path
                };
                PrettyPath::new(&abs_path)
            }
            _ => PrettyPath::new(path),
        };
        self.module_store
            .get(&PrettyPath::new(&abs_path))
            .map(Dupe::dupe)
            .ok_or_else(|| Error::NoSuchModule(PrettyPath::new(path)).into())
    }
}

#[derive(Debug)]
pub struct InitingStore {
    store: Vec<InitingScriptlet>,
    frozen_heap: FrozenHeap,
}

impl InitingStore {
    pub fn init(self) -> Result<VexingStore> {
        let Self { store, frozen_heap } = self;
        let num_scripts = store.len();

        let observer_data = store.into_iter().try_fold(
            ObserverData::with_capacity(4 * num_scripts),
            |mut data, scriptlet| {
                data.extend(scriptlet.init(&frozen_heap)?);
                Ok::<_, Error>(data)
            },
        )?;

        Ok(VexingStore {
            num_scripts,
            observer_data,
            frozen_heap,
        })
    }
}

#[derive(Debug)]
pub struct VexingStore {
    num_scripts: usize,
    observer_data: ObserverData,
    frozen_heap: FrozenHeap,
}

impl VexingStore {
    pub fn frozen_heap(&self) -> &FrozenHeap {
        &self.frozen_heap
    }

    pub fn project_queries_hint(&self) -> usize {
        // Heuristic: expect scriptlets to declare on average at most this many queries during the
        // `open_project` event.
        2 * self.num_scripts
    }

    pub fn file_queries_hint(&self) -> usize {
        // Heuristic: expect scriptlets to declare on average at most this many queries during the
        // `open_file` event.
        2 * self.num_scripts
    }
}

impl Deref for VexingStore {
    type Target = ObserverData;

    fn deref(&self) -> &Self::Target {
        &self.observer_data
    }
}

#[cfg(test)]
mod test {
    use indoc::formatdoc;
    use starlark::{
        environment::{Globals, Module},
        eval::Evaluator,
        syntax::{AstModule, Dialect},
    };

    use super::*;

    #[test]
    fn relative_loads() {
        let mut loader = Loader::new();
        let known_file_paths = [
            "foo/bar/sibling.star",
            "foo/parent.star",
            "grandparent.star",
            "foo/qux/cousin.star",
            "quux/uncle.star",
        ];
        known_file_paths.into_iter().for_each(|path| {
            let module = Module::new();
            module.set("path", module.heap().alloc(path));
            let frozen_module = module.freeze().unwrap();

            loader.store(PrettyPath::new(path.into()), frozen_module)
        });

        Test::file("foo/bar/baz.star")
            .which_loads("./sibling.star")
            .with_loader(&mut loader)
            .gets("foo/bar/sibling.star");
        Test::file("foo/bar/baz.star")
            .which_loads("../parent.star")
            .with_loader(&mut loader)
            .gets("foo/parent.star");
        Test::file("foo/bar/baz.star")
            .which_loads("../../grandparent.star")
            .with_loader(&mut loader)
            .gets("grandparent.star");
        Test::file("foo/bar/baz.star")
            .which_loads("../qux/cousin.star")
            .with_loader(&mut loader)
            .gets("foo/qux/cousin.star");
        Test::file("foo/bar/baz.star")
            .which_loads("../../quux/uncle.star")
            .with_loader(&mut loader)
            .gets("quux/uncle.star");
        Test::file("root.star")
            .which_loads("./grandparent.star")
            .with_loader(&mut loader)
            .gets("grandparent.star");
        Test::file("root.star")
            .which_loads("./grandparent.star")
            .with_loader(&mut loader)
            .gets("grandparent.star");
        Test::file("foo/bar.star")
            .which_loads("../quux/uncle.star")
            .with_loader(&mut loader)
            .gets("quux/uncle.star");

        Test::file("foo/bar/baz.star")
            .which_loads("./nonexistent.star")
            .with_loader(&mut loader)
            .errors();
        Test::file("foo/bar/baz.star")
            .which_loads("../nonexistent.star")
            .with_loader(&mut loader)
            .errors();
        Test::file("foo/bar/baz.star")
            .which_loads("../../../../../../fugitive.star")
            .with_loader(&mut loader)
            .errors();

        // Test structs
        struct Test<'loader> {
            file: &'static str,
            to_load: Option<&'static str>,
            loader: Option<&'loader mut Loader>,
        }

        impl<'loader> Test<'loader> {
            fn file(file: &'static str) -> Self {
                Self {
                    loader: None,
                    file,
                    to_load: None,
                }
            }

            fn which_loads(mut self, to_load: &'static str) -> Self {
                self.to_load = Some(to_load);
                self
            }

            fn with_loader(mut self, loader: &'loader mut Loader) -> Self {
                self.loader = Some(loader);
                self
            }

            fn gets(self, expected_path: &'static str) {
                assert_eq!(self.try_run().unwrap(), expected_path);
            }

            fn errors(self) {
                self.try_run().unwrap_err();
            }

            fn try_run(self) -> Result<String> {
                let Self {
                    file,
                    to_load,
                    loader,
                } = self;
                let to_load = to_load.unwrap();
                let loader = loader.unwrap();
                loader.set_current_file(file.into());

                let code = formatdoc!(
                    r#"
                        load('{to_load}', 'path')
                        path
                    "#
                );
                let ast = AstModule::parse(file, code, &Dialect::Standard).unwrap();
                let module = Module::new();
                let mut eval = Evaluator::new(&module);
                eval.set_loader(loader);
                Ok(eval
                    .eval_module(ast, &Globals::standard())?
                    .unpack_str()
                    .unwrap()
                    .to_owned())
            }
        }
    }
}
