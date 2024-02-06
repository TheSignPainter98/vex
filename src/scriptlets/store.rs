use std::{cell::RefCell, collections::BTreeMap, fs, io::ErrorKind, iter};

use camino::Utf8PathBuf;
use dupe::Dupe;
use enum_map::EnumMap;
use log::{info, log_enabled};
use starlark::{environment::FrozenModule, eval::FileLoader};
use strum::IntoEnumIterator;

use crate::{
    context::Context,
    error::Error,
    scriptlets::{
        scriptlet::{InitingScriptlet, PreinitingScriptlet, VexingScriptlet},
        ScriptletObserverData,
    },
    source_path::{PrettyPath, SourcePath},
    supported_language::SupportedLanguage,
};

type StoreIndex = usize;

#[derive(Debug)]
pub struct PreinitingStore {
    dir: Utf8PathBuf,
    path_indices: BTreeMap<PrettyPath, StoreIndex>,
    store: Vec<PreinitingScriptlet>,
}

impl PreinitingStore {
    pub fn new(ctx: &Context) -> anyhow::Result<Self> {
        let mut ret = Self {
            dir: ctx.vex_dir(),
            path_indices: BTreeMap::new(),
            store: Vec::new(),
        };
        ret.load_dir(ctx, ctx.vex_dir(), true)?;
        Ok(ret)
    }

