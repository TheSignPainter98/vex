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
        action::Action, event::Event, extra_data::UnfrozenInvocationData,
        print_handler::PrintHandler,
    },
    source_path::PrettyPath,
};

use super::{event::EventKind, extra_data::InvocationData, Intents};

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
            EventKind::QueryMatch => panic!("internal error: query_match not observable"),
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

// pub trait Observer<'v> {
//     type Event: 'v;
//
//     fn vex_path(&self) -> &PrettyPath;
//
//     fn callback(&self) -> &OwnedFrozenValue;
//
//     fn handle(
//         &self,
//         module: &'v Module,
//         event: Self::Event,
//     ) -> Result<(Vec<Irritation>, ObserverDataBuilder<'v>)>
//     where
//         Self::Event: StarlarkValue<'v> + AllocValue<'v> + Event,
//     {
//         let extra = InvocationData::new(
//             Action::Vexing(<Self::Event as Event>::TYPE),
//             self.vex_path().dupe(),
//         );
//
//         {
//             let mut eval = Evaluator::new(module);
//             eval.set_print_handler(&PrintHandler);
//             extra.insert_into(&mut eval);
//
//             let func = self.callback().value(); // TODO(kcza): check thread safety! Can this unfrozen
//                                                 // function mutate upvalues if it is a closure?
//             eval.eval_function(func, &[module.heap().alloc(event)], &[])?;
//         }
//
//         let irritations = extra.irritations.into_inner().expect("lock poisoned");
//         Ok(irritations)
//     }
// }

// #[derive(Clone, Debug, Dupe, new)]
// pub struct OpenProjectObserver {
//     vex_path: PrettyPath,
//     callback: OwnedFrozenValue,
// }
//
// impl Observer<'_> for OpenProjectObserver {
//     type Event = OpenProjectEvent;
//
//     fn vex_path(&self) -> &PrettyPath {
//         &self.vex_path
//     }
//
//     fn callback(&self) -> &OwnedFrozenValue {
//         &self.callback
//     }
// }
//
// #[derive(Clone, Debug, Dupe, new)]
// pub struct OpenFileObserver {
//     vex_path: PrettyPath,
//     callback: OwnedFrozenValue,
// }
//
// impl Observer<'_> for OpenFileObserver {
//     type Event = OpenFileEvent;
//
//     fn vex_path(&self) -> &PrettyPath {
//         &self.vex_path
//     }
//
//     fn callback(&self) -> &OwnedFrozenValue {
//         &self.callback
//     }
// }
//
// #[derive(Debug, Allocative)]
// pub struct QueryObserver<'v> {
//     pub language: SupportedLanguage,
//
//     #[allocative(skip)]
//     pub query: Query,
//
//     pub on_match: Value<'v>,
// }
//
// #[derive(Debug, Allocative)]
// pub struct FrozenQueryObserver {
//     pub language: SupportedLanguage,
//
//     #[allocative(skip)]
//     pub query: Query,
//
//     pub on_match: OwnedFrozenValue,
// }
//
// impl<'v> Observer<'v> for FrozenQueryObserver {
//     type Event = QueryMatchEvent<'v>;
//
//     fn vex_path(&self) -> &PrettyPath {
//         &self.vex_path
//     }
//
//     fn callback(&self) -> &OwnedFrozenValue {
//         &self.on_match
//     }
// }
//
// #[derive(Debug)]
// pub struct MatchObserver {
//     vex_path: PrettyPath,
//     query: Query,
//     callback: OwnedFrozenValue,
// }
//
// #[derive(Clone, Debug, Dupe, new)]
// pub struct CloseFileObserver {
//     vex_path: PrettyPath,
//     callback: OwnedFrozenValue,
// }
//
// impl Observer<'_> for CloseFileObserver {
//     type Event = CloseFileEvent;
//
//     fn vex_path(&self) -> &PrettyPath {
//         &self.vex_path
//     }
//
//     fn callback(&self) -> &OwnedFrozenValue {
//         &self.callback
//     }
// }
//
// #[derive(Clone, Debug, Dupe, new)]
// pub struct CloseProjectObserver {
//     vex_path: PrettyPath,
//     callback: OwnedFrozenValue,
// }
//
// impl Observer<'_> for CloseProjectObserver {
//     type Event = CloseProjectEvent;
//
//     fn vex_path(&self) -> &PrettyPath {
//         &self.vex_path
//     }
//
//     fn callback(&self) -> &OwnedFrozenValue {
//         &self.callback
//     }
// }
