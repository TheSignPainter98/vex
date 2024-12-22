use camino::{Utf8Path, Utf8PathBuf};
use indoc::indoc;
use log::log_enabled;
use serde::{Deserialize as Deserialise, Serialize as Serialise};

use std::collections::{BTreeMap, HashMap};
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
use crate::warn;

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

        let manifest: Manifest = toml_edit::de::from_str(&raw_data)?;
        if log_enabled!(log::Level::Warn) {
            let suppress_warning = env::var("VEX_LSP").map_or(false, |v| !v.is_empty());
            let lsp_features_used = manifest.run.lsp_enabled
                || manifest
                    .languages
                    .values()
                    .any(|language_options| language_options.language_server.is_some());
            if !suppress_warning && lsp_features_used {
                warn!("LSP features requested but current support is experimental (Set VEX_LSP=1 to suppress this warning)");
            }
        }

        Ok(Context {
            project_root,
            manifest,
        })
    }

    pub fn new_with_manifest(project_root: &Utf8Path, manifest: Manifest) -> Self {
        Self {
            project_root: PrettyPath::new(project_root),
            manifest,
        }
    }

    #[cfg(test)]
    pub fn acquire_in(project_root: &Utf8Path) -> Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_content_in(project_root)?;
        let project_root = PrettyPath::new(&project_root);
        let data = toml_edit::de::from_str(&raw_data)?;
        Ok(Context {
            project_root,
            manifest: data,
        })
    }

    pub fn init(project_root: impl AsRef<Utf8Path>, force: bool) -> Result<()> {
        let project_root = project_root.as_ref();
        fs::create_dir_all(project_root.join(VexesDir::default().as_str())).map_err(|cause| {
            Error::IO {
                path: PrettyPath::new(project_root),
                action: IOAction::Write,
                cause,
            }
        })?;
        Manifest::init(project_root, force)?;

        let example_vex_path = Utf8PathBuf::from(project_root)
            .join(VexesDir::default().as_str())
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
                    'example',
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
            .languages
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
        self.project_root.join(self.manifest.run.vexes_dir.as_str())
    }
}

impl Deref for Context {
    type Target = Manifest;

    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

#[derive(Clone, Debug, Default, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Manifest {
    #[serde(rename = "vex")]
    pub run: RunConfig,

    #[serde(default)]
    pub files: FilesConfig,

    #[serde(default)]
    pub lints: LintsConfig,

    #[serde(default)]
    pub groups: GroupsConfig,

    #[serde(default)]
    pub languages: LanguagesConfig,
}

impl Manifest {
    pub const FILE_NAME: &'static str = "vex.toml";
    const DEFAULT_CONTENT: &'static str = indoc! {r#"
        [vex]
        version = "1"

        [files]
        ignore = [ "vex.toml", "vexes/", ".git/", ".gitignore", "/target/" ]

        [languages.python]
        use-for = [ "*.star" ]
    "#};

    fn init(project_root: impl AsRef<Utf8Path>, force: bool) -> Result<()> {
        let project_root = project_root.as_ref();
        if !force {
            match Manifest::acquire_content_in(project_root) {
                Ok((found_root, _)) => {
                    return Err(Error::AlreadyInited {
                        found_root: PrettyPath::new(&found_root),
                    })
                }
                Err(Error::ManifestNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        let file_path = project_root.join(Self::FILE_NAME);
        let file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
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
                path: PrettyPath::from("."),
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
                        path: PrettyPath::from(Self::FILE_NAME),
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
                    path: PrettyPath::from(Self::FILE_NAME),
                    action: IOAction::Read,
                    cause,
                })?;
            manifest_raw
        };

        Ok((project_root, raw_data))
    }
}

#[derive(Clone, Debug, Default, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub version: Version,

    #[serde(default)]
    #[serde(rename = "enable-lsp")]
    pub lsp_enabled: bool,

    #[serde(default)]
    #[serde(rename = "directory")]
    pub vexes_dir: VexesDir,
}

#[derive(Clone, Debug, Default, Deserialise, Serialise, PartialEq)]
pub enum Version {
    #[default]
    #[serde(rename = "1")]
    V1,
}