    fn load_dir(&mut self, ctx: &Context, path: Utf8PathBuf, toplevel: bool) -> anyhow::Result<()> {
        let dir = fs::read_dir(&path).map_err(|err| match err.kind() {
            ErrorKind::NotFound => anyhow::Error::from(Error::NoVexesDir(path.clone())),
            _ => err.into(),
        })?;
        for entry in dir {
            let entry = entry?;
            let entry_path = Utf8PathBuf::try_from(entry.path())?;
            let metadata = fs::symlink_metadata(&entry_path)?;

            if metadata.is_symlink() {
                if log_enabled!(log::Level::Info) {
                    let symlink_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                    info!("ignoring /{symlink_path} (symlink)");
                }
                continue;
            }

            if metadata.is_dir() {
                self.load_dir(ctx, entry_path, false)?;
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
            let scriptlet_path = SourcePath::new(&entry_path, &self.dir);
            self.load_file(scriptlet_path, toplevel)?;
        }

        Ok(())
    }

    fn load_file(&mut self, path: SourcePath, toplevel: bool) -> anyhow::Result<()> {
        if self.path_indices.get(&path.pretty_path).is_some() {
            return Ok(());
        }

        let scriptlet = PreinitingScriptlet::new(path.dupe(), toplevel)?;
        self.store.push(scriptlet);
        self.path_indices
            .insert(path.pretty_path.dupe(), self.store.len() - 1);

        Ok(())
    }

    pub fn preinit(mut self) -> anyhow::Result<InitingStore> {
        self.check_loads()?;
        self.sort();
        self.linearise_store()?;

        let Self { store, .. } = self;

        let mut initing_store = Vec::with_capacity(store.len());
        let mut cache = PreinitedModuleCache::new();
        for scriptlet in store.into_iter() {
            let preinited_scriptlet = scriptlet.preinit(&cache)?;
            cache.cache(&preinited_scriptlet);
            initing_store.push(preinited_scriptlet);
        }

        Ok(InitingStore {
            store: initing_store,
        })
    }

    fn check_loads(&self) -> anyhow::Result<()> {
        // TODO(kcza): use relative loads
        let mut unknown_loads = self.store.iter().flat_map(|s| {
            s.loads()
                .iter()
                .filter(|l| self.path_indices.get(l).is_none())
        });
        if let Some(unknown_module) = unknown_loads.next() {
            return Err(Error::NoSuchModule(unknown_module.dupe()).into());
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
    fn linearise_store(&mut self) -> anyhow::Result<()> {
        fn directed_dfs(
            linearised: &mut Vec<StoreIndex>,
            explored: &RefCell<Vec<bool>>,
            loads: &[Vec<StoreIndex>],
            loaded_by: &[Vec<StoreIndex>],
            node: StoreIndex,
        ) {
            {
                let explored = explored.borrow();
                if !loads[node].iter().all(|n| explored[*n]) {
                    return;
                }
            }

            explored.borrow_mut()[node] = true;
            linearised.push(node);
            for m in &loaded_by[node] {
                directed_dfs(linearised, explored, loads, loaded_by, *m);
            }
        }

        let load_edges = self.get_load_edges();
        let loaded_by_edges = self.get_loaded_by_edges(&load_edges);
        let n = self.store.len();
        let explored = RefCell::new(vec![false; n]);
        let mut linearised = Vec::with_capacity(n);
        for node in 0..n {
            if explored.borrow_mut()[node] {
                continue;
            }

            directed_dfs(
                &mut linearised,
                &explored,
                &load_edges,
                &loaded_by_edges,
                node,
            );
        }
        if linearised.len() != self.store.len() {
            // Presence of an import cycle will prevent some nodes entering the
            // linearisation.
            return Err(Error::ImportCycle(self.find_cycle()).into());
        }
        linearised.into_iter().enumerate().for_each(|(i, j)| {
            if i < j {
                self.store.swap(i, j)
            }
        });

        Ok(())
    }

    fn find_cycle(&self) -> Vec<PrettyPath> {
        fn undirected_dfs(
            stack: &RefCell<Vec<StoreIndex>>,
            explored: &RefCell<Vec<bool>>,
            edges: &[Vec<StoreIndex>],
            node: StoreIndex,
        ) -> Option<Vec<StoreIndex>> {
            if stack.borrow().contains(&node) {
                return Some(
                    stack
                        .borrow()
                        .iter()
                        .copied()
                        .skip_while(|n| *n != node)
                        .chain([node])
                        .collect(),
                );
            }

            if explored.borrow()[node] {
                return None;
            }
            explored.borrow_mut()[node] = true;

            stack.borrow_mut().push(node);
            for next in &edges[node] {
                let r = undirected_dfs(stack, explored, edges, *next);
                if r.is_some() {
                    return r;
                }
            }
            stack.borrow_mut().pop();

            None
        }

        let stack = RefCell::new(vec![]);
        let edges = {
            let mut edges = self.get_load_edges();
            self.get_loaded_by_edges(&edges)
                .into_iter()
                .enumerate()
                .for_each(|(n, g)| g.iter().for_each(|m| edges[*m].push(n)));
            edges
        };
        let n = self.store.len();
        let explored = RefCell::new(vec![false; n]);
        let mut cycle = None;
        for node in 0..n {
            let c = undirected_dfs(&stack, &explored, &edges, node);
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
}

impl InitingStore {
    pub fn init(self) -> anyhow::Result<VexingStore> {
        let Self { store } = self;
        let store = store
            .into_iter()
            .map(InitingScriptlet::init)
            .collect::<anyhow::Result<_>>()?;
        Ok(VexingStore { store })
    }

    pub fn vexes(&self) -> impl Iterator<Item = &InitingScriptlet> {
        self.store.iter().filter(|s| s.is_vex())
    }
}

#[derive(Debug)]
pub struct VexingStore {
    #[allow(unused)]
    store: Vec<VexingScriptlet>,
}

impl VexingStore {
    pub fn language_observers(&self) -> EnumMap<SupportedLanguage, Vec<ScriptletObserverData>> {
        let mut result: EnumMap<_, Vec<ScriptletObserverData>> =
            EnumMap::from_iter(SupportedLanguage::iter().map(|s| (s, vec![])));
        self.store
            .iter()
            .flat_map(VexingScriptlet::observer_data)
            .for_each(|h| result[h.lang].push(h.dupe()));
        result
    }
}
