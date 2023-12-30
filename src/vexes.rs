use std::{fmt::Display, ops::Deref, sync::Arc};

use annotate_snippets::{Annotation, AnnotationType, Renderer, Slice, Snippet};
use camino::{Utf8Path, Utf8PathBuf};
use enum_map::EnumMap;
use strum::IntoEnumIterator;
use tokio::{fs, sync::OnceCell, task::JoinSet};
use tree_sitter::{Query, QueryCursor, QueryMatch};

use crate::{
    context::{Context, QueriesDir},
    error::Error,
    source_file::SourceFile,
    supported_language::SupportedLanguage,
};

#[derive(Clone)]
pub struct Vexes(Arc<VexesImpl>);

impl Vexes {
    pub fn new(manifest: &Context) -> Self {
        let queries_dir = manifest.project_root.clone().join(
            &manifest
                .manifest
                .queries_dir
                .as_ref()
                .unwrap_or(&QueriesDir::default())
                .0,
        );
        Self(Arc::new(VexesImpl::new(queries_dir)))
    }
}

impl Deref for Vexes {
    type Target = VexesImpl;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct VexesImpl {
    pub project_root: Utf8PathBuf,
    vex_map: EnumMap<SupportedLanguage, OnceCell<VexSet>>,
}

impl VexesImpl {
    pub fn new(project_root: Utf8PathBuf) -> Self {
        Self {
            project_root,
            vex_map: EnumMap::from_iter(
                SupportedLanguage::iter().map(|lang| (lang, OnceCell::new())),
            ),
        }
    }

    pub async fn check(&self, path: Utf8PathBuf) -> anyhow::Result<Vec<Problem>> {
        let Some(extension) = path.extension() else {
            eprintln!("ignoring {path} (no file extension)");
            return Ok(Vec::with_capacity(0));
        };
        let Some(lang) = SupportedLanguage::try_from_extension(extension) else {
            eprintln!("ignoring {path} (no known language)");
            return Ok(Vec::with_capacity(0));
        };

        let src_file = SourceFile::new(path, lang).await?;
        self.vexset(src_file.lang).await?.check(&src_file).await
    }

    async fn vexset(&self, lang: SupportedLanguage) -> anyhow::Result<&VexSet> {
        self.vex_map[lang]
            .get_or_try_init(|| VexSet::new(&self.project_root, lang))
            .await
    }

    pub async fn vexes(&self) -> anyhow::Result<Vec<(SupportedLanguage, VexSet)>> {
        let mut result = Vec::with_capacity(self.vex_map.len());
        for (lang, _) in &self.vex_map {
            result.push((lang.clone(), VexSet::new(&self.project_root, lang).await?));
        }
        Ok(result)
    }
}

pub struct VexSet {
    queries: Vec<Vex>,
}

impl VexSet {
    async fn new(project_root: &Utf8Path, lang: SupportedLanguage) -> anyhow::Result<Self> {
        let vex_dir = project_root.join(lang.name());
        let mut set = JoinSet::new();
        let mut entries = fs::read_dir(vex_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.metadata().await?.is_file() {
                continue;
            }
            set.spawn(async move {
                let vex_path = Utf8PathBuf::try_from(entry.path())?;
                Vex::new(&vex_path, lang).await
            });
        }
        let mut queries = Vec::with_capacity(set.len());
        while let Some(res) = set.join_next().await {
            queries.push(res??);
        }
        Ok(Self { queries })
    }

    async fn check(&self, src_file: &SourceFile) -> anyhow::Result<Vec<Problem>> {
        Ok(self
            .queries
            .iter()
            .flat_map(|query| query.check(src_file))
            // .flatten()
            .collect())
        // TODO(kcza): sort the problems by: (start_idx, end_idx)
        // TODO(kcza): create the snippets here!
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Vex> {
        self.queries.iter()
    }
}

pub struct Vex {
    pub name: String,
    query: Query,
}

impl Vex {
    async fn new(path: &Utf8Path, lang: SupportedLanguage) -> anyhow::Result<Self> {
        let Some(name) = path.file_stem().map(ToString::to_string) else {
            return Err(Error::MissingFileName(path.to_owned()).into());
        };
        let name = name.to_string();

        let query_src = fs::read_to_string(&path).await?;
        let query = Query::new(lang.ts_language(), &query_src)?;

        Ok(Self { name, query })
    }

    fn check(&self, src_file: &SourceFile) -> Vec<Problem> {
        let root = src_file.tree.root_node();
        let src = &src_file.content;
        let mut cursor = QueryCursor::new();
        cursor
            .matches(&self.query, root, src.as_bytes())
            .map(|nit| Problem::new(self, src_file, nit))
            .collect()
        // for m in cursor.matches(&self.query, root, src.as_bytes()) {
        //
        // }
        // let src = &src_file.content;
        // let mut problems = Vec::new();
        //     if let Some(problem) = query.check(src_file) {
        //         problems.push(problem)
        //     }
        //     // let mut cursor = QueryCursor::new();
        //     // // problems.extend(cursor.matches(&query, root, src))
        //     // for m in cursor.matches(&query.query, root, src.as_bytes()) {
        //     //     println!("{m:?}");
        //     //     problems.push(m.clone())
        //     // }
    }
}

#[derive(PartialEq, Eq, PartialOrd)]
pub struct Problem {
    message: String,
    start_byte: usize,
    end_byte: usize,
}

static_assertions::assert_impl_all!(Problem: Send);

impl Problem {
    fn new(source: &Vex, src_file: &SourceFile, query_match: QueryMatch) -> Self {
        let snippet = Snippet {
            title: Some(Annotation {
                id: None,
                label: Some(&source.name),
                annotation_type: AnnotationType::Warning,
            }),
            footer: Vec::with_capacity(0), // TODO(kcza): is vec![] a good
            // substitute?
            slices: query_match
                .captures
                .iter()
                .map(|capture| {
                    let node = capture.node;
                    let range = node.range();
                    Slice {
                        source: &src_file.content[range.start_byte..range.end_byte], // TODO(kcza): .. or ..=?
                        line_start: range.start_point.row,
                        origin: Some(src_file.path.as_str()),
                        annotations: vec![], // TODO(kcza): figure out how to
                        // annotate this!
                        fold: true,
                    }
                })
                .collect(),
            // TODO(kcza): match -> captures -> node -> range()
        };
        Self {
            message: Renderer::styled().render(snippet).to_string(),
            start_byte: query_match
                .captures
                .iter()
                .map(|cap| cap.node.start_byte())
                .min()
                .unwrap_or(0),
            end_byte: query_match
                .captures
                .iter()
                .map(|cap| cap.node.end_byte())
                .max()
                .unwrap_or(usize::MAX),
        }
    }
}

impl Ord for Problem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.start_byte, self.end_byte).cmp(&(other.start_byte, other.end_byte))
    }
}

impl Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