impl Version {
    #[allow(dead_code)]
    pub fn current() -> Self {
        Self::V1
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
pub struct VexesDir(Utf8PathBuf);

impl VexesDir {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Default for VexesDir {
    fn default() -> Self {
        Self("vexes".into())
    }
}

#[derive(Clone, Debug, Default, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    #[serde(default, rename = "ignore")]
    pub ignores: IgnoreData,

    #[serde(default, rename = "allow")]
    pub allows: Vec<RawFilePattern<String>>,
}

#[derive(Clone, Debug, Default, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LintsConfig {
    #[serde(rename = "active")]
    pub active_lints_config: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupsConfig {
    #[serde(rename = "active")]
    pub active_groups_config: BTreeMap<String, bool>,
}

impl Default for GroupsConfig {
    fn default() -> Self {
        Self {
            active_groups_config: ["deprecated", "nursery", "pedantic"]
                .into_iter()
                .map(|group| (group.to_owned(), false))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
pub struct LanguagesConfig(HashMap<SupportedLanguage, LanguageOptions>);

impl Default for LanguagesConfig {
    fn default() -> Self {
        Self(
            [(
                SupportedLanguage::Python,
                LanguageOptions {
                    file_associations: vec![RawFilePattern::new("*.star".into())],
                    language_server: None,
                },
            )]
            .into_iter()
            .collect(),
        )
    }
}

impl Deref for LanguagesConfig {
    type Target = HashMap<SupportedLanguage, LanguageOptions>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialise, Serialise, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct LanguageOptions {
    #[serde(rename = "use-for", default)]
    file_associations: Vec<RawFilePattern<String>>,

    language_server: Option<String>,
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
mod tests {
    use insta::assert_yaml_snapshot;
    use regex::Regex;
    use toml_edit::Document;

    use crate::{
        cli::{MaxConcurrentFileLimit, MaxProblems},
        scan::{self, ProjectRunData},
        scriptlets::{source, InitOptions, PreinitOptions, PreinitingStore},
        verbosity::Verbosity,
        warning_filter::WarningFilter,
    };

    use super::*;

    #[test]
    fn default_manifest_valid() {
        let init_manifest: Manifest =
            toml_edit::de::from_str(Manifest::DEFAULT_CONTENT).expect("default manifest invalid");
        assert!(init_manifest.files.allows.is_empty());
        assert_eq!(
            init_manifest
                .files
                .ignores
                .iter()
                .map(RawFilePattern::to_string)
                .collect::<Vec<_>>(),
            &["vex.toml", "vexes/", ".git/", ".gitignore", "/target/"]
        );

        assert_eq!(
            init_manifest
                .groups
                .active_groups_config
                .into_iter()
                .collect::<Vec<_>>(),
            ["deprecated", "nursery", "pedantic"]
                .into_iter()
                .map(|group| (group.to_owned(), false))
                .collect::<Vec<_>>(),
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
        PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)
            .unwrap()
            .preinit(PreinitOptions::default())
            .unwrap()
            .init(InitOptions::default())
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
        PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)
            .unwrap()
            .preinit(PreinitOptions::default())
            .unwrap()
            .init(InitOptions::default())
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
        let store = PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)?
            .preinit(PreinitOptions::default())?
            .init(InitOptions::default())?;
        let ProjectRunData { irritations, .. } = scan::scan_project(
            &ctx,
            &store,
            WarningFilter::all(),
            MaxProblems::Unlimited,
            MaxConcurrentFileLimit::new(1),
            Verbosity::default(),
        )?;
        assert_yaml_snapshot!(irritations
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>());

        Ok(())
    }

    #[test]
    fn defaults() {
        let root_dir = tempfile::tempdir().unwrap();
        let root_path = Utf8PathBuf::try_from(root_dir.path().to_owned()).unwrap();

        Context::init(&root_path, false).unwrap();
        let manifest = Context::acquire_in(&root_path).unwrap().manifest;

        assert_eq!(manifest, Manifest::default());
    }

    #[test]
    fn default_version_is_current() {
        assert_eq!(Version::default(), Version::current());
    }

    #[test]
    fn init_manifest_version_is_current() {
        let root_dir = tempfile::tempdir().unwrap();
        let root_path = Utf8PathBuf::try_from(root_dir.path().to_owned()).unwrap();

        Manifest::init(&root_path, false).unwrap();
        let ctx = Context::acquire_in(&root_path).unwrap();
        assert_eq!(ctx.manifest.run.version, Version::current());
    }

    #[test]
    fn minimal_manifest() {
        toml_edit::de::from_str::<Manifest>("").unwrap_err();
        toml_edit::de::from_str::<Manifest>("[vex]").unwrap_err();

        toml_edit::de::from_str::<Manifest>("[vex]\nversion = '1'").unwrap();
    }

    #[test]
    fn maximal_manifest() {
        let manifest_content = indoc! {r#"
            [vex]
            version = "1"
            enable-lsp = true
            directory = "some-dir/"

            [files]
            ignore = ["vexes/", "target/"]
            allow = ["vexes/check-me.star", "target/check-me.rs"]

            [lints.active]
            lint-id-1 = false
            lint-id-2 = true

            [groups.active]
            group-id-1 = false
            group-id-2 = true

            [languages.python]
            use-for = ["*.star", "*.py2"]
            language-server = "custom-language-server"
        "#};
        let parsed_manifest: Manifest = toml_edit::de::from_str(manifest_content).unwrap();

        assert_eq!(parsed_manifest.run.version, Version::V1);
        assert!(parsed_manifest.run.lsp_enabled);
        assert_eq!(parsed_manifest.run.vexes_dir.as_str(), "some-dir/");
        assert_eq!(parsed_manifest.files.ignores.into_inner().len(), 2);
        assert_eq!(parsed_manifest.files.allows.len(), 2);
        assert_eq!(
            parsed_manifest.lints.active_lints_config,
            BTreeMap::from_iter([("lint-id-1".into(), false), ("lint-id-2".into(), true)])
        );
        assert_eq!(
            parsed_manifest.groups.active_groups_config,
            BTreeMap::from_iter([("group-id-1".into(), false), ("group-id-2".into(), true)])
        );
        assert_eq!(
            parsed_manifest.languages[&SupportedLanguage::Python]
                .file_associations
                .len(),
            2
        );
        assert_eq!(
            parsed_manifest.languages[&SupportedLanguage::Python]
                .language_server
                .as_deref(),
            Some("custom-language-server")
        );
    }
}
