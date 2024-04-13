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
            CloseFileEvent, CloseProjectEvent, Event, OpenFileEvent, OpenProjectEvent,
            QueryMatchEvent,
        },
        extra_data::InvocationData,
        print_handler::PrintHandler,
    },
    source_path::PrettyPath,
    trigger::{Trigger, TriggerId},
};

#[derive(Debug)]
pub struct ScriptletObserverData {
    pub vex_path: PrettyPath,
    pub triggers: Vec<Arc<Trigger>>,
    pub on_open_project: Vec<OpenProjectObserver>,
    pub on_open_file: Vec<OpenFileObserver>,
    pub on_match: Vec<MatchObserver>,
    pub on_close_file: Vec<CloseFileObserver>,
    pub on_close_project: Vec<CloseProjectObserver>,
}

impl ScriptletObserverData {
    pub fn trigger_queries(&self) -> impl Iterator<Item = (Option<&TriggerId>, &Query)> {
        self.triggers.iter().filter_map(|trigger| {
            let Some(query) = trigger
                .content_trigger
                .as_ref()
                .and_then(|ct| ct.query.as_ref())
            else {
                return None;
            };
            Some((trigger.id.as_ref(), query))
        })
    }
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
    type Event = QueryMatchEvent<'v>;

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
