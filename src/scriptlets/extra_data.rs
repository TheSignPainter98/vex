use std::{
    cell::RefCell,
    fmt::Display,
    sync::{Arc, Mutex},
};

use allocative::Allocative;
use dupe::Dupe;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{
        AllocFrozenValue, AllocValue, Demand, Freeze, Freezer, FrozenHeap, FrozenValue, Heap,
        OwnedFrozenValue, ProvidesStaticType, StarlarkValue, Trace, Value, ValueLike,
    },
};
use starlark_derive::{starlark_value, NoSerialize};

use crate::{
    error::Error,
    irritation::Irritation,
    result::Result,
    scriptlets::{
        action::Action,
        event::EventType,
        observers::{
            CloseFileObserver, CloseProjectObserver, MatchObserver, OpenFileObserver,
            OpenProjectObserver,
        },
        ScriptletObserverData,
    },
    source_path::PrettyPath,
    trigger::Trigger,
};

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct InvocationData {
    action: Action,
    path: PrettyPath,

    #[allocative(skip)]
    pub irritations: Mutex<Vec<Irritation>>,
}

impl InvocationData {
    pub fn new(action: Action, path: PrettyPath) -> Self {
        Self {
            action,
            path,
            irritations: Mutex::new(vec![]),
        }
    }

    pub fn insert_into<'e>(&'e self, eval: &mut Evaluator<'_, 'e>) {
        eval.extra = Some(self);
    }

    pub fn get_from<'e>(eval: &Evaluator<'_, 'e>) -> &'e Self {
        eval.extra
            .as_ref()
            .expect("Evaluator extra not set")
            .downcast_ref()
            .expect("Evaluator extra has wrong type")
    }

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn path(&self) -> &PrettyPath {
        &self.path
    }

    pub fn add_irritation(&self, irritation: Irritation) {
        self.irritations
            .lock()
            .as_mut()
            .expect("invocation data used concurrently")
            .push(irritation)
    }
}

starlark::starlark_simple_value!(InvocationData);
#[starlark_value(type = "InvocationData")]
impl<'v> StarlarkValue<'v> for InvocationData {}

impl Display for InvocationData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", InvocationData::TYPE)
    }
}

#[derive(Debug, Trace, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ObserverDataBuilder<'v> {
    pub project_root: PrettyPath,
    pub vex_path: PrettyPath,
    pub triggers: RefCell<Vec<Arc<Trigger>>>,
    pub on_open_project: RefCell<Vec<Value<'v>>>,
    pub on_open_file: RefCell<Vec<Value<'v>>>,
    pub on_match: RefCell<Vec<Value<'v>>>,
    pub on_close_file: RefCell<Vec<Value<'v>>>,
    pub on_close_project: RefCell<Vec<Value<'v>>>,
}

impl<'v> ObserverDataBuilder<'v> {
    pub fn new(project_root: PrettyPath, vex_path: PrettyPath) -> Self {
        Self {
            project_root,
            vex_path,
            triggers: RefCell::new(Vec::with_capacity(1)),
            on_open_project: RefCell::new(vec![]),
            on_open_file: RefCell::new(vec![]),
            on_match: RefCell::new(vec![]),
            on_close_file: RefCell::new(vec![]),
            on_close_project: RefCell::new(vec![]),
        }
    }

    pub fn insert_into(self, module: &'v Module) {
        module.set_extra_value(module.heap().alloc(self))
    }

    pub fn get_from(module: &'v Module) -> &'v Self {
        module
            .extra_value()
            .as_ref()
            .expect("Module extra not set")
            .request_value()
            .expect("Module extra has wrong type")
    }

    pub fn add_trigger(&self, trigger: Trigger) -> Result<()> {
        self.triggers.borrow_mut().push(Arc::new(trigger));
        Ok(())
    }

    pub fn add_observer(&self, event: EventType, handler: Value<'v>) {
        match event {
            EventType::OpenProject => self.on_open_project.borrow_mut().push(handler),
            EventType::OpenFile => self.on_open_file.borrow_mut().push(handler),
            EventType::QueryMatch => self.on_match.borrow_mut().push(handler),
            EventType::CloseFile => self.on_close_file.borrow_mut().push(handler),
            EventType::CloseProject => self.on_close_project.borrow_mut().push(handler),
        }
    }
}

#[starlark_value(type = "HandlerDataBuilder")]
impl<'v> StarlarkValue<'v> for ObserverDataBuilder<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl<'v> Freeze for ObserverDataBuilder<'v> {
    type Frozen = FrozenObserverDataBuilder;

