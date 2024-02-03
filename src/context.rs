use camino::{Utf8Path, Utf8PathBuf};
use glob::{MatchOptions, Pattern};
use indoc::indoc;
use serde::{Deserialize as Deserialise, Serialize as Serialise};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Read, Write};
use std::ops::Deref;
use std::sync::Arc;
use std::{env, fs};

use crate::error::Error;

#[derive(Debug)]
pub struct Context {
    pub project_root: Arc<Utf8Path>,
    pub manifest: Manifest,
}

impl Context {
    pub fn acquire() -> anyhow::Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_file()?;
        let project_root = Arc::from(project_root);
        let data = toml_edit::de::from_str(&raw_data)?;
        Ok(Context {
            project_root,
            manifest: data,
        })
    }

    #[cfg(test)]
    pub fn acquire_in(dir: &Utf8Path) -> anyhow::Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_file_in(dir)?;
        let project_root = Arc::from(project_root);
        let data = toml_edit::de::from_str(&raw_data)?;
        Ok(Context {
            project_root,
            manifest: data,
        })
    }

    pub fn init(project_root: Utf8PathBuf) -> anyhow::Result<()> {
        fs::create_dir_all(project_root.join(QueriesDir::default().as_str()))?;
        Manifest::init(project_root)
    }

    pub fn vex_dir(&self) -> Utf8PathBuf {
        self.project_root.join(
            self.manifest
                .queries_dir
                .as_ref()
                .unwrap_or(&QueriesDir::default())
                .as_str(),
        )
    }
}

impl Deref for Context {
    type Target = Manifest;

    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

#[derive(Debug, Deserialise, Serialise, PartialEq)]
pub struct Manifest {
    pub associations: Option<HashMap<String, String>>,

    pub queries_dir: Option<QueriesDir>,

    #[serde(rename = "ignore")]
    pub ignores: Option<IgnoreData>,

    #[serde(rename = "allow")]
    pub allows: Option<Vec<FilePattern>>,
}

impl Manifest {
    const FILE_NAME: &'static str = "vex.toml";
    const DEFAULT_CONTENT: &'static str = indoc! {r#"
        ignore = [ ".git/", ".gitignore", "/target/" ]
    "#};

    fn init(project_root: Utf8PathBuf) -> anyhow::Result<()> {
        match Manifest::acquire_file_in(&project_root) {
            Ok((found_root, _)) => return Err(Error::AlreadyInited { found_root }.into()),
            Err(e)
                if e.downcast_ref::<Error>()
                    .map(|e| e != &Error::ManifestNotFound)
                    .unwrap_or(true) =>
            {
                return Err(e)
            }
            _ => {}
        }

        let file = File::options()
            .write(true)
            .create_new(true)
            .open(project_root.join(Self::FILE_NAME))?;
        let mut writer = BufWriter::new(file);
        writer.write_all(Self::DEFAULT_CONTENT.as_bytes())?;
        Ok(())
    }

    fn acquire_file() -> anyhow::Result<(Utf8PathBuf, String)> {
        Self::acquire_file_in(&Utf8PathBuf::try_from(env::current_dir()?)?)
    }

    fn acquire_file_in(dir: &Utf8Path) -> anyhow::Result<(Utf8PathBuf, String)> {
        let mut project_root = dir.to_path_buf();
        let mut manifest_file = loop {
            match File::open(project_root.join(Self::FILE_NAME)) {
                Ok(f) => break f,
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            project_root = project_root
                .parent()
                .ok_or(Error::ManifestNotFound)?
                .to_owned();
        };

        let len_hint = manifest_file.metadata().map(|m| m.len() as usize).ok();
        let raw_data = {
            let mut manifest_raw = String::with_capacity(len_hint.unwrap_or(0));
            manifest_file.read_to_string(&mut manifest_raw)?;
            manifest_raw
        };

        Ok((project_root, raw_data))
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            associations: None,
            queries_dir: Some(QueriesDir::default()),
            ignores: Some(IgnoreData::default()),
            allows: None,
        }
    }
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
pub struct IgnoreData(pub Vec<FilePattern>);

impl Default for IgnoreData {
    fn default() -> Self {
        Self(
            ["/.git", "/target"]
                .into_iter()
                .map(FilePattern::new)
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
pub struct FilePattern(String);

impl FilePattern {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    pub fn compile(self, project_root: &Utf8Path) -> anyhow::Result<CompiledFilePattern> {
        Ok(CompiledFilePattern(Pattern::new(
            if self.0.starts_with('/') {
                // absolute
                project_root.join(Utf8PathBuf::from(&self.0[1..]))
            } else {
                // relative
                project_root
                    .join(Utf8PathBuf::from("**".to_string()))
                    .join(Utf8PathBuf::from(&self.0))
            }
            .as_str(),
        )?))
    }
}

#[cfg(test)]
impl FilePattern {
    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub struct CompiledFilePattern(Pattern);

impl CompiledFilePattern {
    pub fn matches_path(&self, path: &Utf8Path) -> bool {
        self.0.matches_path_with(
            path.as_std_path(),
            MatchOptions {
                case_sensitive: true,
                require_literal_separator: true,
                require_literal_leading_dot: true,
            },
        )
    }
}

#[cfg(test)]
mod test {
    use regex::Regex;
    use toml_edit::Document;

    use crate::scriptlets::PreinitingStore;

    use super::*;

    #[test]
    fn default_manifest_valid() {
        let init_manifest: Manifest =
            toml_edit::de::from_str(Manifest::DEFAULT_CONTENT).expect("default manifest invalid");
        assert!(init_manifest.allows.is_none());
        assert_eq!(
            init_manifest
                .ignores
                .expect("default ignores are not set")
                .0
                .iter()
                .map(FilePattern::as_str)
                .collect::<Vec<_>>(),
            &[".git/", ".gitignore", "/target/"]
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
    fn init() -> anyhow::Result<()> {
        let tempdir = tempfile::tempdir()?;
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        // Manifest not found
        let err = Context::acquire_in(&tempdir_path).unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot find manifest, try running `vex init` in the project’s root"
        );

        Context::init(tempdir_path.clone()).unwrap();
        let ctx = Context::acquire_in(&tempdir_path).unwrap();
        PreinitingStore::new(&ctx)
            .unwrap()
            .preinit()
            .unwrap()
            .init()
            .unwrap();

        // Already inited
        let re = Regex::new("^already inited in a parent directory .*").unwrap();
        let err = Manifest::init(tempdir_path.clone()).unwrap_err();
        assert!(
            re.is_match(&err.to_string()),
            "incorrect error, expected {} but got {err}",
            re.as_str()
        );

        Ok(())
    }

    #[test]
    fn no_vexes_dir() -> anyhow::Result<()> {
        let tempdir = tempfile::tempdir()?;
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        File::create(tempdir_path.join("vex.toml"))?;

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
}
