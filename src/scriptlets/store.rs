use std::{collections::BTreeMap, fs, io::ErrorKind, iter, ops::Deref};

use camino::Utf8PathBuf;
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
        let mut cache = PreinitedModuleCache::new();
        for scriptlet in store.into_iter() {
            let preinited_scriptlet = scriptlet.preinit(&opts, &cache, &frozen_heap)?;
            cache.cache(&preinited_scriptlet);
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

#[derive(Debug)]
pub struct PreinitedModuleCache {
    exports: BTreeMap<PrettyPath, FrozenModule>,
}

impl PreinitedModuleCache {
    fn new() -> Self {
        Self {
            exports: BTreeMap::new(),
        }
    }

    fn cache(&mut self, scriptlet: &InitingScriptlet) {
        self.exports.insert(
            scriptlet.path.pretty_path.dupe(),
            scriptlet.preinited_module.dupe(),
        );
    }
}

impl FileLoader for &PreinitedModuleCache {
    fn load(&self, path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
        let path = PrettyPath::from(path);
        self.exports
            .get(&path)
            .map(Dupe::dupe)
            .ok_or_else(|| Error::NoSuchModule(path).into())
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

    pub fn vexes(&self) -> impl Iterator<Item = &InitingScriptlet> {
        self.store.iter().filter(|s| s.is_vex())
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
