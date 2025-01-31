use camino::{Utf8Path, Utf8PathBuf};
use dupe::Dupe;
use indoc::{indoc, writedoc};
use lazy_static::lazy_static;
use log::log_enabled;
use regex::Regex;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use starlark::values::StringValue;
use tree_sitter::Language as TSLanguage;
use tree_sitter_loader::{CompileConfig, Loader};

use std::borrow::{Borrow, Cow};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{BufWriter, ErrorKind, Read, Write as _};
use std::ops::Deref;
use std::sync::Arc;
use std::{
    env,
    fs::{self, File},
};
use std::{fmt, slice};

use crate::arena_map::ArenaMap;
use crate::associations::Associations;
use crate::error::{
    Error, ExternalLanguageError, IOAction, InvalidIDReason, InvalidIgnoreQueryReason,
};
use crate::id::Id;
use crate::language::Language;
use crate::query::Query;
use crate::result::Result;
use crate::scriptlets::query_cache::QueryCacheForLanguage;
use crate::source_path::PrettyPath;
use crate::trigger::RawFilePattern;
use crate::warn;

#[derive(Debug)]
pub struct Context {
    pub project_root: PrettyPath,
    pub manifest: Manifest,
    languages: ArenaMap<Language, Option<LanguageData>>,
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

