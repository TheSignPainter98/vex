use std::{cell::RefCell, collections::BTreeMap, fs, iter};

use camino::{Utf8Path, Utf8PathBuf};
use dupe::Dupe;
use log::{info, log_enabled};
use starlark::{environment::FrozenModule, eval::FileLoader};

use crate::{
    context::Context,
    error::Error,
    scriptlets::{scriptlet::InitingScriptlet, PreinitingScriptlet, VexingScriptlet},
};

pub struct PreinitingStore {
    dir: Utf8PathBuf,
    path_indices: BTreeMap<Utf8PathBuf, usize>,
    store: Vec<PreinitingScriptlet>,
}

impl PreinitingStore {
    pub fn new(ctx: &Context) -> anyhow::Result<Self> {
        let mut ret = Self {
            dir: ctx.vex_dir(),
            path_indices: BTreeMap::new(),
            store: Vec::new(),
        };
        ret.load_dir(ctx, ctx.vex_dir())?;
        Ok(ret)
    }

    fn load_dir(&mut self, ctx: &Context, path: Utf8PathBuf) -> anyhow::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = Utf8PathBuf::try_from(entry.path())?;
            let metadata = fs::symlink_metadata(&entry_path)?;

            if metadata.is_symlink() {
                if log_enabled!(log::Level::Info) {
                    let symlink_path = entry_path.strip_prefix(&ctx.project_root)?;
                    info!("ignoring /{symlink_path} (symlink)");
                }
                continue;
            }

            if metadata.is_dir() {
                return self.load_dir(ctx, entry_path);
            }

            if !metadata.is_file() {
                panic!("unreachable");
            }
            if !entry_path.extension().is_some_and(|ext| ext == "star") {
                if log_enabled!(log::Level::Info) {
                    let unknown_path = entry_path.strip_prefix(&ctx.project_root)?;
                    info!("ignoring /{unknown_path} (expected `.star` extension)");
                }
                continue;
            }
            self.load_file(entry_path)?;
        }

        Ok(())
    }

    fn load_file(&mut self, path: Utf8PathBuf) -> anyhow::Result<()> {
        let stripped_path = path.strip_prefix(&self.dir)?;
        if self.path_indices.get(stripped_path).is_some() {
            return Ok(());
        }

        let scriptlet = PreinitingScriptlet::new(path.clone())?;
        self.store.push(scriptlet);
        self.path_indices
            .insert(stripped_path.into(), self.store.len() - 1);

        Ok(())
    }

    pub fn preinit(mut self) -> anyhow::Result<InitingStore> {
        self.sort();
        self.linearise_store()?;

        let Self { dir, store, .. } = self;

        let mut initing_store = Vec::with_capacity(store.len());
        let mut loader = ScriptletExports {
            exports: BTreeMap::new(),
        };
        for scriptlet in store.into_iter() {
            let preinited_scriptlet = scriptlet.preinit(&loader)?;
            loader.insert(&dir, &preinited_scriptlet);
            initing_store.push(preinited_scriptlet);
        }

        Ok(InitingStore {
            store: initing_store,
        })
    }

    fn sort(&mut self) {
        self.store.sort_by(|s, t| s.path.cmp(&t.path));
        self.path_indices = self
            .store
            .iter()
            .enumerate()
            .map(|(i, s)| (s.path.strip_prefix(&self.dir).unwrap().to_path_buf(), i))
            .collect();
    }

    /// Topographically order the store
    fn linearise_store(&mut self) -> anyhow::Result<()> {
        type StoreIndex = usize;

        fn dfs<'s>(
            linearised: &mut Vec<StoreIndex>,
            stack: &RefCell<Vec<StoreIndex>>,
            explored: &RefCell<Vec<bool>>,
            edges_in: &[Vec<StoreIndex>],
            edges_out: &[Vec<StoreIndex>],
            node: StoreIndex,
            store: &[PreinitingScriptlet],
            dir: &Utf8Path,
        ) -> anyhow::Result<()> {
            {
                let explored = explored.borrow();
                if !edges_in[node].iter().all(|n| explored[*n]) {
                    return Ok(());
                }
            }

            if stack.borrow().contains(&node) {
                let cycle = stack
                    .borrow()
                    .iter()
                    .map(|idx| store[*idx].path.clone())
                    .collect();
                return Err(Error::ImportCycle(cycle).into());
            }
            stack.borrow_mut().push(node);

            explored.borrow_mut()[node] = true;
            linearised.push(node);
            for m in &edges_out[node] {
                dfs(
                    linearised, stack, explored, edges_in, edges_out, *m, store, dir,
                )?;
            }

            Ok(())
        }

        let n = self.store.len();
        let stack = RefCell::new(vec![]);
        let edges_in: Vec<Vec<StoreIndex>> = self
            .store
            .iter()
            .map(|s| {
                let mut g = s
                    .loads()
                    .iter()
                    .map(|m| *self.path_indices.get(m).unwrap())
                    .collect::<Vec<_>>();
                g.sort();
                g
            })
            .collect();
        let edges_out: Vec<Vec<StoreIndex>> = {
            let mut edges_out = iter::repeat_with(|| vec![]).take(n).collect::<Vec<_>>();
            edges_in
                .iter()
                .enumerate()
                .rev()
                .for_each(|(n, g)| g.iter().for_each(|m| edges_out[*m].push(n)));
            edges_out
        };
        let explored = RefCell::new(vec![false; n]);
        let mut linearised = Vec::with_capacity(n);
        for node in 0..n {
            if explored.borrow_mut()[node] {
                continue;
            }

            dfs(
                &mut linearised,
                &stack,
                &explored,
                &edges_in,
                &edges_out,
                node,
                &self.store,
                &self.dir,
            )?;
        }
        linearised.into_iter().enumerate().for_each(|(i, j)| {
            if i < j {
                self.store.swap(i, j)
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct ScriptletExports {
    exports: BTreeMap<String, FrozenModule>,
}

impl ScriptletExports {
    fn insert(&mut self, dir: &Utf8Path, scriptlet: &InitingScriptlet) {
        self.exports.insert(
            scriptlet.path.strip_prefix(dir).unwrap().to_string(),
            scriptlet.preinited_module.dupe(),
        );
    }
}

impl FileLoader for &ScriptletExports {
    fn load(&self, module_path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
        self.exports
            .get(module_path)
            .map(Dupe::dupe)
            .ok_or_else(|| Error::UnknownModule(module_path.into()).into())
    }
}

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

pub struct VexingStore {
    store: Vec<VexingScriptlet>,
}
