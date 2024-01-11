use std::{collections::HashSet, fs, marker::PhantomData};

use camino::{Utf8Path, Utf8PathBuf};
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::Evaluator,
    syntax::{AstModule, Dialect},
    values::FrozenValueTyped,
    PrintHandler,
};

use crate::{
    error::Error,
    scriptlets::{
        app_object::AppObject,
        stage::{Initing, Preiniting, Vexing},
        store::ScriptletExports,
        Stage,
    },
};

pub struct Scriptlet<S: Stage> {
    pub path: Utf8PathBuf,
    pub module: ScriptletModule,
    pub loads_files: HashSet<Utf8PathBuf>,
    state: PhantomData<S>,
}

impl Scriptlet<Preiniting> {
    pub fn new(path: impl Into<Utf8PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let code = fs::read_to_string(&path)?;
        Self::new_from_str(path, code)
    }

    fn new_from_str(path: impl Into<Utf8PathBuf>, code: impl Into<String>) -> anyhow::Result<Self> {
        let path = path.into();
        let code = code.into();

        let ast = AstModule::parse(path.as_str(), code, &Dialect::Standard)?;
        let loads_files = ast
            .loads()
            .into_iter()
            .map(|load| Utf8PathBuf::from(load.module_id))
            .collect();
        Ok(Self {
            path,
            module: ast.into(),
            loads_files,
            state: PhantomData,
        })
    }

    #[allow(unused)]
    pub fn lint(&self) -> Vec<Lint> {
        self.module
            .as_ast()
            .unwrap()
            .lint(Some(&self.global_names()))
    }

    pub fn preinit(self, store: ScriptletExports) -> anyhow::Result<Scriptlet<Initing>> {
        let module = {
            let module = Module::new();
            {
                let print_handler = StdoutPrintHandler { path: &self.path };
                let mut eval = Evaluator::new(&module);
                eval.set_loader(&store);
                eval.set_print_handler(&print_handler);
                let globals = self.globals();
                let ast = self.module.ast().expect("module not ast");
                eval.eval_module(ast, &globals)?;
            }
            module.freeze()?
        };

        Ok(Scriptlet {
            path: self.path,
            module: module.into(),
            loads_files: self.loads_files,
            state: PhantomData,
        })
    }

    pub fn loads(&self, other: &Scriptlet<Preiniting>) -> bool {
        self.loads_files.contains(&other.path)
    }
}

impl Scriptlet<Initing> {
    pub fn init(self) -> anyhow::Result<Scriptlet<Vexing>> {
        println!(
            "{:?}",
            self.module
                .as_frozen()
                .unwrap()
                .names()
                .map(FrozenValueTyped::as_str)
                .collect::<Vec<_>>()
        );
        let Some(init) = self
            .module
            .frozen()
            .expect("inited module not frozen")
            .get_option("init")?
        else {
            return Err(Error::NoInit(self.path).into());
        };

        let blank_module = Module::new();
        let print_handler = StdoutPrintHandler { path: &self.path };
        let mut eval = Evaluator::new(&blank_module);
        eval.set_print_handler(&print_handler);
        eval.eval_function(init.value(), &[], &[])?;

        todo!();
    }
}

impl<S: Stage> Scriptlet<S> {
    fn app_object(&self) -> AppObject {
        AppObject::new::<S>()
    }

    fn globals(&self) -> Globals {
        let mut builder = GlobalsBuilder::extended_by(&[LibraryExtension::Print]);
        builder.set(AppObject::NAME, builder.alloc(self.app_object()));
        builder.build()
    }

    fn global_names(&self) -> HashSet<String> {
        HashSet::from_iter(["vex".to_string()].into_iter())
    }
}

pub enum ScriptletModule {
    Ast(AstModule),
    Frozen(FrozenModule),
}

impl ScriptletModule {
    pub fn ast(self) -> Option<AstModule> {
        match self {
            Self::Ast(ast) => Some(ast),
            _ => None,
        }
    }

    pub fn as_ast(&self) -> Option<&AstModule> {
        match self {
            Self::Ast(ast) => Some(ast),
            _ => None,
        }
    }

    pub fn frozen(self) -> Option<FrozenModule> {
        match self {
            Self::Frozen(frozen) => Some(frozen),
            _ => None,
        }
    }

    pub fn as_frozen(&self) -> Option<&FrozenModule> {
        match self {
            Self::Frozen(frozen) => Some(frozen),
            _ => None,
        }
    }
}

impl From<AstModule> for ScriptletModule {
    fn from(ast: AstModule) -> Self {
        Self::Ast(ast)
    }
}

impl From<FrozenModule> for ScriptletModule {
    fn from(frozen: FrozenModule) -> Self {
        Self::Frozen(frozen)
    }
}

struct StdoutPrintHandler<'a> {
    path: &'a Utf8Path,
}

impl<'a> PrintHandler for StdoutPrintHandler<'a> {
    fn println(&self, text: &str) -> anyhow::Result<()> {
        println!("{}: {text}", self.path);
        Ok(())
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
