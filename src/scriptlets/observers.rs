use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    eval::Evaluator,
    values::{Freeze, Freezer, FrozenValue, StarlarkValue, Value},
};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType, Trace};

use crate::{
    ignore_markers::IgnoreMarkers,
    result::Result,
    scriptlets::{
        action::Action, event::EventKind, extra_data::TempData, handler_module::HandlerModule,
        print_handler::PrintHandler, query_cache::QueryCache,
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

    pub fn observers_for(&self, event_kind: EventKind) -> &[Observer] {
        match event_kind {
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

pub trait Observable {
    fn observe<'v>(
        &self,
        handler_module: &'v HandlerModule,
        event: Value<'v>,
        opts: ObserveOptions<'_>,
    ) -> Result<()>;
}

#[derive(Clone, Debug, Dupe)]
pub struct ObserveOptions<'v> {
    pub action: Action,
    pub query_cache: &'v QueryCache,
    pub ignore_markers: Option<&'v IgnoreMarkers>,
}

impl Observable for Observer {
    fn observe<'v>(
        &self,
        handler_module: &'v HandlerModule,
        event: Value<'v>,
        opts: ObserveOptions<'_>,
    ) -> Result<()> {
        let temp_data = TempData {
            action: opts.action,
            query_cache: opts.query_cache,
            vex_path: self.vex_path.dupe(),
            ignore_markers: opts.ignore_markers,
        };

        let mut eval = Evaluator::new(handler_module);
        eval.extra = Some(&temp_data);
        eval.set_print_handler(&PrintHandler);

        let func = self.callback.dupe().to_value(); // TODO(kcza): check thread safety! Can this unfrozen
                                                    // function mutate upvalues if it is a closure?
        eval.eval_function(func, &[event], &[])?;

        Ok(())
    }
}

impl Observable for &[Observer] {
    fn observe<'v>(
        &self,
        handler_module: &'v HandlerModule,
        event: Value<'v>,
        opts: ObserveOptions<'_>,
    ) -> Result<()> {
        self.iter()
            .try_for_each(|observer| observer.observe(handler_module, event.dupe(), opts.dupe()))
    }
}
