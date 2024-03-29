use std::{collections::HashSet, fs};

use dupe::Dupe;
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::Evaluator,
    syntax::{AstModule, Dialect},
};

use crate::{
    error::{Error, IOAction},
    result::Result,
    scriptlets::{
        action::Action,
        app_object::AppObject,
        extra_data::{FrozenObserverDataBuilder, InvocationData, ObserverDataBuilder},
        print_handler::PrintHandler,
        store::PreinitedModuleCache,
        ScriptletObserverData,
    },
    source_path::{PrettyPath, SourcePath},
};

#[derive(Debug)]
pub struct PreinitingScriptlet {
    pub path: SourcePath,
    toplevel: bool,
    ast: AstModule,
    loads_files: HashSet<PrettyPath>,
}

impl PreinitingScriptlet {
    pub fn new(path: SourcePath, toplevel: bool) -> Result<Self> {
        let code = fs::read_to_string(path.abs_path.as_str()).map_err(|cause| Error::IO {
            path: path.pretty_path.dupe(),
            action: IOAction::Read,
            cause,
        })?;
        Self::new_from_str(path, code, toplevel)
    }

    fn new_from_str(path: SourcePath, code: String, toplevel: bool) -> Result<Self> {
        let ast =
            AstModule::parse(path.as_str(), code, &Dialect::Standard).map_err(Error::starlark)?;
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

    // #[allow(unused)]
    // pub fn typecheck(&self, globals: &Globals, ...) -> Result<()> {
    // // TODO(kcza): typecheck starlark before executing it!
    // }

    pub fn preinit(self, cache: &PreinitedModuleCache) -> Result<InitingScriptlet> {
        let Self {
            path,
            ast,
            toplevel,
            loads_files: _,
        } = self;

        let preinited_module = {
            let module = Module::new();
            {
                let extra = InvocationData::new(Action::Preiniting, path.pretty_path.dupe());
                let mut eval = Evaluator::new(&module);
                eval.set_loader(&cache);
                eval.set_print_handler(&PrintHandler);
                extra.insert_into(&mut eval);
                let globals = Self::globals();
                eval.eval_module(ast, &globals).map_err(Error::starlark)?;
            }
            module.freeze().map_err(Error::starlark)?
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
        HashSet::from_iter(["vex".to_string()])
    }

    pub fn loads(&self) -> &HashSet<PrettyPath> {
        &self.loads_files
    }
}

#[derive(Debug)]
pub struct InitingScriptlet {
    pub path: SourcePath,
    toplevel: bool,
    pub preinited_module: FrozenModule,
}

impl InitingScriptlet {
    pub fn init(self) -> Result<VexingScriptlet> {
        let Self {
            path,
            toplevel,
            preinited_module,
        } = self;

        let Some(init) = preinited_module
            .get_option("init")
            .map_err(Error::starlark)?
        else {
            if toplevel {
                return Err(Error::NoInit(path.pretty_path.dupe()));
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
            ObserverDataBuilder::new(path.pretty_path.dupe()).insert_into(&module);
            {
                let extra = InvocationData::new(Action::Initing, path.pretty_path.dupe());
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&PrintHandler);
                extra.insert_into(&mut eval);
                eval.eval_function(init.value(), &[], &[])
                    .map_err(Error::starlark)?;
            }
            module.freeze().map_err(Error::starlark)?
        };
        let observer_data = FrozenObserverDataBuilder::get_from(&inited_module).build()?;

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

#[derive(Debug)]
pub struct VexingScriptlet {
    pub path: SourcePath,
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
    use indoc::{formatdoc, indoc};

    use crate::vextest::VexTest;

    #[test]
    fn global_names_consistent() {
        // TODO(kcza): complete me once linting is added!
        // let scriptlet = PreinitingScriptlet::new_from_str(
        //     Utf8PathBuf::from("consistency.star"),
        //     "".into(),
        //     true,
        // )
        // .unwrap();
        // let global_names = HashSet::from_iter(
        //     PreinitingScriptlet::globals()
        //         .names()
        //         .map(|n| n.to_string()),
        // );
        // assert_eq!(scriptlet.global_names(), global_names);
        // TODO(kcza): complete me once linting is added!
    }

    #[test]
    fn syntax_error() {
        VexTest::new("incomplete-binary")
            .with_scriptlet("vexes/test.star", "x+")
            .try_run()
            .expect_err("unexpected success");
    }

    #[test]
    fn missing_init() {
        VexTest::new("no-init")
            .with_scriptlet("vexes/test.star", "")
            .returns_error(r"test\.star declares no init function")
    }

    #[test]
    fn missing_declarations() {
        VexTest::new("no-language")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        pass
                "#},
            )
            .returns_error(r"test\.star declares no target language");
        VexTest::new("no-query")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('rust')
                "#},
            )
            .returns_error(r"test\.star declares no query");
        VexTest::new("no-callbacks")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression)')
                "#},
            )
            .returns_error(r"test\.star declares no callbacks");
    }

