use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Mutex, MutexGuard},
};

use camino::{Utf8Path, Utf8PathBuf};
use log::{info, log_enabled};
use starlark::values::FrozenHeap;

use crate::{
    error::Error,
    result::Result,
    scriptlets::{
        scriptlet::{InitingScriptlet, PreinitingScriptlet},
        source::ScriptSource,
        ObserverData,
    },
    source_path::PrettyPath,
    verbosity::Verbosity,
};

#[derive(Debug)]
pub struct PreinitingStore {
    store: Vec<PreinitingScriptlet>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct StoreIndex(usize);

impl PreinitingStore {
    pub fn new<S: ScriptSource>(scripts: &[S]) -> Result<Self> {
        let store: Vec<_> = scripts
            .iter()
            .map(|source| Result::Ok((source.path(), source.content()?)))
            .inspect(|content_result| {
                if let Err(err) = content_result {
                    if log_enabled!(log::Level::Info) {
                        info!("{err}");
                    }
                }
            })
            .flatten()
            .map(|(path, content)| PreinitingScriptlet::new(path.to_owned(), content))
            .collect::<Result<_>>()?;
        Ok(Self { store })
    }

    pub fn preinit(mut self, opts: PreinitOptions) -> Result<InitingStore> {
        self.store.sort_by(|sc1, sc2| sc1.path.cmp(&sc2.path));
        self.topographic_sort()?;
        let Self { store } = self;

        let frozen_heap = FrozenHeap::new();
        let mut partial_store = PreinitedModuleStore::new();
        for scriptlet in store.into_iter() {
            let preinited_scriptlet = scriptlet.preinit(&opts, &partial_store, &frozen_heap)?;
            partial_store.add(preinited_scriptlet);
        }

        let store = partial_store.into_entry_modules().collect();
        Ok(InitingStore { store, frozen_heap })
    }

    /// Topographically order the store
    fn topographic_sort(&mut self) -> Result<()> {
        fn directed_dfs(
            linearised: &mut Vec<StoreIndex>,
            explored: &mut [bool],
            loads: &[Vec<StoreIndex>],
            loaded_by: &[Vec<StoreIndex>],
            node: StoreIndex,
        ) {
            if !loads[node.0].iter().all(|n| explored[n.0]) {
                return;
            }

            explored[node.0] = true;
            linearised.push(node);
            for m in &loaded_by[node.0] {
                directed_dfs(linearised, explored, loads, loaded_by, *m);
            }
        }

        let load_edges = self.get_load_edges();
        let loaded_by_edges = self.get_loaded_by_edges(&load_edges);
        let n = self.store.len();
        let mut explored = vec![false; n];
        let mut linearised = Vec::with_capacity(n);
        for node in (0..n).map(StoreIndex) {
            if explored[node.0] {
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
            .filter(|(i, j)| *i < j.0)
            .for_each(|(i, j)| self.store.swap(i, j.0));

        Ok(())
    }

    fn find_cycle(&self) -> Vec<PrettyPath> {
        fn undirected_dfs(
            stack: &mut Vec<StoreIndex>,
            explored: &mut [bool],
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

            if explored[node.0] {
                return None;
            }
            explored[node.0] = true;

            stack.push(node);
            for next in &edges[node.0] {
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
                .for_each(|(n, g)| g.into_iter().for_each(|m| edges[m.0].push(StoreIndex(n))));
            edges
        };
        let n = self.store.len();
        let mut explored = vec![false; n];
        let mut cycle = None;
        for node in (0..n).map(StoreIndex) {
            let c = undirected_dfs(&mut stack, &mut explored, &edges, node);
            if c.is_some() {
                cycle = c;
                break;
            }
        }
        cycle
            .unwrap()
            .into_iter()
            .map(|idx| PrettyPath::new(&self.store[idx.0].path))
            .collect()
    }

    fn get_load_edges(&self) -> Vec<Vec<StoreIndex>> {
        let script_indices_by_path: BTreeMap<_, _> = self
            .store
            .iter()
            .enumerate()
            .map(|(idx, script)| (script.path.as_path(), StoreIndex(idx)))
            .collect();
        self.store
            .iter()
            .map(|script| {
                script
                    .loads()
                    .values()
                    .flat_map(|load| script_indices_by_path.get(load.path()).copied())
                    .collect()
            })
            .collect()
    }

    fn get_loaded_by_edges(&self, load_edges: &[Vec<StoreIndex>]) -> Vec<Vec<StoreIndex>> {
        let mut ret = vec![Vec::new(); load_edges.len()];
        for (idx, loads) in load_edges.iter().enumerate() {
            for load_idx in loads {
                ret[load_idx.0].push(StoreIndex(idx));
            }
        }
        ret
    }
}

#[derive(Debug, Default)]
pub struct PreinitOptions {
    pub lenient: bool,
    pub verbosity: Verbosity,
}

#[derive(Debug)]
pub struct PreinitedModuleStore {
    pub entries: BTreeMap<Utf8PathBuf, InitingScriptlet>,
}

impl PreinitedModuleStore {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    fn add(&mut self, scriptlet: InitingScriptlet) {
        self.entries.insert(scriptlet.path.to_owned(), scriptlet);
    }

    pub fn into_entry_modules(self) -> impl Iterator<Item = InitingScriptlet> {
        self.entries.into_values()
    }

    pub fn get(&self, path: &Utf8Path) -> Option<&InitingScriptlet> {
        self.entries.get(path)
    }
}

#[derive(Debug)]
pub struct InitingStore {
    store: Vec<InitingScriptlet>,
    frozen_heap: FrozenHeap,
}

impl InitingStore {
    pub fn init(self, opts: InitOptions) -> Result<VexingStore> {
        let Self { store, frozen_heap } = self;
        let num_scripts = store.len();

        let observer_data = store.into_iter().try_fold(
            ObserverData::with_capacity(4 * num_scripts),
            |mut data, scriptlet| {
                data.extend(scriptlet.init(&opts, &frozen_heap)?);
                Result::Ok(data)
            },
        )?;

        let frozen_heap = Mutex::new(frozen_heap);
        Ok(VexingStore {
            num_scripts,
            observer_data,
            frozen_heap,
        })
    }
}

#[derive(Debug, Default)]
pub struct InitOptions {
    pub verbosity: Verbosity,
}

#[derive(Debug)]
pub struct VexingStore {
    num_scripts: usize,
    observer_data: ObserverData,
    frozen_heap: Mutex<FrozenHeap>,
}

impl VexingStore {
    pub fn frozen_heap(&self) -> MutexGuard<'_, FrozenHeap> {
        self.frozen_heap.lock().expect("frozen heap lock poisoned")
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
