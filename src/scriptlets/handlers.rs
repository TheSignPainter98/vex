use std::sync::Arc;

use derive_new::new;
use dupe::Dupe;
use starlark::values::FrozenValue;
use tree_sitter::Query;

use crate::supported_language::SupportedLanguage;

#[derive(Clone, Debug, Dupe)]
pub struct ScriptletHandlerData {
    pub lang: SupportedLanguage,
    pub query: Arc<Query>,
    pub on_start: Arc<Vec<OnStartHandler>>,
    pub on_match: Arc<Vec<OnMatchHandler>>,
    pub on_eof: Arc<Vec<OnEofHandler>>,
    pub on_end: Arc<Vec<OnEndHandler>>,
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