        let languages = ArenaMap::new();
        Ok(Context {
            project_root,
            manifest,
            languages,
        })
    }

    pub fn new_with_manifest(project_root: &Utf8Path, manifest: Manifest) -> Self {
        Self {
            project_root: PrettyPath::new(project_root),
            manifest,
            languages: ArenaMap::new(),
        }
    }

    #[cfg(test)]
    pub fn acquire_in(project_root: &Utf8Path) -> Result<Self> {
        let (project_root, raw_data) = Manifest::acquire_content_in(project_root)?;
        let project_root = PrettyPath::new(&project_root);
        let manifest = toml_edit::de::from_str(&raw_data)?;

        let languages = ArenaMap::new();
        Ok(Context {
            project_root,
            manifest,
            languages,
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
                (patterns, language.dupe())
            })
            .try_for_each(|(patterns, language)| {
                ret.insert(patterns?, language);
                Ok::<_, Error>(())
            })?;
        Ok(ret)
    }

    pub fn language_data(&self, language: &Language) -> Result<Option<&LanguageData>> {
        self.languages
            .get_or_init(language, || {
                if language.is_builtin() {
                    return LanguageData::load(language.dupe(), &self.project_root);
                }

                if let Some(opts) = self.manifest.languages.get(language) {
                    LanguageData::load_with_options(language.dupe(), opts, &self.project_root)
                } else {
                    Err(Error::ExternalLanguage {
                        language: language.dupe(),
                        cause: ExternalLanguageError::NoConfig(language.dupe()),
                    })
                }
            })
            .map(Option::as_ref)
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Manifest {
    #[serde(rename = "vex")]
    pub run: RunConfig,

    #[serde(default)]
    pub files: FilesConfig,

    #[serde(rename = "args")]
    #[serde(default)]
    pub script_args: ScriptArgs,

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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    #[serde(default, rename = "ignore")]
    pub ignores: IgnoreData,

    #[serde(default, rename = "allow")]
    pub allows: Vec<RawFilePattern<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ScriptArgs(BTreeMap<Id, ScriptArgsForId>);

impl Deref for ScriptArgs {
    type Target = BTreeMap<Id, ScriptArgsForId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScriptArgsForId(BTreeMap<ScriptArgKey, ScriptArgValue>);

impl Deref for ScriptArgsForId {
    type Target = BTreeMap<ScriptArgKey, ScriptArgValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct ScriptArgKey(String);

impl Borrow<str> for ScriptArgKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Deref for ScriptArgKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for ScriptArgKey {
    type Error = Error;

    fn try_from(raw_key: String) -> Result<Self> {
        let invalid_key = |reason| Error::InvalidScriptArgKey {
            raw_key: raw_key.clone(),
            reason,
        };

        // NOTE: these should be the same as the min and max values for Id.
        const MIN_KEY_LEN: usize = 3;
        const MAX_KEY_LEN: usize = 25;
        if raw_key.len() < MIN_KEY_LEN {
            return Err(invalid_key(InvalidIDReason::TooShort {
                len: raw_key.len(),
                min_len: MIN_KEY_LEN,
            }));
        }
        if raw_key.len() > MAX_KEY_LEN {
            return Err(invalid_key(InvalidIDReason::TooLong {
                len: raw_key.len(),
                max_len: MAX_KEY_LEN,
            }));
        }

        lazy_static! {
            static ref VALID_KEY_ID: Regex = Regex::new("^[a-z][a-z0-9-]*[a-z0-9]$").unwrap();
        }
        if !VALID_KEY_ID.is_match(&raw_key) {
            return Err(invalid_key(InvalidIDReason::IllegalChar));
        }

        if let Some(index) = raw_key.find("--") {
            return Err(invalid_key(InvalidIDReason::UglySubstring {
                found: "--".to_owned(),
                index,
            }));
        }

        Ok(Self(raw_key))
    }
}

impl<'de> Deserialize<'de> for ScriptArgKey {
    fn deserialize<D>(deserializer: D) -> std::prelude::v1::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(ScriptArgKeyVisitor)
    }
}

struct ScriptArgKeyVisitor;

impl<'de> Visitor<'de> for ScriptArgKeyVisitor {
    type Value = ScriptArgKey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, v: &str) -> std::prelude::v1::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_borrowed_str(v)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> std::prelude::v1::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_string(v.to_owned())
    }

    fn visit_string<E>(self, v: String) -> std::prelude::v1::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        ScriptArgKey::try_from(v).map_err(E::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
#[serde(expecting = "invalid type: expecting a bool, int, float, string, sequence or table")]
pub enum ScriptArgValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Sequence(Vec<ScriptArgValue>),
    Table(BTreeMap<String, ScriptArgValue>),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LintsConfig {
    #[serde(rename = "active")]
    pub active_lints_config: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct LanguagesConfig(HashMap<Language, LanguageOptions>);

impl Default for LanguagesConfig {
    fn default() -> Self {
        Self(
            [(
                Language::Python,
                LanguageOptions {
                    file_associations: vec![RawFilePattern::new("*.star".into())],
                    language_server: None,
                    parser_dir: None,
                    ignore_query: None, // Guess.
                },
            )]
            .into_iter()
            .collect(),
        )
    }
}

impl Deref for LanguagesConfig {
    type Target = HashMap<Language, LanguageOptions>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct LanguageOptions {
    #[serde(rename = "use-for", default)]
    file_associations: Vec<RawFilePattern<String>>,

    language_server: Option<LanguageServerCommand>,

    parser_dir: Option<Utf8PathBuf>,

    ignore_query: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
#[serde(expecting = "invalid type: expected string or sequence")]
pub enum LanguageServerCommand {
    JustName(String),
    NameWithArgs(Vec<String>),
}

impl LanguageServerCommand {
    #[allow(unused)]
    pub fn parts(&self) -> impl Iterator<Item = &str> {
        let parts_slice = match self {
            Self::JustName(cmd) => slice::from_ref(cmd),
            Self::NameWithArgs(cmd) => cmd,
        };
        parts_slice.iter().map(String::as_str)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Dupe)]
pub struct LanguageData(Arc<LanguageDataInner>);

#[derive(Debug)]
struct LanguageDataInner {
    language: Language,
    ts_language: TSLanguage,
    ignore_query: Option<Query>,
    query_cache: QueryCacheForLanguage,
}

impl LanguageData {
    pub(crate) fn load(language: Language, project_root: &Utf8Path) -> Result<Option<Self>> {
        Self::load_with_options(language, &LanguageOptions::default(), project_root)
    }

    pub(crate) fn load_with_options(
        language: Language,
        language_options: &LanguageOptions,
        project_root: &Utf8Path,
    ) -> Result<Option<Self>> {
        let (ts_language, raw_ignore_query) = match language {
            Language::Go => {
                let ts_language = TSLanguage::from(tree_sitter_go::LANGUAGE);
                let raw_ignore_query = Some(Cow::from(indoc! {r#"
                    (
                        (comment) @marker (#match? @marker "^/[/*] *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#}));
                (ts_language, raw_ignore_query)
            }
            Language::Python => {
                let ts_language = TSLanguage::from(tree_sitter_python::LANGUAGE);
                let raw_ignore_query = Some(Cow::from(indoc! {r#"
                    (
                        (comment) @marker (#match? @marker "^# *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#}));
                (ts_language, raw_ignore_query)
            }
            Language::Rust => {
                let ts_language = TSLanguage::from(tree_sitter_rust::LANGUAGE);
                let raw_ignore_query = Some(Cow::from(indoc! {r#"
                    (
                        (line_comment) @marker (#match? @marker "^// *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#}));
                (ts_language, raw_ignore_query)
            }
            Language::External(_) => {
                let ts_language =
                    Self::load_ts_language(&language, language_options, project_root)?;
                let raw_ignore_query = language_options
                    .ignore_query
                    .as_ref()
                    .map(Cow::from)
                    .or_else(|| Self::guess_raw_ignore_query(&ts_language).map(Cow::from));
                (ts_language, raw_ignore_query)
            }
        };
        let ignore_query = raw_ignore_query
            .map(|raw_ignore_query| {
                Query::new(&ts_language, &raw_ignore_query).map_err(|err| {
                    Error::InvalidIgnoreQuery(InvalidIgnoreQueryReason::General(Box::new(err)))
                })
            })
            .transpose()?;
        let query_cache = QueryCacheForLanguage::new();
        let inner = LanguageDataInner {
            language,
            ts_language,
            ignore_query,
            query_cache,
        };
        Ok(Some(Self(Arc::new(inner))))
    }

    fn load_ts_language(
        language: &Language,
        language_options: &LanguageOptions,
        project_root: &Utf8Path,
    ) -> Result<TSLanguage> {
        let LanguageOptions { parser_dir, .. } = language_options;
        let parser_dir = parser_dir.as_ref().ok_or_else(|| Error::ExternalLanguage {
            language: language.dupe(),
            cause: ExternalLanguageError::MissingParserDir,
        })?;

        let loader = Loader::new()?;
        let src_path = project_root.join(parser_dir).join("src");
        let compile_config = CompileConfig {
            name: language.name().to_owned(),
            ..CompileConfig::new(src_path.as_std_path(), None, None)
        };
        loader
            .load_language_at_path_with_name(compile_config)
            .map_err(|cause| Error::InaccessibleParserFiles {
                language: language.dupe(),
                cause,
            })
    }

    fn guess_raw_ignore_query(ts_language: &TSLanguage) -> Option<String> {
        const KNOWN_COMMENT_NODES: [&str; 2] = ["comment", "line_comment"];

        let mut defined_comment_nodes = KNOWN_COMMENT_NODES.map(|_| false);
        for id in 0..(ts_language.node_kind_count() as u16) {
            if !ts_language.node_kind_is_visible(id) || !ts_language.node_kind_is_named(id) {
                continue;
            }
            let node_kind = match ts_language.node_kind_for_id(id) {
                Some(nk) => nk,
                None => continue,
            };

            if let Some(node_idx) = KNOWN_COMMENT_NODES
                .iter()
                .position(|marker| marker == &node_kind)
            {
                defined_comment_nodes[node_idx] = true
            }
        }

        if defined_comment_nodes.iter().all(|defined| !defined) {
            return None;
        }

        let guess = {
            let mut guess = String::new();
            for comment_node_kind in defined_comment_nodes {
                writedoc!(
                    &mut guess,
                    r#"
                        (
                            ({comment_node_kind}) @marker (#match? @marker "vex:ignore")
                            .
                            (_)? @ignore
                        )
                    "#
                )
                .expect("internal error: cannot write to String");
            }
            guess
        };
        Some(guess)
    }

    pub fn language(&self) -> &Language {
        &self.0.language
    }

    pub fn ts_language(&self) -> &TSLanguage {
        &self.0.ts_language
    }

    pub fn ignore_query(&self) -> Option<&Query> {
        self.0.ignore_query.as_ref()
    }

    pub fn get_or_create_query(&self, raw_query: &StringValue<'_>) -> Result<Arc<Query>> {
        let query_hash = raw_query.get_hashed().hash(); // This hash value is only 32 bits long.

        if let Some(cached_query) = self.0.query_cache.get(query_hash) {
            return Ok(cached_query);
        }

        let query = Arc::new(Query::new(&self.0.ts_language, raw_query)?);
        self.0.query_cache.put(query_hash, query.dupe());
        Ok(query)
    }
}

#[cfg(test)]
mod tests {
    use indoc::formatdoc;
    use insta::assert_yaml_snapshot;
    use regex::Regex;
    use toml_edit::Document;

    use crate::{
        cli::{MaxConcurrentFileLimit, MaxProblems},
        scan::{self, ProjectRunData},
        scriptlets::{source, InitOptions, PreinitOptions, PreinitingStore, ScriptArgsValueMap},
        verbosity::Verbosity,
        vextest::VexTest,
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
        let preinit_options = PreinitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        let init_options = InitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)
            .unwrap()
            .preinit(&ctx, preinit_options)
            .unwrap()
            .init(&ctx, init_options)
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
        let preinit_options = PreinitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        let init_options = InitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)
            .unwrap()
            .preinit(&ctx, preinit_options)
            .unwrap()
            .init(&ctx, init_options)
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
        let preinit_options = PreinitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        let init_options = InitOptions {
            script_args: &ScriptArgsValueMap::new(),
            verbosity: Verbosity::default(),
        };
        let store = PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)?
            .preinit(&ctx, preinit_options)?
            .init(&ctx, init_options)?;
        let ProjectRunData { irritations, .. } = scan::scan_project(
            &ctx,
            &store,
            WarningFilter::all(),
            MaxProblems::Unlimited,
            MaxConcurrentFileLimit::new(1),
            &ScriptArgsValueMap::new(),
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
        use ScriptArgValue as SAV;

        let manifest_content = indoc! {r#"
            [vex]
            version = "1"
            enable-lsp = true
            directory = "some-dir/"

            [files]
            ignore = ["vexes/", "target/"]
            allow = ["vexes/check-me.star", "target/check-me.rs"]

            [args]
            hello.world = [true, 123, 123.4, "foo", {bar = ["baz"]}]

            [lints.active]
            lint-id-1 = false
            lint-id-2 = true

            [groups.active]
            group-id-1 = false
            group-id-2 = true

            [languages.python]
            use-for = ["*.star", "*.py2"]
            language-server = [ "custom-language-server", "with-arg" ]
            ignore-query = '(_) . (_)'

            [languages.some-custom-language]
            use-for = ["*.custom"]
            language-server = "custom-ls"
            ignore-query = '(_) . (_)'
        "#};
        let parsed_manifest: Manifest = toml_edit::de::from_str(manifest_content).unwrap();

        assert_eq!(parsed_manifest.run.version, Version::V1);
        assert!(parsed_manifest.run.lsp_enabled);
        assert_eq!(parsed_manifest.run.vexes_dir.as_str(), "some-dir/");
        assert_eq!(parsed_manifest.files.ignores.into_inner().len(), 2);
        assert_eq!(parsed_manifest.files.allows.len(), 2);
        {
            let hello_id = Id::try_from("hello".to_owned()).unwrap();
            assert_eq!(
                parsed_manifest
                    .script_args
                    .get(&hello_id)
                    .unwrap()
                    .get("world")
                    .unwrap(),
                &SAV::Sequence(vec![
                    SAV::Bool(true),
                    SAV::Int(123),
                    SAV::Float(123.4),
                    SAV::String("foo".into()),
                    SAV::Table(BTreeMap::from_iter([(
                        "bar".to_owned(),
                        SAV::Sequence(vec![SAV::String("baz".into())]),
                    )])),
                ])
            );
        }
        assert_eq!(
            parsed_manifest.lints.active_lints_config,
            BTreeMap::from_iter([("lint-id-1".into(), false), ("lint-id-2".into(), true)])
        );
        assert_eq!(
            parsed_manifest.groups.active_groups_config,
            BTreeMap::from_iter([("group-id-1".into(), false), ("group-id-2".into(), true)])
        );
        assert_eq!(
            parsed_manifest.languages[&Language::Python]
                .file_associations
                .len(),
            2
        );
        assert_eq!(
            parsed_manifest.languages[&Language::Python]
                .language_server
                .as_ref()
                .unwrap()
                .parts()
                .collect::<Vec<_>>(),
            ["custom-language-server", "with-arg"],
        );
        assert_eq!(
            parsed_manifest.languages[&Language::Python]
                .ignore_query
                .as_ref()
                .unwrap(),
            "(_) . (_)"
        );
        let custom_language = Language::External(Arc::from("some-custom-language"));
        assert_eq!(
            parsed_manifest.languages[&custom_language]
                .file_associations
                .len(),
            1
        );
        assert_eq!(
            parsed_manifest.languages[&custom_language]
                .language_server
                .as_ref()
                .unwrap()
                .parts()
                .collect::<Vec<_>>(),
            ["custom-ls"],
        );
        assert_eq!(
            parsed_manifest.languages[&custom_language]
                .ignore_query
                .as_ref()
                .unwrap(),
            "(_) . (_)"
        );
    }

    #[test]
    fn minimal_language_server() {
        let manifest_content = indoc! {r#"
            [vex]
            version = "1"
            enable-lsp = true

            [languages.python]
            language-server = "custom-language-server"
        "#};
        let parsed_manifest: Manifest = toml_edit::de::from_str(manifest_content).unwrap();

        assert_eq!(
            parsed_manifest.languages[&Language::Python]
                .language_server
                .as_ref()
                .unwrap()
                .parts()
                .collect::<Vec<_>>(),
            ["custom-language-server"],
        );
    }

    #[test]
    fn script_arg_key_sanitisation() {
        ScriptArgKey::try_from("xyz0-9yxz".to_owned()).unwrap();

        ScriptArgKey::try_from("xy".to_owned()).unwrap_err();
        ScriptArgKey::try_from("CAPITALS".to_owned()).unwrap_err();
        ScriptArgKey::try_from("under_score".to_owned()).unwrap_err();
        ScriptArgKey::try_from("sm:le".to_owned()).unwrap_err();
        ScriptArgKey::try_from("-starts-with-dash".to_owned()).unwrap_err();
        ScriptArgKey::try_from("ends-with-dash-".to_owned()).unwrap_err();
        ScriptArgKey::try_from("1-starts-with-num".to_owned()).unwrap_err();
        ScriptArgKey::try_from("double--dash".to_owned()).unwrap_err();
    }

    #[test]
    fn manifest_script_arg_key_sanitisation() {
        const EXPECTED_ERR: &str = "too few characters";

        let err = {
            let raw_manifest = formatdoc! {r#"
                [vex]
                version = "1"

                [args]
                too-short.v = 1
            "#};
            toml_edit::de::from_str::<Manifest>(&raw_manifest).unwrap_err()
        };
        assert!(
            err.to_string().contains(EXPECTED_ERR),
            "incorrect error: should contain {EXPECTED_ERR} but got {err}"
        );
    }

    #[test]
    fn args_must_be_namespaced() {
        const EXPECTED_ERR: &str = "invalid type";

        let err = {
            let raw_manifest = formatdoc! {r#"
                [vex]
                version = "1"

                [args]
                unnamespaced-arg = true
            "#};
            toml_edit::de::from_str::<Manifest>(&raw_manifest).unwrap_err()
        };
        assert!(
            err.to_string().contains(EXPECTED_ERR),
            "incorrect error: should contain {EXPECTED_ERR} but got {err}"
        );
    }

    #[test]
    fn load_custom_parser() {
        const PARSER_LINK: &str = "vexes/tree-sitter-lua";
        let run_data = VexTest::new("lua")
            .with_manifest(formatdoc! {r#"
                [vex]
                version = "1"

                [languages.lua]
                use-for = ['*.lua']
                parser-dir = '{PARSER_LINK}'
            "#})
            .with_parser_dir_link("test-data/tree-sitter-lua", PARSER_LINK)
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'lua',
                            '''
                                (function_call) @func
                            ''',
                            on_match,
                        )

                    def on_match(event):
                        func = event.captures['func']
                        vex.warn('test-id', 'found a function', at=func)
                "#},
            )
            .with_source_file(
                "main.lua",
                indoc! {r#"
                    print('hello')
                "#},
            )
            .try_run()
            .unwrap();
        assert_eq!(run_data.irritations.len(), 1);
    }

    #[test]
    fn load_missing_parser() {
        VexTest::new("brainfuck")
            .with_manifest(indoc! {r#"
                [vex]
                version = "1"

                [languages.brainfuck]
                use-for = ['*.bf']
                parser-dir = 'i/do/not/exist'
            "#})
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'brainfuck',
                            '''
                                (function_call) @func
                            ''',
                            lambda _: fail('on_match called'),
                        )
                        fail('vex.search returned successfully')
                "#},
            )
            .with_source_file(
                "main.lua",
                indoc! {r#"
                    print('hello')
                "#},
            )
            .returns_error("cannot load brainfuck parser");
    }

    #[test]
    fn invalid_ignore_query() {
        Assert::query("empty", "").causes_error("query is empty");
        Assert::query("malformed", ")").causes_error("syntax");
        Assert::query("missing-marker", "(_) @ignore")
            .causes_error("missing capture group 'marker'");
        Assert::query("missing-ignore", "(_) @marker")
            .causes_error("missing capture group 'ignore'");
        Assert::query("does-not-capture-marker", "(comment) @marker . (_) @ignore")
            .causes_error("query did not capture 'vex:ignore' marker");

        // test structs.
        struct Assert {
            name: &'static str,
            query: &'static str,
        }

        impl Assert {
            fn query(name: &'static str, query: &'static str) -> Self {
                Self { name, query }
            }

            fn causes_error(self, error: &'static str) {
                let Self { name, query } = self;

                const PARSER_LINK: &str = "vexes/tree-sitter-lua";
                VexTest::new(name)
                    .with_manifest(formatdoc! {r#"
                        [vex]
                        version = "1"

                        [languages.lua]
                        use-for = ['*.lua']
                        parser-dir = '{PARSER_LINK}/'
                        ignore-query = '{query}'
                    "#})
                    .with_parser_dir_link("test-data/tree-sitter-lua", PARSER_LINK)
                    .with_scriptlet(
                        "vexes/test.star",
                        indoc! {r#"
                            def init():
                                vex.observe('open_project', on_open_project)

                            def on_open_project(event):
                                vex.search(
                                    'lua',
                                    '''
                                        (number) @func
                                    ''',
                                    on_match,
                                )

                            def on_match(event):
                                fail('uh oh')

                        "#},
                    )
                    .with_source_file(
                        "src/main.lua",
                        indoc! {r#"
                            -- invalid marker
                            print('hello, world')
                        "#},
                    )
                    .returns_error(error);
            }
        }
    }
}
