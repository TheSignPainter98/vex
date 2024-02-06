use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
};

use camino::Utf8PathBuf;
use indoc::indoc;
use regex::Regex;

use crate::{context::Context, irritation::Irritation, scriptlets::PreinitingStore};

pub struct VexTest {
    name: Cow<'static, str>,
    bare: bool,
    manifest_content: Option<Cow<'static, str>>,
    scriptlets: BTreeMap<Utf8PathBuf, Cow<'static, str>>,
    source_files: BTreeMap<Utf8PathBuf, Cow<'static, str>>,
}

impl VexTest {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            bare: false,
            manifest_content: None,
            scriptlets: BTreeMap::new(),
            source_files: BTreeMap::new(),
        }
    }

    #[allow(unused)]
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    #[allow(unused)]
    pub fn with_manifest(mut self, content: impl Into<Cow<'static, str>>) -> Self {
        self.manifest_content = Some(content.into());
        self
    }

    pub fn with_scriptlet(
        mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.add_scriptlet(path, content);
        self
    }

    fn add_scriptlet(
        &mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'static, str>>,
    ) {
        let path = path.into();
        let content = content.into();

        assert!(
            path.starts_with("vexes/"),
            "test scriptlet path must start with vexes/"
        );
        assert!(
            self.scriptlets.insert(path, content).is_none(),
            "duplicate scriptlet declaration"
        );
    }

    #[allow(unused)]
    pub fn with_source_file(
        mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'static, str>>,
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
        assert_eq!(self.try_run().unwrap(), &[], "irritations returned!");
    }

    #[allow(unused)]
    pub fn returns_irritations(self, irritations: Vec<IrritationMatch>) {
        self.try_run()
            .unwrap()
            .into_iter()
            .zip(irritations)
            .enumerate()
            .for_each(|(i, (irritation, matcher))| {
                assert!(
                    matcher.matches(&irritation),
                    "irritation {i} incorrect, expected {matcher:?}, got {irritation:?}"
                )
            });
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

    pub fn try_run(mut self) -> anyhow::Result<Vec<Irritation>> {
        self.setup();

        let root_dir = tempfile::tempdir().unwrap();
        let root_path = Utf8PathBuf::try_from(root_dir.path().to_path_buf()).unwrap();

        if !self.bare {
            let manifest_content = self.manifest_content.as_deref().unwrap_or_default();
            File::create(root_path.join("vex.toml"))
                .unwrap()
                .write_all(manifest_content.as_bytes())?;
        }

        for (path, content) in &self.scriptlets {
            let scriptlet_path = root_path.join(path);
            fs::create_dir_all(scriptlet_path.parent().unwrap()).unwrap();
            File::create(scriptlet_path)
                .unwrap()
                .write_all(content.as_bytes())
                .unwrap();
        }

        for (path, content) in &self.source_files {
            let source_path = root_path.join(path);
            fs::create_dir_all(source_path.parent().unwrap()).unwrap();
            File::create(root_path.join(path))
                .unwrap()
                .write_all(content.as_bytes())
                .unwrap();
        }

        let ctx = Context::acquire_in(&root_path).unwrap();
        if !self.bare {
            fs::create_dir(ctx.vex_dir()).ok();
        }
        let store = PreinitingStore::new(&ctx)?.preinit()?.init()?;
        super::vex(&ctx, &store)
    }

    fn setup(&mut self) {
        eprintln!("running test {}...", self.name);

        self.add_scriptlet(
            "vexes/check.star",
            indoc! {r#"
                check = {}

                # Placate error checker
                def init():
                    vex.language('rust')
                    vex.query('(binary_expression)')
                    vex.observe('match', lambda x: x)

                def check_eq(left, right):
                    if left != right:
                        fail('assertion failed: %r != %r' % (left, right))
                check['eq'] = check_eq

                def check_hasattr(obj, attr):
                    if not hasattr(obj, attr):
                        fail('assertion failed: %r.%v does not exist' % (obj, attr))
                check['hasattr'] = check_hasattr

                def check_in(obj, what):
                    if what not in obj:
                        fail('assertion failed: %r not in %r' % (what, obj))
                check['in'] = check_in

                def check_is_path(to_check):
                    str_to_check = str(to_check)
                    if '/' not in str_to_check and '\\' not in str_to_check:
                        fail('assertion failed: %r is not a path' % to_check)
                check['is_path'] = check_is_path

                check
            "#},
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrritationMatch {
    vex_id: Cow<'static, str>,
    message: Cow<'static, str>,
    start_byte: usize,
    end_byte: usize,
    path: Utf8PathBuf,
}

impl IrritationMatch {
    pub fn matches(&self, _irritation: &Irritation) -> bool {
        todo!();
    }
}
