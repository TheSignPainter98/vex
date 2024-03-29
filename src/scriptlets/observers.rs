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
    error::Error,
    irritation::Irritation,
    result::Result,
    scriptlets::{
        action::Action,
        event::{
            CloseFileEvent, CloseProjectEvent, Event, MatchEvent, OpenFileEvent, OpenProjectEvent,
        },
        extra_data::InvocationData,
        print_handler::PrintHandler,
    },
    source_path::PrettyPath,
    supported_language::SupportedLanguage,
};

#[derive(Clone, Debug, Dupe)]
pub struct ScriptletObserverData {
    pub path: PrettyPath,
    pub lang: SupportedLanguage,
    pub query: Arc<Query>,
    pub on_open_project: Arc<Vec<OpenProjectObserver>>,
    pub on_open_file: Arc<Vec<OpenFileObserver>>,
    pub on_match: Arc<Vec<MatchObserver>>,
    pub on_close_file: Arc<Vec<CloseFileObserver>>,
    pub on_close_project: Arc<Vec<CloseProjectObserver>>,
}

pub trait Observer<'v> {
    type Event: 'v;

    fn function(&self) -> &OwnedFrozenValue;

    fn handle(
        &'v self,
        module: &'v Module,
        path: &PrettyPath,
        event: Self::Event,
    ) -> Result<Vec<Irritation>>
    where
        Self::Event: StarlarkValue<'v> + AllocValue<'v> + Event,
    {
        let extra = InvocationData::new(Action::Vexing(<Self::Event as Event>::TYPE), path.dupe());

        {
            let mut eval = Evaluator::new(module);
            eval.set_print_handler(&PrintHandler);
            extra.insert_into(&mut eval);

            let func = self.function().value(); // TODO(kcza): check thread safety! Can this unfrozen
                                                // function mutate upvalues if it is a closure?
            eval.eval_function(func, &[module.heap().alloc(event)], &[])
                .map_err(Error::starlark)?;
        }

        let irritations = extra.irritations.into_inner().expect("lock poisoned");
        Ok(irritations)
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct OpenProjectObserver(OwnedFrozenValue);

impl Observer<'_> for OpenProjectObserver {
    type Event = OpenProjectEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct OpenFileObserver(OwnedFrozenValue);

impl Observer<'_> for OpenFileObserver {
    type Event = OpenFileEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct MatchObserver(OwnedFrozenValue);

impl<'v> Observer<'v> for MatchObserver {
    type Event = MatchEvent<'v>;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct CloseFileObserver(OwnedFrozenValue);

impl Observer<'_> for CloseFileObserver {
    type Event = CloseFileEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}

#[derive(Clone, Debug, Dupe, new)]
pub struct CloseProjectObserver(OwnedFrozenValue);
impl Observer<'_> for CloseProjectObserver {
    type Event = CloseProjectEvent;

    fn function(&self) -> &OwnedFrozenValue {
        &self.0
    }
}
