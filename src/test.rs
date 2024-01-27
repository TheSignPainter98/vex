use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
};

use camino::Utf8PathBuf;

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
        assert!(
            self.scriptlets
                .insert(path.into(), content.into())
                .is_none(),
            "duplicate scriptlet declaration"
        );
        self
    }

    #[allow(unused)]
    pub fn with_source_file(
        mut self,
        path: impl Into<Utf8PathBuf>,
        content: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.source_files
            .insert(path.into(), content.into())
            .expect("duplicate source file declaration");
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

    #[allow(unused)]
    pub fn returns_error(self, message: impl Into<Cow<'static, str>>) {
        assert_eq!(self.try_run().unwrap_err().to_string(), message.into());
    }

    fn try_run(mut self) -> anyhow::Result<Vec<Irritation>> {
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
            File::create(scriptlet_path)?
                .write_all(content.as_bytes())
                .unwrap();
        }

        for (path, content) in &self.source_files {
            File::create(root_path.join(path))?.write_all(content.as_bytes())?;
        }

        let ctx = Context::acquire_in(&root_path).unwrap();
        if !self.bare {
            fs::create_dir(ctx.vex_dir()).ok();
        }
        let store = PreinitingStore::new(&ctx)?.preinit()?.init()?;
        super::vex(&ctx, &store)
    }

    fn setup(&mut self) {
        println!("running test {}...", self.name);
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
