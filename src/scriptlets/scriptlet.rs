use std::{collections::HashSet, fmt::Display, fs, sync::Arc};

use anyhow::Context;
use camino::Utf8Path;
use dupe::Dupe;
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::Evaluator,
    syntax::{AstModule, Dialect},
};

use crate::{
    error::Error,
    scriptlets::{
        action::Action,
        app_object::AppObject,
        extra_data::{FrozenObserverDataBuilder, InvocationData, ObserverDataBuilder},
        print_handler::PrintHandler,
        store::PreinitedModuleCache,
        ScriptletObserverData,
    },
};

pub struct PreinitingScriptlet {
    pub path: ScriptletPath,
    toplevel: bool,
    ast: AstModule,
    loads_files: HashSet<PrettyPath>,
}

impl PreinitingScriptlet {
    pub fn new(path: ScriptletPath, toplevel: bool) -> anyhow::Result<Self> {
        let code = fs::read_to_string(&path.abs_path.as_str())
            .with_context(|| format!("could not read {path}"))?;
        Self::new_from_str(path, code, toplevel)
    }

    fn new_from_str(path: ScriptletPath, code: String, toplevel: bool) -> anyhow::Result<Self> {
        println!("path is {path}");
        let ast = AstModule::parse(path.as_str(), code, &Dialect::Standard)?;
        let loads_files = ast
            .loads()
            .into_iter()
            .map(|load| PrettyPath::from(load.module_id))
            .collect();
        Ok(Self {
            path,
            toplevel,
            ast,
            loads_files,
        })
    }

    #[allow(unused)]
    pub fn lint(&self) -> Vec<Lint> {
        self.ast.lint(Some(&self.global_names()))
    }

    pub fn preinit(self, cache: &PreinitedModuleCache) -> anyhow::Result<InitingScriptlet> {
        let Self {
            path,
            ast,
            toplevel,
            loads_files: _,
        } = self;

        let preinited_module = {
            let module = Module::new();
            {
                let extra = InvocationData::new(Action::Preiniting);
                let print_handler = PrintHandler::new(&path);
                let mut eval = Evaluator::new(&module);
                eval.set_loader(&cache);
                eval.set_print_handler(&print_handler);
                extra.insert_into(&mut eval);
                let globals = Self::globals();
                eval.eval_module(ast, &globals)?;
            }
            module.freeze()?
        };
        Ok(InitingScriptlet {
            path,
            toplevel,
            preinited_module,
        })
    }

    fn globals() -> Globals {
        let mut builder = GlobalsBuilder::extended_by(&[LibraryExtension::Print]);
        builder.set(AppObject::NAME, builder.alloc(AppObject));
        builder.build()
    }

    fn global_names(&self) -> HashSet<String> {
        HashSet::from_iter(["vex".to_string()].into_iter())
    }

    pub fn loads(&self) -> &HashSet<PrettyPath> {
        &self.loads_files
    }
}

#[derive(Clone, Debug, Dupe)]
pub struct ScriptletPath {
    abs_path: Arc<Utf8Path>,
    pub pretty_path: PrettyPath,
}

impl ScriptletPath {
    pub fn new(path: &Utf8Path, vex_dir: &Utf8Path) -> Self {
        Self {
            abs_path: path.into(),
            pretty_path: PrettyPath(
                path.strip_prefix(vex_dir)
                    .expect("vex not in vexes dir")
                    .into(),
            ),
        }
    }

    pub fn as_str(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl AsRef<str> for ScriptletPath {
    fn as_ref(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl Display for ScriptletPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty_path.fmt(f)
    }
}

#[derive(Clone, Debug, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrettyPath(Arc<Utf8Path>);

impl PrettyPath {
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl From<&str> for PrettyPath {
    fn from(value: &str) -> Self {
        Self(Utf8Path::new(value).into())
    }
}

impl AsRef<str> for PrettyPath {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for PrettyPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub struct InitingScriptlet {
    pub path: ScriptletPath,
    toplevel: bool,
    pub preinited_module: FrozenModule,
}

impl InitingScriptlet {
    pub fn init(self) -> anyhow::Result<VexingScriptlet> {
        let Self {
            path,
            toplevel,
            preinited_module,
        } = self;

        let Some(init) = preinited_module.get_option("init")? else {
            if toplevel {
                return Err(Error::NoInit(path.pretty_path.dupe()).into());
            }
            // Non-toplevel scriptlets may be helper libraries.
            return Ok(VexingScriptlet {
                path,
                _preinited_module: preinited_module,
                _inited_module: None,
                observer_data: None,
            });
        };

        let inited_module = {
            let module = Module::new();
            ObserverDataBuilder::new().insert_into(&module);
            {
                let extra = InvocationData::new(Action::Initing);
                let print_handler = PrintHandler::new(&path);
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&print_handler);
                extra.insert_into(&mut eval);
                eval.eval_function(init.value(), &[], &[])?;
            }
            module.freeze()?
        };
        let observer_data = FrozenObserverDataBuilder::get_from(&inited_module).build(&path)?;

        Ok(VexingScriptlet {
            path,
            _preinited_module: preinited_module,
            _inited_module: Some(inited_module),
            observer_data: Some(observer_data),
        })
    }

    pub fn is_vex(&self) -> bool {
        self.preinited_module
            .get_option("init")
            .is_ok_and(|o| o.is_some())
    }
}

pub struct VexingScriptlet {
    pub path: ScriptletPath,
    _preinited_module: FrozenModule,      // Keep frozen heap alive
    _inited_module: Option<FrozenModule>, // Keep frozen heap alive
    observer_data: Option<ScriptletObserverData>,
}

impl VexingScriptlet {
    pub fn observer_data(&self) -> Option<&ScriptletObserverData> {
        self.observer_data.as_ref()
    }
}

#[cfg(test)]
mod test {
    use starlark::values::FrozenStringValue;

    use super::*;

    #[test]
    fn global_names_consistent() {
        let scriptlet = Scriptlet::new_from_str(Utf8Path::new("consistency.star"), "a = 1")?;
        let global_names =
            HashSet::from_iter(scriptlet.globals().names().map(FrozenStringValue::borrow));
        assert_eq!(scriptlet.global_names(), global_names);
    }
}
