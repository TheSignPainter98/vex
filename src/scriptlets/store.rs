use std::{
    collections::{BTreeMap, HashSet},
    fs,
    marker::PhantomData,
};

use camino::Utf8PathBuf;
use log::{info, log_enabled};
use starlark::{environment::FrozenModule, eval::FileLoader};

use crate::{
    context::Context,
    error::Error,
    scriptlets::{
        stage::{Initing, Preiniting, Vexing},
        Scriptlet, Stage,
    },
};

pub struct Store<S: Stage> {
    dir: Utf8PathBuf,
    path_indices: BTreeMap<Utf8PathBuf, usize>,
    toplevel: Vec<usize>,

    /// Scriptlets stored in topographic order.
    store: Vec<Scriptlet<S>>,
}

impl Store<Preiniting> {
    pub fn new(ctx: &Context) -> anyhow::Result<Self> {
        let mut ret = Self {
            dir: ctx.vex_dir(),
            path_indices: BTreeMap::new(),
            toplevel: Vec::new(),
            store: Vec::new(),
        };
        ret.load_recursively(ctx, ctx.vex_dir(), true)?;
        Ok(ret)
    }

    fn load_recursively(
        &mut self,
        ctx: &Context,
        path: Utf8PathBuf,
        toplevel: bool,
    ) -> anyhow::Result<()> {
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
                return self.load_recursively(ctx, entry_path, false);
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
            if toplevel {
                self.load_toplevel(entry_path)?;
            } else {
                self.load(entry_path)?;
            }
        }

        Ok(())
    }

    fn load_toplevel(&mut self, path: Utf8PathBuf) -> anyhow::Result<()> {
        println!("loading toplevel: {path}");

        if let Some(new_idx) = self.load(path.clone())? {
            self.toplevel.push(new_idx);
        }
        Ok(())
    }

    fn load(&mut self, path: Utf8PathBuf) -> anyhow::Result<Option<usize>> {
        if self.path_indices.get(&path).is_some() {
            return Ok(None);
        }

        let scriptlet = Scriptlet::new(&path)?;
        let midst_idx_lim = self
            .store
            .iter()
            .enumerate()
            .filter_map(|(i, s)| if s.loads(&scriptlet) { Some(i) } else { None })
            .next();
        let new_idx = midst_idx_lim.unwrap_or(self.store.len());
        self.store.insert(new_idx, scriptlet); // Insert in topographic order.
        if let Some(midst_insertion_idx) = midst_idx_lim {
            self.toplevel.iter_mut().for_each(|idx| {
                if *idx >= midst_insertion_idx {
                    *idx += 1;
                }
            });
            self.path_indices.values_mut().for_each(|idx| {
                if *idx >= midst_insertion_idx {
                    *idx += 1;
                }
            });
        }
        self.path_indices.insert(path, new_idx);
        Ok(Some(new_idx))
    }

    pub fn preinit(self) -> anyhow::Result<Store<Initing>> {
        self.validate_imports()?;

        let mut store = Vec::with_capacity(self.store.len());
        for scriptlet in self.store {
            let loader = ScriptletExports::new(&store);
            store.push(scriptlet.preinit(loader)?);
        }

        Ok(Store {
            dir: self.dir,
            path_indices: self.path_indices,
            toplevel: self.toplevel,
            store,
        })
    }

    fn validate_imports(&self) -> anyhow::Result<()> {
        let known_files: HashSet<_> = self.store.iter().map(|s| &s.path).collect();
        self.store
            .iter()
            .flat_map(|s| s.loads_files.iter())
            .map(|load| {
                known_files
                    .get(load)
                    .ok_or_else(|| {
                        anyhow::Error::from(Error::UnknownModule {
                            vexes_dir: self.dir.clone(),
                            requested: load.to_path_buf(),
                        })
                    })
                    .map(|_| ())
            })
            .collect()
    }
}

pub struct ScriptletExports<'s> {
    exports: BTreeMap<&'s str, &'s FrozenModule>,
}

impl<'s> ScriptletExports<'s> {
    fn new(store: &'s [Scriptlet<Initing>]) -> Self {
        Self {
            exports: store
                .iter()
                .map(|s| (s.path.as_str(), s.module.as_frozen().unwrap()))
                .collect(),
        }
    }
}

impl<'s> FileLoader for ScriptletExports<'s> {
    fn load(&self, module_path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
        let Some(module) = self.exports.get(module_path) else {
            todo!("compute cycles");
        };
        #[allow(suspicious_double_ref_op)]
        Ok(module.clone().clone())
    }
}

impl Store<Initing> {
    pub fn init(self) -> anyhow::Result<Store<Vexing>> {
        let store = self
            .store
            .into_iter()
            .map(Scriptlet::init)
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Store {
            dir: self.dir,
            path_indices: self.path_indices,
            toplevel: self.toplevel,
            store,
        })
    }
}

impl Store<Vexing> {
    pub fn toplevel(&self) -> impl Iterator<Item = ScriptletRef<'_, Vexing>> {
        self.toplevel.iter().map(|index| ScriptletRef {
            index: *index,
            _store: PhantomData,
        })
    }
}

pub struct ScriptletRef<'s, S: Stage> {
    #[allow(unused)]
    index: usize,
    _store: PhantomData<&'s Store<S>>,
}

impl<'s, S: Stage> ScriptletRef<'s, S> {
    #[allow(unused)]
    fn new(index: usize) -> Self {
        Self {
            index,
            _store: PhantomData,
        }
    }
}

// impl<'s, S: Stage> Deref for ScriptletRef<'s, S> {
//     type Target = Scriptlet<S>;
//
//     fn deref(&self) -> &Self::Target {
//
//     }
// }
