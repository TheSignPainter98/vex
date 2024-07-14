use camino::{Utf8Path, Utf8PathBuf};
use indoc::indoc;
use serde::{Deserialize as Deserialise, Serialize as Serialise};

use std::collections::HashMap;
use std::io::{BufWriter, ErrorKind, Read, Write};
use std::ops::Deref;
use std::{
    env,
    fs::{self, File},
};

use crate::associations::Associations;
use crate::error::{Error, IOAction};
use crate::result::Result;
use crate::source_path::PrettyPath;
use crate::supported_language::SupportedLanguage;
use crate::trigger::RawFilePattern;

#[derive(Debug)]
pub struct Context {
    pub project_root: PrettyPath,
    pub manifest: Manifest,
}

pub const EXAMPLE_VEX_FILE: &str = "example.star";

impl Context {
    pub fn acquire() -> Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_content()?;
        let project_root = PrettyPath::new(&project_root);
        let data = toml_edit::de::from_str(&raw_data)?;
        Ok(Context {
            project_root,
            manifest: data,
        })
    }

    #[cfg(test)]
    pub fn acquire_in(dir: &Utf8Path) -> Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_content_in(dir)?;
        let project_root = PrettyPath::new(&project_root);
        let data = toml_edit::de::from_str(&raw_data)?;
        Ok(Context {
            project_root,
            manifest: data,
        })
    }

    pub fn init(project_root: impl AsRef<Utf8Path>, force: bool) -> Result<()> {
        let project_root = project_root.as_ref();
        fs::create_dir_all(project_root.join(QueriesDir::default().as_str())).map_err(|cause| {
            Error::IO {
                path: PrettyPath::new(project_root),
                action: IOAction::Write,
                cause,
            }
        })?;
        Manifest::init(project_root, force)?;

        let example_vex_path = Utf8PathBuf::from(project_root)
            .join(QueriesDir::default().as_str())
            .join(EXAMPLE_VEX_FILE);
        const EXAMPLE_VEX_CONTENT: &str = indoc! {r#"
            def init():
                # First add callbacks for vex's top-level events.
                vex.observe('open_project', on_open_project)

            def on_open_project(event):
                # When the project is opened, declare an intent to find integer literals
                vex.search(
                    'rust',
                    '(integer_literal) @lit',
                    on_match,
                )

            def on_match(event):
                # When an integer literal is found, if long base-10, ensure broken up with
                # underscores.

                lit = event.captures['lit']
                lit_str = str(lit)

                if lit_str.startswith('0x') or lit_str.startswith('0b'):
                    return
                if len(lit_str) <= 6:
                    return
                if '_' in lit_str:
                    return

                vex.warn(
                    'large unbroken integer literal',
                    at=(lit, 'consider adding underscores')
                )
        "#};
        File::create(&example_vex_path)
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&example_vex_path),
                action: IOAction::Create,
                cause,
            })?
            .write_all(EXAMPLE_VEX_CONTENT.as_bytes())
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&example_vex_path),
                action: IOAction::Write,
                cause,
            })?;
        Ok(())
    }

    pub fn associations(&self) -> Result<Associations> {
        let mut ret = Associations::base();
        self.manifest
            .language_options
            .iter()
            .map(|(language, options)| {
                let patterns = options
                    .file_associations
                    .iter()
                    .cloned()
                    .map(|pattern| pattern.compile())
                    .collect::<Result<Vec<_>>>();
                (patterns, *language)
            })
            .try_for_each(|(patterns, language)| {
                ret.insert(patterns?, language);
                Ok::<_, Error>(())
            })?;
        Ok(ret)
    }

    pub fn vex_dir(&self) -> Utf8PathBuf {
        self.project_root.join(self.manifest.queries_dir.as_str())
    }
}

impl Deref for Context {
    type Target = Manifest;

    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

#[derive(Debug, Default, Deserialise, Serialise, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Manifest {
    #[serde(flatten)]
    pub language_options: LanguageData,

    #[serde(default)]
    pub queries_dir: QueriesDir,

