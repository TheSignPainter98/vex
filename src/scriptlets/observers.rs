use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    environment::Module,
    eval::Evaluator,
    values::{Freeze, Freezer, FrozenHeap, FrozenValue, StarlarkValue, Value},
};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType, Trace};

use crate::{
    result::Result,
    scriptlets::{
        action::Action,
        event::{Event, EventKind},
        extra_data::{InvocationData, UnfrozenInvocationData},
        print_handler::PrintHandler,
        Intents,
    },
    source_path::PrettyPath,
};

#[derive(Debug, derive_more::Display, NoSerialize, ProvidesStaticType, Allocative)]
#[display(fmt = "ObserverData")]
pub struct ObserverData {
    on_open_project: Vec<Observer>,
    on_open_file: Vec<Observer>,
}

impl ObserverData {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            on_open_project: Vec::with_capacity(capacity),
            on_open_file: Vec::with_capacity(capacity),
        }
    }

    pub fn empty() -> Self {
        Self {
            on_open_project: Vec::with_capacity(0),
            on_open_file: Vec::with_capacity(0),
        }
    }

    pub fn len(&self) -> usize {
        let Self {
            on_open_project,
            on_open_file,
        } = self;
        on_open_project.len() + on_open_file.len()
    }

    pub fn add_open_project_observer(&mut self, observer: Observer) {
        self.on_open_project.push(observer)
    }

    pub fn add_open_file_observer(&mut self, observer: Observer) {
        self.on_open_file.push(observer)
    }

    pub fn extend(&mut self, other: Self) {
        let Self {
            on_open_project,
            on_open_file,
        } = self;
        on_open_project.extend(other.on_open_project);
        on_open_file.extend(other.on_open_file);
    }

    pub fn handle(&self, event: Event<'_>, frozen_heap: &FrozenHeap) -> Result<Intents> {
        self.observers_for(&event)
            .iter()
            .map(|observer| observer.handle(event.dupe(), frozen_heap))
            .collect()
    }

    fn observers_for(&self, event: &Event<'_>) -> &[Observer] {
        match event.kind() {
            EventKind::OpenProject => &self.on_open_project,
            EventKind::OpenFile => &self.on_open_file,
            EventKind::Match => panic!("internal error: query_match not observable"),
        }
    }
}

#[starlark_value(type = "ObserverData")]
impl<'v> StarlarkValue<'v> for ObserverData {}

#[derive(new, Debug, Trace, Allocative)]
pub struct UnfrozenObserver<'v> {
    vex_path: PrettyPath,
    callback: Value<'v>,
}

impl<'v> Freeze for UnfrozenObserver<'v> {
    type Frozen = Observer;

    fn freeze(self, freezer: &Freezer) -> anyhow::Result<Self::Frozen> {
        let Self { vex_path, callback } = self;
        let callback = callback.freeze(freezer)?;
        Ok(Observer { vex_path, callback })
    }
}

#[derive(new, Debug, Clone, Dupe, Allocative)]
pub struct Observer {
    vex_path: PrettyPath,
    callback: FrozenValue,
}

impl Observer {
    pub fn handle(&self, event: Event<'_>, frozen_heap: &FrozenHeap) -> Result<Intents> {
        let handler_module = Module::new();
        UnfrozenInvocationData::new(Action::Vexing(event.kind()), self.vex_path.dupe())
            .insert_into(&handler_module);
        {
            let mut eval = Evaluator::new(&handler_module);
            eval.set_print_handler(&PrintHandler);

            let func = self.callback.dupe().to_value(); // TODO(kcza): check thread safety! Can this unfrozen
                                                        // function mutate upvalues if it is a closure?
            eval.eval_function(func, &[event.into_value_on(handler_module.heap())], &[])?;
        }

        let handler_module = handler_module.freeze()?;
        frozen_heap.add_reference(handler_module.frozen_heap());
        Ok(InvocationData::get_from(&handler_module).intents().clone())
    }
}
