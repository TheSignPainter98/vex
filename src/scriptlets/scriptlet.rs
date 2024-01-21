use std::{collections::HashSet, fs};

use camino::{Utf8Path, Utf8PathBuf};
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::Evaluator,
    syntax::{AstModule, Dialect},
    PrintHandler,
};

use crate::{
    error::Error,
    scriptlets::{
        action::Action,
        app_object::AppObject,
        extra_data::{FrozenHandlerDataBuilder, HandlerDataBuilder, InvocationData},
        store::ScriptletExports,
        ScriptletHandlerData,
    },
};

pub struct PreinitingScriptlet {
    pub path: Utf8PathBuf,
    toplevel: bool,
    ast: AstModule,
    loads_files: HashSet<Utf8PathBuf>,
}

impl PreinitingScriptlet {
    pub fn new(path: Utf8PathBuf, toplevel: bool) -> anyhow::Result<Self> {
        let code = fs::read_to_string(&path)?;
        Self::new_from_str(path, code, toplevel)
    }

    fn new_from_str(path: Utf8PathBuf, code: String, toplevel: bool) -> anyhow::Result<Self> {
        let ast = AstModule::parse(path.as_str(), code, &Dialect::Standard)?;
        let loads_files = ast
            .loads()
            .into_iter()
            .map(|load| Utf8PathBuf::from(load.module_id))
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

    pub fn preinit(self, store: &ScriptletExports) -> anyhow::Result<InitingScriptlet> {
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
                let print_handler = StdoutPrintHandler { path: &path };
                let mut eval = Evaluator::new(&module);
                eval.set_loader(&store);
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

    pub fn loads(&self) -> &HashSet<Utf8PathBuf> {
        &self.loads_files
    }
}

pub struct InitingScriptlet {
    pub path: Utf8PathBuf,
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
                return Err(Error::NoInit(path).into());
            }
            // Non-toplevel scriptlets may be helper libraries.
            return Ok(VexingScriptlet {
                path,
                preinited_module,
                inited_module: None,
                handler_data: None,
            });
        };

        let inited_module = {
            let module = Module::new();
            HandlerDataBuilder::new().insert_into(&module);
            {
                let extra = InvocationData::new(Action::Initing);
                let print_handler = StdoutPrintHandler { path: &path };
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&print_handler);
                extra.insert_into(&mut eval);
                eval.eval_function(init.value(), &[], &[])?;
            }
            module.freeze()?
        };
        let handler_data = FrozenHandlerDataBuilder::get_from(&inited_module).build(&path)?;

        Ok(VexingScriptlet {
            path,
            preinited_module,
            inited_module: Some(inited_module),
            handler_data: Some(handler_data),
        })
    }

    pub fn is_vex(&self) -> bool {
        self.preinited_module
            .get_option("init")
            .is_ok_and(|o| o.is_some())
    }
}

pub struct VexingScriptlet {
    pub path: Utf8PathBuf,
    #[allow(unused)]
    preinited_module: FrozenModule, // Keep frozen heap alive
    #[allow(unused)]
    inited_module: Option<FrozenModule>, // Keep frozen heap alive
    handler_data: Option<ScriptletHandlerData>,
}

impl VexingScriptlet {
    pub fn handler_data(&self) -> Option<&ScriptletHandlerData> {
        self.handler_data.as_ref()
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