    #[serde(default, rename = "ignore")]
    pub ignores: IgnoreData,

    #[serde(default, rename = "allow")]
    pub allows: Vec<RawFilePattern<String>>,
}

impl Manifest {
    const FILE_NAME: &'static str = "vex.toml";
    const DEFAULT_CONTENT: &'static str = indoc! {r#"
        ignore = [ "vex.toml", "vexes/", ".git/", ".gitignore", "/target/" ]

        # If this is a C++ project where header files have file-extension .h, uncomment the
        # following lines.
        # ["c++"]
        # use-for = [ "*.h" ]

        [python]
        use-for = [ "*.star" ]
    "#};

    fn init(project_root: impl AsRef<Utf8Path>, force: bool) -> Result<()> {
        let project_root = project_root.as_ref();
        if !force {
            match Manifest::acquire_content_in(project_root) {
                Ok((found_root, _)) => return Err(Error::AlreadyInited { found_root }),
                Err(Error::ManifestNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        let file_path = project_root.join(Self::FILE_NAME);
        let file = File::options()
            .write(true)
            .create_new(!force)
            .open(&file_path)
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&file_path),
                action: IOAction::Write,
                cause,
            })?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(Self::DEFAULT_CONTENT.as_bytes())
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&file_path),
                action: IOAction::Write,
                cause,
            })?;
        Ok(())
    }

    fn acquire_content() -> Result<(Utf8PathBuf, String)> {
        Self::acquire_content_in(&Utf8PathBuf::try_from(env::current_dir().map_err(
            |cause| Error::IO {
                path: PrettyPath::new(Utf8Path::new(".")),
                action: IOAction::Read,
                cause,
            },
        )?)?)
    }

    fn acquire_content_in(dir: &Utf8Path) -> Result<(Utf8PathBuf, String)> {
        let mut project_root = dir.to_path_buf();
        let mut manifest_file = loop {
            match File::open(project_root.join(Self::FILE_NAME)) {
                Ok(f) => break f,
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(Error::IO {
                        path: PrettyPath::new(Utf8Path::new(Self::FILE_NAME)),
                        action: IOAction::Read,
                        cause: e,
                    })
                }
            }
            project_root = project_root
                .parent()
                .ok_or(Error::ManifestNotFound)?
                .to_owned();
        };

        let len_hint = manifest_file.metadata().map(|m| m.len() as usize).ok();
        let raw_data = {
            let mut manifest_raw = String::with_capacity(len_hint.unwrap_or(0));
            manifest_file
                .read_to_string(&mut manifest_raw)
                .map_err(|cause| Error::IO {
                    path: PrettyPath::new(Utf8Path::new(Self::FILE_NAME)),
                    action: IOAction::Read,
                    cause,
                })?;
            manifest_raw
        };

        Ok((project_root, raw_data))
    }
}

#[derive(Debug, Deserialise, Serialise, PartialEq)]
pub struct LanguageData(HashMap<SupportedLanguage, LanguageOptions>);

impl Default for LanguageData {
    fn default() -> Self {
        Self(
            [(
                SupportedLanguage::Python,
                LanguageOptions {
                    file_associations: vec![RawFilePattern::new("*.star".into())],
                },
            )]
            .into_iter()
            .collect(),
        )
    }
}

