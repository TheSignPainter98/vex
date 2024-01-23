use std::sync::Arc;

use derive_new::new;
use dupe::Dupe;
use starlark::{
    environment::Module,
    eval::Evaluator,
    values::{AllocValue, OwnedFrozenValue, StarlarkValue},
};
use tree_sitter::Query;

use crate::{
    scriptlets::{
        action::Action,
        event::Event,
        event::{CloseFileEvent, CloseProjectEvent, MatchEvent, OpenFileEvent, OpenProjectEvent},
        extra_data::InvocationData,
        print_handler::PrintHandler,
    },
    supported_language::SupportedLanguage,
};

#[derive(Clone, Debug, Dupe)]
pub struct ScriptletObserverData {
    pub lang: SupportedLanguage,
    pub query: Arc<Query>,
    pub on_open_project: Arc<Vec<OpenProjectObserver>>,
    pub on_open_file: Arc<Vec<OpenFileObserver>>,
    pub on_match: Arc<Vec<MatchObserver>>,
    pub on_close_file: Arc<Vec<CloseFileObserver>>,
    pub on_close_project: Arc<Vec<CloseProjectObserver>>,
}

pub trait Observer {
    type Event;

    fn function(&self) -> &OwnedFrozenValue;

    fn handle(&self, event: Self::Event) -> anyhow::Result<()>
    where
        Self::Event: for<'v> StarlarkValue<'v> + for<'v> AllocValue<'v> + Event,
    {
        let extra = InvocationData::new(Action::Vexing(<Self::Event as Event>::TYPE));
        let module = Module::new();

        let print_handler = PrintHandler::new("asdf"); // TODO(kzca): what should the tag be here?
        let mut eval = Evaluator::new(&module);
        eval.set_print_handler(&print_handler);
        extra.insert_into(&mut eval);

        let func = self.function().value(); // TODO(kcza): check thread safety! Can this unfrozen
                                            // function mutate upvalues if it is a closure?
        eval.eval_function(func, &[module.heap().alloc(event)], &[])?;
        Ok(())
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct OpenProjectObserver(OwnedFrozenValue);

impl Observer for OpenProjectObserver {
    type Event = OpenProjectEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct OpenFileObserver(OwnedFrozenValue);

impl Observer for OpenFileObserver {
    type Event = OpenFileEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct MatchObserver(OwnedFrozenValue);

impl Observer for MatchObserver {
    type Event = MatchEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct CloseFileObserver(OwnedFrozenValue);

impl Observer for CloseFileObserver {
    type Event = CloseFileEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct CloseProjectObserver(OwnedFrozenValue);
impl Observer for CloseProjectObserver {
    type Event = CloseProjectEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}
