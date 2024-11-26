use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
};

use camino::{Utf8Component, Utf8PathBuf};
use indoc::indoc;
use regex::Regex;

use crate::{
    cli::{MaxConcurrentFileLimit, MaxProblems},
    context::Context,
    result::Result,
    scan,
    scriptlets::{
        source::{ScriptSource, TestSource},
        InitOptions, PreinitOptions, PreinitingStore,
    },
    verbosity::Verbosity,
    ProjectRunData,
};

#[must_use]
#[derive(Default)]
pub struct VexTest<'s> {
    name: Cow<'s, str>,
    bare: bool,
    manifest_content: Option<Cow<'s, str>>,
    max_problems: MaxProblems,
    lenient: bool,
    fire_test_events: bool,
    scriptlets: Vec<TestSource<Utf8PathBuf, Cow<'s, str>>>,
    source_files: BTreeMap<Utf8PathBuf, Cow<'s, str>>,
}

impl<'s> VexTest<'s> {
    pub fn new(name: impl Into<Cow<'s, str>>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    #[allow(unused)]
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    #[allow(unused)]
    pub fn with_manifest(mut self, content: impl Into<Cow<'s, str>>) -> Self {
        self.manifest_content = Some(content.into());
        self
    }

    pub fn with_max_problems(mut self, max_problems: MaxProblems) -> Self {
        self.max_problems = max_problems;
        self
    }

    pub fn with_lenient(mut self, lenient: bool) -> Self {
        self.lenient = lenient;
        self
    }

    pub fn with_test_events(mut self, fire_test_events: bool) -> Self {
        self.fire_test_events = fire_test_events;
        self
    }

    pub fn with_scriptlet(
        mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'s, str>>,
    ) -> Self {
        self.add_scriptlet(path, content);
        self
    }

    fn add_scriptlet(&mut self, path: impl Into<Utf8PathBuf>, content: impl Into<Cow<'s, str>>) {
        let path = path.into();
        let content = content.into();

        assert!(
            path.as_str().starts_with("vexes/"),
            "test scriptlet path must start with vexes/"
        );
        assert!(
            !self.scriptlets.iter().any(|s| s.path() == path),
            "duplicate scriptlet declaration"
        );
        let vex_dir = path
            .components()
            .next()
            .and_then(|first| match first {
                Utf8Component::Normal(d) => Some(d),
                _ => None,
            })
            .unwrap()
            .to_owned()
            .into();
        self.scriptlets.push(TestSource {
            vex_dir,
            path,
            content,
        });
    }

    #[allow(unused)]
    pub fn with_source_file(
        mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'s, str>>,
    ) -> Self {
        assert!(
            self.source_files
                .insert(path.into(), content.into())
                .is_none(),
            "duplicate source file declaration"
        );
        self
    }

    pub fn assert_irritation_free(self) {
        assert_eq!(
            self.try_run().unwrap().irritations,
            &[],
            "irritations returned!"
        );
    }

    pub fn returns_error(self, message: impl Into<Cow<'static, str>>) {
        let message = message.into();
        let err = self.try_run().unwrap_err();
        let re = Regex::new(&message).expect("regex invalid");
        assert!(
            re.is_match(&err.to_string()),
            "unexpected error: expected error matching '{message}' but got:\n{err}"
        );
    }

    pub fn try_run(mut self) -> Result<ProjectRunData> {
        self.setup();

        let root_dir = tempfile::tempdir().unwrap();
        let root_path = Utf8PathBuf::try_from(root_dir.path().to_path_buf()).unwrap();

        if !self.bare {
            let manifest_content = self
                .manifest_content
                .as_deref()
                .unwrap_or("[vex]\nversion = '1'");
            File::create(root_path.join("vex.toml"))
                .unwrap()
                .write_all(manifest_content.as_bytes())
                .unwrap();
        }

        let ctx = Context::acquire_in(&root_path).unwrap();
        if !self.bare {
            fs::create_dir(ctx.vex_dir()).ok();
        }
        if self.fire_test_events {
            crate::test::run_tests(&self.scriptlets)?;
            Ok(ProjectRunData::default())
        } else {
            for (path, content) in &self.source_files {
                let source_path = root_path.join(path);
                fs::create_dir_all(source_path.parent().unwrap()).unwrap();
                File::create(root_path.join(path))
                    .unwrap()
                    .write_all(content.as_bytes())
                    .unwrap();
            }

            let warning_filter = crate::try_make_warning_filter(&ctx.manifest)?;

            let verbosity = Verbosity::default();
            let preinit_opts = PreinitOptions {
                lenient: self.lenient,
                verbosity,
            };
            let init_opts = InitOptions { verbosity };
            let store = PreinitingStore::new(&self.scriptlets)?
                .preinit(preinit_opts)?
                .init(init_opts)?;
            scan::scan_project(
                &ctx,
                &store,
                warning_filter,
                self.max_problems,
                MaxConcurrentFileLimit::new(1),
                verbosity,
            )
        }
    }

    fn setup(&mut self) {
        eprintln!("running test {}...", self.name);
        self.add_scriptlet(VexTest::CHECK_FS_PATH, VexTest::CHECK_SRC)
    }

    pub const CHECK_STARLARK_PATH: &'static str = "lib/check.star";
    pub const CHECK_FS_PATH: &'static str = "vexes/lib/check.star";
    pub const CHECK_SRC: &'static str = indoc! {r#"
        check = {}

        def check_true(x):
            check_eq(x, True)
        check['true'] = check_true

        def check_false(x):
            check_eq(x, False)
        check['false'] = check_false

        def check_eq(left, right):
            if left != right:
                fail('assertion failed: %r != %r' % (left, right))
        check['eq'] = check_eq

        def check_neq(left, right):
            if left == right:
                fail('assertion failed: %r != %r' % (left, right))
        check['neq'] = check_neq

        def check_attrs(obj, attrs):
            attrs = sorted(attrs)
            check['eq'](dir(obj), attrs)
            for attr in attrs:
                check['hasattr'](obj, attr)
                _ = getattr(obj, attr)
            for attr in dir(obj):
                check['hasattr'](obj, attr)
                _ = getattr(obj, attr)
        check['attrs'] = check_attrs

        def check_hasattr(obj, attr):
            if not hasattr(obj, attr):
                fail('assertion failed: %r.%s does not exist' % (obj, attr))
        check['hasattr'] = check_hasattr

        def check_dir(obj, attr):
            if attr not in dir(obj):
                fail('assertion failed: %r.%s not in dir(%r)' % (obj, attr, obj))
        check['dir'] = check_dir

        def check_in(what, obj):
            if what not in obj:
                fail('assertion failed: %r not in %r' % (what, obj))
        check['in'] = check_in

        def check_not_in(what, obj):
            if what in obj:
                fail('assertion failed: %r in %r' % (what, obj))
        check['not in'] = check_not_in

        def check_is_path(to_check):
            str_to_check = str(to_check)
            if '/' not in str_to_check and '\\' not in str_to_check:
                fail('assertion failed: %r is not a path' % to_check)
        check['is_path'] = check_is_path

        def check_type(obj, typ):
            check['eq'](type(obj), typ)
        check['type'] = check_type

        def check_sorted(obj):
            if obj != sorted(obj):
                fail('assertion failed: %r is not sorted' % obj)
        check['sorted'] = check_sorted

        check
    "#};
}