    #[test]
    fn unknown_language() {
        VexTest::new("unknown-language")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('brainfuck')
                        vex.query('(binary_expression)')
                        vex.observe('match', on_match)

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error("unknown language 'brainfuck'")
    }

    #[test]
    fn malformed_query() {
        VexTest::new("empty")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('')
                        vex.observe('match', on_match)

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error(r"test\.star declares empty query");
        VexTest::new("syntax-error")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression') # Missing closing bracket
                        vex.observe('match', on_match)

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error("Invalid syntax");
    }

    #[test]
    fn unknown_event() {
        VexTest::new("unknown-event")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression)')
                        vex.observe('smissmass', on_smissmass)

                    def on_smissmass(event):
                        pass
                "#},
            )
            .returns_error("unknown event 'smissmass'");
    }

    #[test]
    fn app_object_attr_availability() {
        let assert_unavailable_preiniting = |name, call| {
            VexTest::new(format!("preiniting-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        {call}

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression)')
                            vex.observe('match', print)
                    "#},
                )
                .returns_error(format!("{name} unavailable while preiniting"));
        };
        assert_unavailable_preiniting("vex.language", "vex.language('rust')");
        assert_unavailable_preiniting("vex.query", "vex.query('(binary_expression)')");
        assert_unavailable_preiniting("vex.observe", "vex.observe('match', print)");
        assert_unavailable_preiniting("vex.warn", "vex.warn('oh no!')");

        let assert_unavailable_initing = |name, call| {
            VexTest::new(format!("initing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            {call}
                            vex.language('rust')
                            vex.query('(binary_expression)')
                            vex.observe('match', print)
                    "#},
                )
                .returns_error(format!("{name} unavailable while initing"));
        };
        assert_unavailable_initing("vex.warn", "vex.warn('oh no!')");

        let assert_unavailable_vexing = |name, call| {
            VexTest::new(format!("vexing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression)')
                            vex.observe('match', print)
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            {call}
                    "#},
                )
                .returns_error(format!("{name} unavailable while vexing"));
        };
        assert_unavailable_vexing("vex.language", "vex.language('rust')");
        assert_unavailable_vexing("vex.query", "vex.query('(binary_expression)')");
        assert_unavailable_vexing("vex.observe", "vex.observe('match', print)");
    }

    #[test]
    fn invalid_global() {
        VexTest::new("invalid global")
            .with_scriptlet("vexes/test.star", "problems()")
            .returns_error("not found")
    }

    #[test]
    fn loads() {
        VexTest::new("valid")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    load('lib/helper.star', imported_on_match='on_match')

                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression)')
                        vex.observe('match', imported_on_match)
                "#},
            )
            .with_scriptlet(
                "vexes/lib/helper.star",
                indoc! {r#"
                    def on_match(event):
                        pass
                "#},
            )
            .assert_irritation_free();
        VexTest::new("invalid-loads")
            .with_scriptlet("vexes/test.star", "load('i-do-not-exist.star', 'x')")
            .returns_error(r"cannot find module 'i-do-not-exist\.star'");
        VexTest::new("cycle-loop")
            .with_scriptlet("vexes/test.star", "load('test.star', '_')")
            .returns_error(r"import cycle detected: test\.star -> test\.star");
        VexTest::new("cycle-simple")
            .with_scriptlet("vexes/test.star", "load('1.star', '_')")
            .with_scriptlet("vexes/1.star", r#"load('2.star', '_')"#)
            .with_scriptlet("vexes/2.star", r#"load('1.star', '_')"#)
            .returns_error(r"import cycle detected: 1\.star -> 2\.star -> 1\.star");
        VexTest::new("cycle-complex")
            .with_scriptlet("vexes/test.star", "load('1.star', '_')")
            .with_scriptlet("vexes/1.star", r#"load('2.star', '_')"#)
            .with_scriptlet("vexes/2.star", r#"load('3.star', '_')"#)
            .with_scriptlet("vexes/3.star", r#"load('lib/4.star', '_')"#)
            .with_scriptlet("vexes/lib/4.star", r#"load('1.star', '_')"#)
            .returns_error(
                r"import cycle detected: 1\.star -> 2\.star -> 3\.star -> lib(/|\\)4.star -> 1.star",
            );
    }
}
