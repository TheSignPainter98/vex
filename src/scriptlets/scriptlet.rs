use std::{collections::HashSet, fs};

use camino::{Utf8Component, Utf8Path};
use dupe::Dupe;
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::Evaluator,
    syntax::{AstModule, Dialect},
};

use crate::{
    error::{Error, IOAction, InvalidLoadReason},
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
        let ast = AstModule::parse(path.as_str(), code, &Dialect::Standard)?;
        Self::validate_loads(&ast, &path.pretty_path)?;
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

    fn validate_loads(ast: &AstModule, path: &PrettyPath) -> Result<()> {
        ast.loads()
            .iter()
            .map(|l| LoadStatementModule(l.module_id))
            .try_for_each(|m| m.validate(path))
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
        HashSet::from_iter(["vex".to_string()])
    }

    pub fn loads(&self) -> &HashSet<PrettyPath> {
        &self.loads_files
    }
}

pub struct LoadStatementModule<'a>(&'a str);

impl LoadStatementModule<'_> {
    pub const MIN_COMPONENT_LEN: usize = 3;

    pub fn validate(&self, path: &PrettyPath) -> Result<()> {
        let self_as_path = Utf8Path::new(self.0);
        let components = self_as_path.components().collect::<Vec<_>>();
        let invalid_load = |reason| Error::InvalidLoad {
            path: path.dupe(),
            module: self.0.into(),
            reason,
        };

        if self_as_path.has_root() {
            return Err(invalid_load(InvalidLoadReason::IsAbsolute));
        }

        let extension = self_as_path.extension();
        if !matches!(extension, Some("star")) {
            return Err(invalid_load(InvalidLoadReason::HasIncorrectExtension(
                extension.map(ToString::to_string).unwrap_or_default(),
            )));
        }

        if let Some(too_short) = components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .find(|c| c.as_str().len() < Self::MIN_COMPONENT_LEN)
        {
            return Err(invalid_load(InvalidLoadReason::HasTooShortComponent(
                too_short.to_string(),
            )));
        }

        let Some(stem) = self_as_path.file_stem() else {
            return Err(invalid_load(InvalidLoadReason::IsDir));
        };
        if stem.len() < Self::MIN_COMPONENT_LEN {
            return Err(invalid_load(InvalidLoadReason::HasTooShortStem(
                stem.into(),
            )));
        }
        if stem.ends_with('_') {
            return Err(invalid_load(InvalidLoadReason::HasUnderscoreAtEndOfStem(
                stem.into(),
            )));
        }

        if let Some(forbidden_char) = self.0.chars().find(|c| match c {
            'a'..='z' | '0'..='9' | '/' | '.' | '_' => false,
            _ => true,
        }) {
            return Err(invalid_load(InvalidLoadReason::HasForbiddenChar(
                forbidden_char,
            )));
        }

        if let Some(idx) = self.0.find("...") {
            let last_dot_idx = idx
                + self.0[idx..]
                    .chars()
                    .enumerate()
                    .find(|(_, c)| *c != '.')
                    .map(|(i, _)| i)
                    .unwrap_or(self.0.len());
            return Err(invalid_load(InvalidLoadReason::HasSuccessiveDots(
                self.0[idx..last_dot_idx].to_string(),
            )));
        }

        if components
            .iter()
            .skip_while(|c| !matches!(c, Utf8Component::Normal(_)))
            .skip(1)
            .any(|c| !matches!(c, Utf8Component::Normal(_)))
        {
            return Err(invalid_load(InvalidLoadReason::HasMidwayPathOperator));
        }

        if self.0.contains("__") {
            return Err(invalid_load(InvalidLoadReason::HasSuccessiveUnderscores));
        }

        if let Some(bad_underscore_component) = components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .map(|c| c.as_str())
            .find(|c| c.starts_with('_') || c.ends_with('_'))
        {
            return Err(invalid_load(
                InvalidLoadReason::HasBadUnderscoresInComponent(bad_underscore_component.into()),
            ));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct InitingScriptlet {
    pub path: SourcePath,
    toplevel: bool,
    pub preinited_module: FrozenModule,
}

impl InitingScriptlet {
    pub fn init(self, project_root: &PrettyPath) -> Result<VexingScriptlet> {
        let Self {
            path,
            toplevel,
            preinited_module,
        } = self;

        let Some(init) = preinited_module.get_option("init")? else {
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
            ObserverDataBuilder::new(project_root.dupe(), path.pretty_path.dupe())
                .insert_into(&module);
            {
                let extra = InvocationData::new(Action::Initing, path.pretty_path.dupe());
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&PrintHandler);
                extra.insert_into(&mut eval);
                eval.eval_function(init.value(), &[], &[])?;
            }
            module.freeze()?
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
        VexTest::new("no-triggers")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        pass
                "#},
            )
            .returns_error(r"test\.star adds no triggers");
        VexTest::new("no-callbacks")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(language='rust')
                "#},
            )
            .returns_error(r"test\.star declares no callbacks");
        VexTest::new("no-queries")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(language='rust')
                        vex.observe('query_match', lambda x: x)
                "#},
            )
            .returns_error(r#"test\.star observes query_match but adds no triggers with queries"#);
        VexTest::new("no-query-match-listener")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression)',
                        )
                "#},
            )
            .returns_error(
                r#"test\.star adds trigger with query but does not observe query_match"#,
            );
    }

    #[test]
    fn unknown_event() {
        VexTest::new("unknown-event")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression)',
                        )
                        vex.observe('smissmass', on_smissmass)

                    def on_smissmass(event):
                        pass
                "#},
            )
            .returns_error("unknown event 'smissmass'");
    }

    #[test]
    fn app_object_attr_availability() {
        enum Availability {
            Available,
            Unavailable,
        }
        use Availability::*;

        let test_preiniting_availability = |name, availability, call| {
            let result = VexTest::new(format!("preiniting-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        {call}

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression)',
                            )
                            vex.observe('query_match', lambda x: x)
                    "#},
                )
                .try_run();
            match availability {
                Available => {
                    result.unwrap();
                }
                Unavailable => assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("{name} unavailable while preiniting"))),
            }
        };
        test_preiniting_availability(
            "vex.add_trigger",
            Unavailable,
            "vex.add_trigger(language='rust')",
        );
        test_preiniting_availability(
            "vex.observe",
            Unavailable,
            "vex.observe('query_match', print)",
        );
        test_preiniting_availability("vex.warn", Unavailable, "vex.warn('oh no!')");

        let assert_available_initing = |name, call| {
            VexTest::new(format!("initing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            {call}
                            vex.add_trigger(path="*.rs")
                            vex.observe('', lambda x: x)
                    "#},
                )
                .returns_error(format!("{name} unavailable while initing"));
        };
        assert_available_initing("vex.warn", "vex.warn('oh no!')");

        let test_vexing_availability = |name, availability, call| {
            let result = VexTest::new(format!("vexing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression)',
                            )
                            vex.observe('query_match', lambda x: x)
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            {call}
                    "#},
                )
                .try_run();
            match availability {
                Available => drop(result.unwrap()),
                Unavailable => assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("{name} unavailable while vexing"))),
            }
        };
        test_vexing_availability(
            "vex.add_trigger",
            Unavailable,
            "vex.add_trigger(language='rust')",
        );
        test_vexing_availability(
            "vex.observe",
            Unavailable,
            "vex.observe('query_match', print)",
        );
        test_vexing_availability("vex.warn", Available, "vex.warn('oh no!')");
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
                    load('lib/helper.star', imported_on_query_match='on_query_match')

                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression)',
                        )
                        vex.observe('query_match', imported_on_query_match)
                "#},
            )
            .with_scriptlet(
                "vexes/lib/helper.star",
                indoc! {r#"
                    def on_query_match(event):
                        pass
                "#},
            )
            .assert_irritation_free();
        VexTest::new("invalid-loads")
            .with_scriptlet("vexes/test.star", "load('i_do_not_exist.star', 'x')")
            .returns_error(r"cannot find module 'i_do_not_exist\.star'");
        VexTest::new("cycle-loop")
            .with_scriptlet("vexes/test.star", "load('test.star', '_')")
            .returns_error(r"import cycle detected: test\.star -> test\.star");
        VexTest::new("cycle-simple")
            .with_scriptlet("vexes/test.star", "load('file_1.star', '_')")
            .with_scriptlet("vexes/file_1.star", r#"load('file_2.star', '_')"#)
            .with_scriptlet("vexes/file_2.star", r#"load('file_1.star', '_')"#)
            .returns_error(r"import cycle detected: file_1\.star -> file_2\.star -> file_1\.star");
        VexTest::new("cycle-complex")
            .with_scriptlet("vexes/test.star", "load('file_1.star', '_')")
            .with_scriptlet("vexes/file_1.star", r#"load('file_2.star', '_')"#)
            .with_scriptlet("vexes/file_2.star", r#"load('file_3.star', '_')"#)
            .with_scriptlet("vexes/file_3.star", r#"load('lib/file_4.star', '_')"#)
            .with_scriptlet("vexes/lib/file_4.star", r#"load('file_1.star', '_')"#)
            .returns_error(
                r"import cycle detected: file_1\.star -> file_2\.star -> file_3\.star -> lib(/|\\)file_4.star -> file_1.star",
            );
    }
}
