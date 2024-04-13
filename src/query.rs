use allocative::Allocative;
use starlark::values::Trace;
use tree_sitter::Query as TSQuery;

use crate::supported_language::SupportedLanguage;

#[derive(Debug, Trace, Allocative)]
pub struct Query {
    pub name: Option<String>,
    #[allocative(skip)]
    pub language: SupportedLanguage,
    #[allocative(skip)]
    pub query: TSQuery,
}