    fn freeze(self, freezer: &Freezer) -> anyhow::Result<Self::Frozen> {
        let ObserverDataBuilder {
            project_root: _project_root,
            vex_path,
            triggers,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        } = self;
        let triggers = triggers.into_inner();
        let on_open_project = on_open_project
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_open_file = on_open_file
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_match = on_match
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_close_file = on_close_file
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_close_project = on_close_project
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        Ok(FrozenObserverDataBuilder {
            vex_path,
            triggers,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        })
    }
}

impl<'v> AllocValue<'v> for ObserverDataBuilder<'v> {
    #[inline]
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl<'v> Display for ObserverDataBuilder<'v> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::TYPE.fmt(f)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct FrozenObserverDataBuilder {
    pub vex_path: PrettyPath,
    pub triggers: Vec<Arc<Trigger>>,
    pub on_open_project: Vec<FrozenValue>,
    pub on_open_file: Vec<FrozenValue>,
    pub on_match: Vec<FrozenValue>,
    pub on_close_file: Vec<FrozenValue>,
    pub on_close_project: Vec<FrozenValue>,
}

impl FrozenObserverDataBuilder {
    pub fn get_from(frozen_module: &FrozenModule) -> &Self {
        frozen_module
            .extra_value()
            .as_ref()
            .expect("FrozenModule extra not set")
            .downcast_ref()
            .expect("FrozenModule extra has wrong type")
    }

    pub fn build(&self) -> Result<ScriptletObserverData> {
        let Self {
            vex_path,
            triggers,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        } = self;

        let vex_path = vex_path.dupe();
        if triggers.is_empty() {
            return Err(Error::NoTriggers(vex_path));
        }
        let has_queries = triggers.iter().any(|t| {
            t.content_trigger
                .as_ref()
                .is_some_and(|ct| ct.query.is_some())
        });
        if on_match.is_empty() && has_queries {
            return Err(Error::NoQueryMatch(vex_path));
        } else if !on_match.is_empty() && !has_queries {
            return Err(Error::NoQuery(vex_path));
        }
        let triggers = triggers.to_vec();

        if on_open_project.is_empty()
            && on_open_file.is_empty()
            && on_match.is_empty()
            && on_close_file.is_empty()
            && on_close_project.is_empty()
        {
            return Err(Error::NoCallbacks(vex_path));
        }
        let on_open_project = on_open_project
            .iter()
            .map(Dupe::dupe)
            .map(OwnedFrozenValue::alloc)
            .map(OpenProjectObserver::new)
            .collect();
        let on_open_file = on_open_file
            .iter()
            .map(Dupe::dupe)
            .map(OwnedFrozenValue::alloc)
            .map(OpenFileObserver::new)
            .collect();
        let on_match = on_match
            .iter()
            .map(Dupe::dupe)
            .map(OwnedFrozenValue::alloc)
            .map(MatchObserver::new)
            .collect();
        let on_close_file = on_close_file
            .iter()
            .map(Dupe::dupe)
            .map(OwnedFrozenValue::alloc)
            .map(CloseFileObserver::new)
            .collect();
        let on_close_project = on_close_project
            .iter()
            .map(Dupe::dupe)
            .map(OwnedFrozenValue::alloc)
            .map(CloseProjectObserver::new)
            .collect();

        Ok(ScriptletObserverData {
            vex_path,
            triggers,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        })
    }
}

#[starlark_value(type = "HandlerData")]
impl<'v> StarlarkValue<'v> for FrozenObserverDataBuilder {
    type Canonical = ObserverDataBuilder<'v>;

    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl AllocFrozenValue for FrozenObserverDataBuilder {
    #[inline]
    fn alloc_frozen_value(self, heap: &FrozenHeap) -> FrozenValue {
        heap.alloc_simple(self)
    }
}

impl Display for FrozenObserverDataBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::TYPE.fmt(f)
    }
}
