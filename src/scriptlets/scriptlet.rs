use std::{collections::HashSet, fs};

use camino::{Utf8Component, Utf8Path};
use const_format::formatcp;
use dupe::Dupe;
use lazy_static::lazy_static;
use regex::Regex;
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

    fn new_from_str(path: SourcePath, code: impl Into<String>, toplevel: bool) -> Result<Self> {
        let code = code.into();
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

    pub fn validate(&self, current_file: &PrettyPath) -> Result<()> {
        let self_as_path = Utf8Path::new(self.0);
        let components = self_as_path.components().collect::<Vec<_>>();
        let invalid_load = |reason| Error::InvalidLoad {
            load: self.0.to_string(),
            module: current_file.dupe(),
            reason,
        };

        if self.0.is_empty() {
            return Err(invalid_load(InvalidLoadReason::Empty));
        }

        if let Some(forbidden_char) = self
            .0
            .chars()
            .find(|c| !matches!(c, 'a'..='z' | '0'..='9' | '/' | '.' | '_'))
        {
            return Err(invalid_load(InvalidLoadReason::ForbiddenChar(
                forbidden_char,
            )));
        }

        let is_unix_absolute = cfg!(target_os = "windows") && self_as_path.starts_with("/"); // Ensure consistent messaging.
        if self_as_path.is_absolute() || is_unix_absolute {
            return Err(invalid_load(InvalidLoadReason::Absolute));
        }

        let extension = self_as_path.extension();
        if !matches!(extension, Some("star")) {
            if self.0.len() == ".star".len() {
                // Override error message for slightly more intuitive one.
                return Err(invalid_load(InvalidLoadReason::TooShortStem));
            }
            return Err(invalid_load(InvalidLoadReason::IncorrectExtension));
        }

        if components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .any(|c| c.as_str().len() < Self::MIN_COMPONENT_LEN)
        {
            return Err(invalid_load(InvalidLoadReason::TooShortComponent));
        }

        let Some(stem) = self_as_path.file_stem() else {
            return Err(invalid_load(InvalidLoadReason::Dir));
        };
        if stem.len() < Self::MIN_COMPONENT_LEN {
            return Err(invalid_load(InvalidLoadReason::TooShortStem));
        }
        if stem.ends_with('_') {
            return Err(invalid_load(InvalidLoadReason::UnderscoreAtEndOfStem));
        }

        if components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .any(|c| c.as_str().contains(".."))
        {
            return Err(invalid_load(InvalidLoadReason::SuccessiveDots));
        }

        if components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .any(|c| c.as_str().starts_with('.'))
        {
            return Err(invalid_load(InvalidLoadReason::HiddenComponent));
        }
        if components[..components.len() - 1]
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .any(|c| c.as_str().contains('.'))
        {
            return Err(invalid_load(InvalidLoadReason::MidwayDot));
        }
        if components[components.len() - 1]
            .as_str()
            .chars()
            .filter(|c| *c == '.')
            .count()
            > 1
        {
            return Err(invalid_load(InvalidLoadReason::IncorrectExtension));
        }

        if self.0.contains("//") {
            return Err(invalid_load(InvalidLoadReason::DoubleSlash));
        }

        let dumb_components = self.0.split('/').collect::<Vec<_>>();
        match components.first().expect("internal error: path empty") {
            Utf8Component::CurDir => {
                if dumb_components[1..].contains(&".") {
                    return Err(invalid_load(InvalidLoadReason::MultipleCurDirs));
                }
                if components
                    .iter()
                    .any(|c| matches!(c, Utf8Component::ParentDir))
                {
                    return Err(invalid_load(InvalidLoadReason::MixedPathOperators));
                }
            }
            Utf8Component::ParentDir => {
                if dumb_components.contains(&".") {
                    return Err(invalid_load(InvalidLoadReason::MixedPathOperators));
                }
            }
            _ => {
                if dumb_components.contains(&".") || dumb_components.contains(&"..") {
                    return Err(invalid_load(InvalidLoadReason::MidwayPathOperator));
                }
            }
        }

        if self.0.contains("__") {
            return Err(invalid_load(InvalidLoadReason::SuccessiveUnderscores));
        }

        if components
            .iter()
            .filter(|c| matches!(c, Utf8Component::Normal(_)))
            .map(|c| c.as_str())
            .any(|c| c.starts_with('_') || c.ends_with('_'))
        {
            return Err(invalid_load(InvalidLoadReason::UnderscoresAtEndOfComponent));
        }

        // Catch-all case in case any specfic error has been missed.
        lazy_static! {
            static ref VALID_PATH: Regex = {
                const VALID_COMPONENT: &str = "[a-z0-9][a-z0-9_]+[a-z0-9]";
                Regex::new(formatcp!(
                    r"^(\./|(\.\./)+)?({VALID_COMPONENT}/)*{VALID_COMPONENT}\.star$"
                ))
                .unwrap()
            };
        };
        if !VALID_PATH.is_match(self.0) {
            return Err(invalid_load(InvalidLoadReason::NonSpecific));
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
    use camino::Utf8Path;
    use const_format::formatcp;
    use indoc::{formatdoc, indoc};
    use uniquote::Quote;

    use crate::{
        result::Result, scriptlets::scriptlet::PreinitingScriptlet, source_path::SourcePath,
        vextest::VexTest,
    };

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
        VexTest::new("nonexistent-loads")
            .with_scriptlet("vexes/test.star", "load('i_do_not_exist.star', 'x')")
            .returns_error(r"cannot find module 'i_do_not_exist\.star'");
        VexTest::new("cycle-loop")
            .with_scriptlet("vexes/test.star", "load('test.star', '_')")
            .returns_error(r"import cycle detected: test\.star -> test\.star");
        VexTest::new("cycle-simple")
            .with_scriptlet("vexes/test.star", "load('111.star', '_')")
            .with_scriptlet("vexes/111.star", r#"load('222.star', '_')"#)
            .with_scriptlet("vexes/222.star", r#"load('111.star', '_')"#)
            .returns_error(r"import cycle detected: 111\.star -> 222\.star -> 111\.star");
        VexTest::new("cycle-complex")
            .with_scriptlet("vexes/test.star", "load('111.star', '_')")
            .with_scriptlet("vexes/111.star", r#"load('222.star', '_')"#)
            .with_scriptlet("vexes/222.star", r#"load('333.star', '_')"#)
            .with_scriptlet("vexes/333.star", r#"load('lib/444.star', '_')"#)
            .with_scriptlet("vexes/lib/444.star", r#"load('111.star', '_')"#)
            .returns_error(
                r"import cycle detected: 111\.star -> 222\.star -> 333\.star -> lib(/|\\)444.star -> 111.star",
            );
    }

    #[test]
    fn load_validation() {
        #[derive(Default)]
        struct LoadTest {
            name: &'static str,
            path: Option<&'static str>,
        }

        impl LoadTest {
            fn new(name: &'static str) -> Self {
                Self {
                    name,
                    ..Self::default()
                }
            }

            fn path(mut self, path: &'static str) -> Self {
                self.path = Some(path);
                self
            }

            fn ok(self) {
                self.run().unwrap();
            }

            fn causes(self, message: &'static str) {
                let expected_message = format!(
                    "cannot load {}: {message}",
                    self.path.expect("path not set").replace(r"\\", r"\")
                );
                assert_eq!(expected_message, self.run().unwrap_err().to_string());
            }

            fn run(self) -> Result<()> {
                let Self { name, path } = self;
                let path = path.expect("path not set");
                eprintln!("running test {name} with {path:?}...");
                println!(r#"load({}, 'unused') "#, path.quote());

                const DIR: &str = "/tmp/vex_project";
                PreinitingScriptlet::new_from_str(
                    SourcePath::new(
                        Utf8Path::new(formatcp!("{DIR}/test.star")),
                        Utf8Path::new(DIR),
                    ),
                    formatdoc! {
                        r#"
                            load({}, 'unused')
                        "#,
                        path.quote()
                    },
                    false,
                )
                .map(|_| ())
            }
        }

        LoadTest::new("toplevel")
            .path("abcdefghijklmnopqrstuvwxyz_0123456789.star")
            .ok();
        LoadTest::new("nested").path("aaa/bbb/ccc.star").ok();
        LoadTest::new("relative-toplevel").path("./aaa.star").ok();
        LoadTest::new("relative-nested")
            .path("./aaa/bbb/ccc.star")
            .ok();
        LoadTest::new("parent-toplevel").path("../aaa.star").ok();
        LoadTest::new("parent-nested")
            .path("../../../aaa/bbb/ccc.star")
            .ok();

        LoadTest::new("dash")
            .path("---.star")
            .causes("load path can only contain a-z, 0-9, `_`, `.` and `/`, found `-`");
        LoadTest::new("backslashes")
            .path(r".\\.\\aaa.star")
            .causes(r"load path can only contain a-z, 0-9, `_`, `.` and `/`, found `\`");
        LoadTest::new("extra-starting-current-dir")
            .path("././aaa.star")
            .causes("load path cannot contain multiple `./`");
        LoadTest::new("current-dir-in-parent-dir")
            .path(".././aaa.star")
            .causes("load path cannot contain both `./` and `../`");
        LoadTest::new("parent-op-in-current-dir")
            .path("./../aaa.star")
            .causes("load path cannot contain both `./` and `../`");
        LoadTest::new("midway-current-dir")
            .path("aaa/./bbb.star")
            .causes("load path can only have path operators at the start");
        LoadTest::new("midway-parent-dir")
            .path("aaa/../bbb.star")
            .causes("load path can only have path operators at the start");
        LoadTest::new("successive-slashes")
            .path("aaa//bbb.star")
            .causes("load path cannot contain `//`");
        LoadTest::new("empty")
            .path("")
            .causes("load path cannot be empty");
        LoadTest::new("absolute-unix")
            .path("/aaa.star")
            .causes("load path cannot be absolute");
        LoadTest::new("absolute-windows-uppercase")
            .path("C:/aaa.star")
            .causes("load path can only contain a-z, 0-9, `_`, `.` and `/`, found `C`");
        LoadTest::new("absolute-windows-lowercase")
            .path("c:/aaa.star")
            .causes("load path can only contain a-z, 0-9, `_`, `.` and `/`, found `:`");
        LoadTest::new("wrong-extension")
            .path("aaa.starlark")
            .causes("load path must have the `.star` extension");
        LoadTest::new("short-components")
            .path("aa/bb/ccc.star")
            .causes("load path components must be at least 3 characters");
        LoadTest::new("short-stem")
            .path("aa.star")
            .causes("load path stem must be at least 3 characters");
        LoadTest::new("nested-short-stem")
            .path("aaa/bbb/cc.star")
            .causes("load path stem must be at least 3 characters");
        LoadTest::new("uppercase-firbidden")
            .path("AAA.star")
            .causes("load path can only contain a-z, 0-9, `_`, `.` and `/`, found `A`");
        LoadTest::new("invalid-rune-emoji")
            .path("🤸🪑🏌️.star")
            .causes("load path can only contain a-z, 0-9, `_`, `.` and `/`, found `🤸`");
        LoadTest::new("no-stem")
            .path(".star")
            .causes("load path stem must be at least 3 characters");
        LoadTest::new("hidden-files")
            .path(".secret.star")
            .causes("load path cannot have hidden components");
        LoadTest::new("hidden-dirs")
            .path("aaa/.secret/aaa.star")
            .causes("load path cannot have hidden components");
        LoadTest::new("midway-dots")
            .path("aaa/b.b/ccc.star")
            .causes("load path can only have a `.` in the file extension");
        LoadTest::new("many-extensions")
            .path("aaa/bbb.tar.star")
            .causes("load path must have the `.star` extension");
        LoadTest::new("successive-dots-as-component")
            .path("aaa/.../bbb.star")
            .causes("load path cannot contain successive dots in file component");
        LoadTest::new("successive-dots-in-component")
            .path("aaa..bbb.star")
            .causes("load path cannot contain successive dots in file component");
        LoadTest::new("successive-underscores")
            .path("a__a.star")
            .causes("load path cannot contain successive underscores");
        LoadTest::new("leading-underscore")
            .path("_aaa.star")
            .causes("load path cannot have underscores at component-ends");
        LoadTest::new("midway-leading-underscore")
            .path("aaa/_bbb.star")
            .causes("load path cannot have underscores at component-ends");
        LoadTest::new("trailing-underscore")
            .path("aaa_/bbb.star")
            .causes("load path cannot have underscores at component-ends");
        LoadTest::new("trailing-underscore-before-extension")
            .path("aaa/bbb_.star")
            .causes("load path cannot have underscores at end of stem");
        LoadTest::new("underscore-before-dot")
            .path("aaa/b_.b.star")
            .causes("load path must have the `.star` extension");
    }
}
