use std::collections::{BTreeMap, HashSet};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use const_format::formatcp;
use dupe::Dupe;
use lazy_static::lazy_static;
use regex::Regex;
use starlark::{
    analysis::AstModuleLint,
    environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
    errors::Lint,
    eval::{Evaluator, FileLoader},
    syntax::{AstModule, Dialect},
    values::FrozenHeap,
};

use crate::{
    context::Context,
    error::{Error, InvalidLoadReason},
    result::Result,
    scriptlets::{
        action::Action,
        app_object::AppObject,
        event::EventKind,
        extra_data::{RetainedData, TempData, UnfrozenRetainedData},
        handler_module::HandlerModule,
        print_handler::PrintHandler,
        store::{InitOptions, PreinitedModuleStore},
        Intent, ObserverData, PreinitOptions,
    },
    source_path::PrettyPath,
};

#[derive(Debug)]
pub struct PreinitingScriptlet {
    pub path: Utf8PathBuf,
    ast: AstModule,
    loads: BTreeMap<String, LoadPath>,
}

impl PreinitingScriptlet {
    pub fn new(path: Utf8PathBuf, code: String) -> Result<Self> {
        let ast = AstModule::parse(path.as_str(), code, &Dialect::Standard)?;
        let loads = ast
            .loads()
            .into_iter()
            .map(|load| load.module_id.to_owned())
            .map(|raw_load| {
                let load_path = LoadPath::new(&path, &raw_load)?;
                Ok((raw_load, load_path))
            })
            .collect::<Result<_>>()?;
        Ok(Self { path, ast, loads })
    }

    #[allow(unused)]
    pub fn lint(&self) -> Vec<Lint> {
        self.ast.lint(Some(&self.global_names()))
    }

    // #[allow(unused)]
    // pub fn typecheck(&self, globals: &Globals, ...) -> Result<()> {
    // // TODO(kcza): typecheck starlark before executing it!
    // }

    pub fn preinit(
        self,
        ctx: &Context,
        opts: &PreinitOptions,
        partial_store: &PreinitedModuleStore,
        frozen_heap: &FrozenHeap,
    ) -> Result<InitingScriptlet> {
        let Self { path, ast, loads } = self;
        let PreinitOptions {
            script_args,
            verbosity,
        } = opts;

        let preinited_module = {
            let preinited_module = Module::new();
            UnfrozenRetainedData::new().insert_into(&preinited_module);

            {
                let temp_data = TempData {
                    ctx,
                    action: Action::Preiniting,
                    script_args,
                    ignore_markers: None,
                    lsp_enabled: false,
                    warning_filter: None,
                };
                let print_handler = PrintHandler::new(*verbosity, path.as_str());
                let loader = Loader::new(&loads, partial_store);
                let mut eval = Evaluator::new(&preinited_module);
                eval.set_loader(&loader);
                eval.set_print_handler(&print_handler);
                eval.extra = Some(&temp_data);
                eval.eval_module(ast, &Self::globals())?;
            };
            preinited_module.freeze()?
        };
        frozen_heap.add_reference(preinited_module.frozen_heap());

        Ok(InitingScriptlet {
            path,
            preinited_module,
        })
    }

    fn globals() -> Globals {
        let mut builder = GlobalsBuilder::extended_by(&[LibraryExtension::Print]);
        let app = AppObject::new();
        builder.set(AppObject::NAME, builder.alloc(app));
        builder.build()
    }

    fn global_names(&self) -> HashSet<String> {
        HashSet::from_iter(["vex".to_string()])
    }

    pub fn loads(&self) -> &BTreeMap<String, LoadPath> {
        &self.loads
    }
}

struct Loader<'src> {
    loads: &'src BTreeMap<String, LoadPath>,
    store: &'src PreinitedModuleStore,
}

impl<'src> Loader<'src> {
    fn new(loads: &'src BTreeMap<String, LoadPath>, store: &'src PreinitedModuleStore) -> Self {
        Self { loads, store }
    }
}

impl FileLoader for Loader<'_> {
    fn load(&self, path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
        self.loads
            .get(path)
            .and_then(|load_path| self.store.get(load_path.path()))
            .map(|scriptlet| scriptlet.preinited_module.dupe())
            .ok_or_else(|| Error::NoSuchModule(path.into()).into())
    }
}