impl Deref for LanguageData {
    type Target = HashMap<SupportedLanguage, LanguageOptions>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
pub struct LanguageOptions {
    #[serde(rename = "use-for", default)]
    file_associations: Vec<RawFilePattern<String>>,
}

#[derive(Debug, Deserialise, Serialise, PartialEq)]
pub struct QueriesDir(String);

impl QueriesDir {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for QueriesDir {
    fn default() -> Self {
        Self("vexes".into())
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
pub struct IgnoreData(Vec<RawFilePattern<String>>);

impl IgnoreData {
    pub fn into_inner(self) -> Vec<RawFilePattern<String>> {
        self.0
    }
}

impl Default for IgnoreData {
    fn default() -> Self {
        Self(
            ["vex.toml", "vexes/", ".git/", ".gitignore", "/target/"]
                .into_iter()
                .map(Into::into)
                .map(RawFilePattern::new)
                .collect(),
        )
    }
}

impl Deref for IgnoreData {
    type Target = [RawFilePattern<String>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use insta::assert_yaml_snapshot;
    use regex::Regex;
    use toml_edit::Document;

    use crate::{
        cli::MaxProblems,
        scriptlets::{PreinitOptions, PreinitingStore},
        RunData,
    };

    use super::*;

    #[test]
    fn default_manifest_valid() {
        let init_manifest: Manifest =
            toml_edit::de::from_str(Manifest::DEFAULT_CONTENT).expect("default manifest invalid");
        assert!(init_manifest.allows.is_empty());
        assert_eq!(
            init_manifest
                .ignores
                .iter()
                .map(RawFilePattern::to_string)
                .collect::<Vec<_>>(),
            &["vex.toml", "vexes/", ".git/", ".gitignore", "/target/"]
        );

        let raw_manifest: Document = Manifest::DEFAULT_CONTENT.parse().unwrap();
        let formatted = {
            let mut formatted = raw_manifest.clone();
            formatted.fmt();
            formatted
        };
        assert_eq!(raw_manifest.to_string(), formatted.to_string());
    }

    #[test]
    fn init() -> Result<()> {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        // Manifest not found
        let err = Context::acquire_in(&tempdir_path).unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot find manifest, try running `vex init` in the project’s root"
        );

        Context::init(tempdir_path.clone(), false).unwrap();
        let ctx = Context::acquire_in(&tempdir_path).unwrap();
        PreinitingStore::new(&ctx)
            .unwrap()
            .preinit(PreinitOptions::default())
            .unwrap()
            .init()
            .unwrap();

        // Already inited, no-force
        let re = Regex::new("^already inited in a parent directory .*").unwrap();
        let err = Manifest::init(tempdir_path.clone(), false).unwrap_err();
        assert!(
            re.is_match(&err.to_string()),
            "incorrect error, expected {} but got {err}",
            re.as_str()
        );

        // Already inited, force
        Context::init(&tempdir_path, true).unwrap();
        let ctx = Context::acquire_in(&tempdir_path).unwrap();
        PreinitingStore::new(&ctx)
            .unwrap()
            .preinit(PreinitOptions::default())
            .unwrap()
            .init()
            .unwrap();

        Ok(())
    }

    #[test]
    fn init_example() -> Result<()> {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        File::create(tempdir_path.join("test.rs"))
            .unwrap()
            .write_all(
                indoc! {r#"
                fn func() -> i32 {
                    1234567890
                    + 1_234_567_890
                    + 0x1234567890
                    + 0b1111111111
                }
            "#}
                .as_bytes(),
            )
            .unwrap();

        Context::init(&tempdir_path, false)?;
        let ctx = Context::acquire_in(&tempdir_path)?;
        let store = PreinitingStore::new(&ctx)?
            .preinit(PreinitOptions::default())?
            .init()?;
        let RunData { irritations, .. } = crate::vex(&ctx, &store, MaxProblems::Unlimited)?;
        assert_yaml_snapshot!(irritations
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>());

        Ok(())
    }

    #[test]
    fn no_vexes_dir() -> Result<()> {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        File::create(tempdir_path.join("vex.toml")).unwrap();

        let re = Regex::new("^cannot find vexes directory at .*").unwrap();
        let ctx = Context::acquire_in(&tempdir_path).unwrap();
        let err = PreinitingStore::new(&ctx).unwrap_err();
        assert!(
            re.is_match(&err.to_string()),
            "incorrect error, expected {} but got {err}",
            re.as_str()
        );

        Ok(())
    }

    #[test]
    fn defaults() {
        let root_dir = tempfile::tempdir().unwrap();
        let root_path = Utf8PathBuf::try_from(root_dir.path().to_path_buf()).unwrap();

        Context::init(&root_path, false).unwrap();
        let manifest = Context::acquire_in(&root_path).unwrap().manifest;

        assert_eq!(manifest, Manifest::default());
    }
}
