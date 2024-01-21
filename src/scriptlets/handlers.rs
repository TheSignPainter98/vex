use derive_new::new;
use dupe::Dupe;
use starlark::values::FrozenValue;
use tree_sitter::Query;

use crate::supported_language::SupportedLanguage;

#[derive(Debug)]
pub struct ScriptletHandlerData {
    pub lang: SupportedLanguage,
    pub query: Query,
    pub on_start: Vec<OnStartHandler>,
    pub on_match: Vec<OnMatchHandler>,
    pub on_eof: Vec<OnEofHandler>,
    pub on_end: Vec<OnEndHandler>,
}

pub trait Handler {
    type Event;

    fn handle(&self, e: Self::Event) -> anyhow::Result<()>;
}
// TODO(kcza): implement Handler for the handler types

#[derive(Clone, Debug, Dupe, new)]
pub struct OnStartHandler(FrozenValue);

#[derive(Clone, Debug, Dupe, new)]
pub struct OnMatchHandler(FrozenValue);

#[derive(Clone, Debug, Dupe, new)]
pub struct OnEofHandler(FrozenValue);

#[derive(Clone, Debug, Dupe, new)]
pub struct OnEndHandler(FrozenValue);