#[derive(Debug)]
pub struct LoadPath(Utf8PathBuf);

impl LoadPath {
    pub const MIN_COMPONENT_LEN: usize = 3;

    pub fn path(&self) -> &Utf8Path {
        &self.0
    }

    fn new(from: &Utf8Path, load: &str) -> Result<Self> {
        let load_path = Utf8Path::new(load);
        Self::validate_raw(from, load_path)?;
        let resolved_path = match load_path.components().next() {
            Some(Utf8Component::CurDir | Utf8Component::ParentDir) => {
                Self::path_in_dir(from, load)?
            }
            _ => load_path.to_owned(),
        };
        Ok(Self(resolved_path))
    }

    fn validate_raw(from: &Utf8Path, load: &Utf8Path) -> Result<()> {
        let components = load.components().collect::<Vec<_>>();
        let invalid_load = |reason| Error::InvalidLoad {
            load: load.to_string(),
            module: PrettyPath::new(from),
            reason,
        };

        if load.as_str().is_empty() {
            return Err(invalid_load(InvalidLoadReason::Empty));
        }

        if let Some(forbidden_char) = load
            .as_str()
            .chars()
            .find(|c| !matches!(c, 'a'..='z' | '0'..='9' | '/' | '.' | '_'))
        {
            return Err(invalid_load(InvalidLoadReason::ForbiddenChar(
                forbidden_char,
            )));
        }

        let is_unix_absolute = cfg!(target_os = "windows") && load.starts_with("/"); // Ensure consistent messaging.
        if load.is_absolute() || is_unix_absolute {
            return Err(invalid_load(InvalidLoadReason::Absolute));
        }

        let extension = load.extension();
        if !matches!(extension, Some("star")) {
            if load.as_str().len() == ".star".len() {
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

        let Some(stem) = load.file_stem() else {
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

        if load.as_str().contains("//") {
            return Err(invalid_load(InvalidLoadReason::DoubleSlash));
        }

        let dumb_components = load.as_str().split('/').collect::<Vec<_>>();
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

        if load.as_str().contains("__") {
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
        if !VALID_PATH.is_match(load.as_str()) {
            return Err(invalid_load(InvalidLoadReason::NonSpecific));
        }

        Ok(())
    }

    fn path_in_dir(from: &Utf8Path, load: &str) -> Result<Utf8PathBuf> {
        let invalid_load = |reason| Error::InvalidLoad {
            load: load.to_owned(),
            module: PrettyPath::new(from),
            reason,
        };

        let dir = from.parent().unwrap_or(Utf8Path::new(""));
        let mut clean_path_components = Vec::with_capacity(10);
        for component in dir.components().chain(Utf8Path::new(load).components()) {
            match component {
                Utf8Component::CurDir => {}
                Utf8Component::RootDir | Utf8Component::Prefix(_) | Utf8Component::Normal(_) => {
                    clean_path_components.push(component)
                }
                Utf8Component::ParentDir => {
                    if clean_path_components.is_empty() {
                        return Err(invalid_load(InvalidLoadReason::OutsideDirectory));
                    }
                    clean_path_components.pop();
                }
            }
        }

        Ok(Utf8PathBuf::from_iter(clean_path_components))
    }
}

#[derive(Debug)]
pub struct InitingScriptlet {
    pub path: Utf8PathBuf,
    pub preinited_module: FrozenModule,
}

impl InitingScriptlet {
    pub fn init(
        self,
        ctx: &Context,
        opts: &InitOptions,
        frozen_heap: &FrozenHeap,
    ) -> Result<ObserverData> {
        let Self {
            path,
            preinited_module,
        } = self;
        let InitOptions {
            script_args,
            verbosity,
        } = opts;

        let Some(init) = preinited_module.get_option("init")? else {
            return Ok(ObserverData::empty());
        };

        let module = {
            let module = HandlerModule::new();
            {
                let temp_data = TempData {
                    ctx,
                    action: Action::Initing,
                    script_args,
                    ignore_markers: None,
                    lsp_enabled: false,
                    warning_filter: None,
                };
                let print_handler = PrintHandler::new(*verbosity, path.as_str());
                let mut eval = Evaluator::new(&module);
                eval.extra = Some(&temp_data);
                eval.set_print_handler(&print_handler);
                eval.eval_function(init.value(), &[], &[])?;
            }
            module.into_module().freeze()?
        };
        frozen_heap.add_reference(module.frozen_heap());

        let observer_data = {
            let invocation_data = RetainedData::get_from(&module);
            let intents = invocation_data.intents();
            let mut observer_data = ObserverData::with_capacity(intents.len());
            intents.iter().for_each(|intent| {
                if let Intent::Observe {
                    event_kind,
                    observer,
                } = intent
                {
                    let observer = observer.dupe();
                    match event_kind {
                        EventKind::OpenProject => observer_data.add_open_project_observer(observer),
                        EventKind::OpenFile => observer_data.add_open_file_observer(observer),
                        EventKind::Match => panic!("internal error: query_match not observable"),
                        EventKind::PreTestRun => observer_data.add_pre_test_run_observer(observer),
                        EventKind::PostTestRun => {
                            observer_data.add_post_test_run_observer(observer)
                        }
                    }
                }
            });
            observer_data
        };
        if observer_data.len() == 0 {
            crate::warn!("{} observes no events", path);
        }
        Ok(observer_data)
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use const_format::formatcp;
    use indoc::{formatdoc, indoc};
    use insta::assert_snapshot;
    use uniquote::Quote;

    use crate::{result::Result, scriptlets::scriptlet::PreinitingScriptlet, vextest::VexTest};

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
            .assert_irritation_free();
    }

    #[test]
    fn no_callbacks() {
        VexTest::new("no-callbacks")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        pass
                "#},
            )
            .assert_irritation_free();
    }

    #[test]
    fn bad_search() {
        assert_snapshot!(VexTest::new("no-args")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search()
                "#},
            )
            .try_run()
            .unwrap_err());
        assert_snapshot!(VexTest::new("no-query")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                        )
                "#},
            )
            .try_run()
            .unwrap_err());
        assert_snapshot!(VexTest::new("no-query-match-listener")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(binary_expression)',
                        )
                "#},
            )
            .try_run()
            .unwrap_err());
    }

    #[test]
    fn unknown_event() {
        VexTest::new("unknown-event")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
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
                .with_scriptlet("vexes/test.star", call)
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
            "vex.observe",
            Unavailable,
            "vex.observe('open_file', lambda x: x)",
        );
        test_preiniting_availability("vex.lsp_for", Unavailable, "vex.lsp_for('rust')");
        test_preiniting_availability(
            "vex.search",
            Unavailable,
            "vex.search('rust', '(source_file)', lambda x: x)",
        );
        test_preiniting_availability("vex.active", Unavailable, "vex.active('some-id')");
        test_preiniting_availability("vex.warn", Unavailable, "vex.warn('test', 'oh no!')");

        let assert_available_initing = |name, call| {
            VexTest::new(format!("initing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            {call}
                            vex.observe('open_project', lambda x: x)
                    "#},
                )
                .returns_error(format!("{name} unavailable while initing"));
        };
        assert_available_initing("vex.warn", "vex.warn('test', 'oh no!')");

        let test_vexing_open_availability = |name, availability, call| {
            let result = VexTest::new(format!("vexing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            vex.observe('open_project', on_open_project)
                            vex.observe('open_file', on_open_file)

                        def on_open_project(event):
                            {call}

                        def on_open_file(event):
                            {call}
                    "#},
                )
                .try_run();
            match availability {
                Available => drop(result.unwrap()),
                Unavailable => {
                    let err = result.unwrap_err().to_string();
                    assert!(
                        err.contains(&format!("{name} unavailable while")),
                        "wrong error, got {err}"
                    );
                }
            }
        };
        test_vexing_open_availability(
            "vex.observe",
            Unavailable,
            "vex.observe('open_file', lambda x: x)",
        );
        test_vexing_open_availability("vex.lsp_for", Available, "vex.lsp_for('rust')");
        test_vexing_open_availability(
            "vex.search",
            Available,
            "vex.search('rust', '(source_file)', lambda x: x)",
        );
        test_vexing_open_availability("vex.active", Available, "vex.active('some-id')");
        test_vexing_open_availability("vex.warn", Available, "vex.warn('test', 'oh no!')");

        let test_vexing_match_availability = |name, availability, call| {
            let result = VexTest::new(format!("vexing-{name}"))
                .with_scriptlet(
                    "vexes/test.star",
                    formatdoc! {r#"
                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(source_file)',
                                on_match,
                            )

                        def on_match(event):
                            {call}
                    "#},
                )
                .with_source_file(
                    "src/main.rs",
                    indoc! {r#"
                        fn main() {
                            assert_eq!(2 + 2, 5);
                        }
                    "#},
                )
                .try_run();
            match availability {
                Available => drop(result.unwrap()),
                Unavailable => {
                    let err = result.unwrap_err().to_string();
                    assert!(
                        err.contains(&format!("{name} unavailable while handling match")),
                        "wrong error, got {err}"
                    );
                }
            }
        };
        test_vexing_match_availability(
            "vex.observe",
            Unavailable,
            "vex.observe('open_file', lambda x: x)",
        );
        test_vexing_match_availability("vex.lsp_for", Available, "vex.lsp_for('rust')");
        test_vexing_match_availability(
            "vex.search",
            Unavailable,
            "vex.search('rust', '(source_file)', lambda x: x)",
        );
        test_vexing_match_availability("vex.active", Unavailable, "vex.active('some-id')");
        test_vexing_match_availability("vex.warn", Available, "vex.warn('test', 'oh no!')");
    }

    #[test]
    fn invalid_global() {
        VexTest::new("invalid global")
            .with_scriptlet("vexes/test.star", "problems()")
            .returns_error("not found")
    }

    #[test]
    fn loads() {
        VexTest::new("valid-absolute")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    load('lib/helper.star', imported_on_open_project='on_open_project')

                    def init():
                        vex.observe('open_project', imported_on_open_project)
                "#},
            )
            .with_scriptlet(
                "vexes/lib/helper.star",
                indoc! {r#"
                    def on_open_project(event):
                        pass
                "#},
            )
            .assert_irritation_free();
        VexTest::new("valid-sibling")
            .with_scriptlet("vexes/dir/test.star", "load('./sibling.star', 'func')")
            .with_scriptlet("vexes/dir/sibling.star", "fail('marker')")
            .returns_error("marker");
        VexTest::new("valid-parent")
            .with_scriptlet("vexes/dir/test.star", "load('../sibling.star', 'func')")
            .with_scriptlet("vexes/sibling.star", "fail('marker')")
            .returns_error("marker");

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
        VexTest::new("cycle-complex-absolute")
            .with_scriptlet("vexes/test.star", "load('111.star', '_')")
            .with_scriptlet("vexes/111.star", r#"load('222.star', '_')"#)
            .with_scriptlet("vexes/222.star", r#"load('333.star', '_')"#)
            .with_scriptlet("vexes/333.star", r#"load('lib/444.star', '_')"#)
            .with_scriptlet("vexes/lib/444.star", r#"load('111.star', '_')"#)
            .returns_error(
                r"import cycle detected: 111\.star -> 222\.star -> 333\.star -> lib/444.star -> 111.star",
            );
        VexTest::new("cycle-complex-relative")
            .with_scriptlet("vexes/test.star", "load('111.star', '_')")
            .with_scriptlet("vexes/111.star", r#"load('aaa/222.star', '_')"#)
            .with_scriptlet("vexes/aaa/222.star", r#"load('./333.star', '_')"#)
            .with_scriptlet("vexes/aaa/333.star", r#"load('../lib/444.star', '_')"#)
            .with_scriptlet("vexes/lib/444.star", r#"load('111.star', '_')"#)
            .returns_error(
                r"import cycle detected: 111\.star -> aaa/222\.star -> aaa/333\.star -> lib/444.star -> 111.star",
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

                const DIR: &str = "/tmp/vex_project";
                PreinitingScriptlet::new(
                    Utf8PathBuf::from(formatcp!("{DIR}/test.star")),
                    formatdoc! {
                        r#"
                            load({}, 'unused')
                        "#,
                        path.quote()
                    },
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
