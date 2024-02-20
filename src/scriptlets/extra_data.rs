use std::{cell::RefCell, fmt::Display, sync::Arc};

use allocative::Allocative;
use dupe::Dupe;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{
        AllocFrozenValue, AllocValue, Demand, Freeze, Freezer, FrozenHeap, FrozenValue, Heap,
        OwnedFrozenValue, ProvidesStaticType, StarlarkValue, Trace, Tracer, Value, ValueLike,
    },
};
use starlark_derive::{starlark_value, NoSerialize};
use tree_sitter::Query;

use crate::{
    error::Error,
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
    supported_language::SupportedLanguage,
};

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct InvocationData {
    action: Action,
}

impl InvocationData {
    pub fn new(action: Action) -> Self {
        Self { action }
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
}

starlark::starlark_simple_value!(InvocationData);
#[starlark_value(type = "EvaluatorData")]
impl<'v> StarlarkValue<'v> for InvocationData {}

impl Display for InvocationData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", InvocationData::TYPE)
    }
}

#[derive(Debug, Trace, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ObserverDataBuilder<'v> {
    pub path: PrettyPath,
    #[allocative(skip)]
    pub lang: RefCell<Option<RawSupportedLanguage<'v>>>,
    #[allocative(skip)]
    pub query: RefCell<Option<RawQuery<'v>>>,
    pub on_open_project: RefCell<Vec<Value<'v>>>,
    pub on_open_file: RefCell<Vec<Value<'v>>>,
    pub on_match: RefCell<Vec<Value<'v>>>,
    pub on_close_file: RefCell<Vec<Value<'v>>>,
    pub on_close_project: RefCell<Vec<Value<'v>>>,
}

impl<'v> ObserverDataBuilder<'v> {
    pub fn new(path: PrettyPath) -> Self {
        Self {
            path,
            lang: RefCell::new(None),
            query: RefCell::new(None),
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

    pub fn set_language(&self, language: RawSupportedLanguage<'v>) {
        *self.lang.borrow_mut() = Some(language);
    }

    pub fn set_query(&self, query: RawQuery<'v>) {
        *self.query.borrow_mut() = Some(query);
    }

    pub fn add_observer(&self, event: EventType, handler: Value<'v>) {
        match event {
            EventType::OpenProject => self.on_open_project.borrow_mut().push(handler),
            EventType::OpenFile => self.on_open_file.borrow_mut().push(handler),
            EventType::Match => self.on_match.borrow_mut().push(handler),
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
            path,
            lang,
            query,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        } = self;
        let lang = lang.into_inner().map(|e| e.to_string());
        let query = query.into_inner().map(|e| e.to_string());
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
            path,
            lang,
            query,
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

pub type RawSupportedLanguage<'v> = RawStr<'v>;
pub type RawQuery<'v> = RawStr<'v>;

/// Wrapper type to allow implementation of certain traits on &str.
#[derive(Debug)]
pub struct RawStr<'v>(&'v str);

unsafe impl<'v> Trace<'v> for RawStr<'v> {
    fn trace(&mut self, _tracer: &Tracer<'v>) {}
}

impl<'v> From<&'v str> for RawStr<'v> {
    fn from(value: &'v str) -> Self {
        Self(value)
    }
}

impl<'v> Display for RawStr<'v> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct FrozenObserverDataBuilder {
    pub path: PrettyPath,
    pub lang: Option<String>,
    pub query: Option<String>,
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
            path,
            lang,
            query,
            on_open_project,
            on_open_file,
            on_match,
            on_close_file,
            on_close_project,
        } = self;

        let path = path.dupe();
        let lang = {
            let Some(lang) = lang else {
                return Err(Error::NoLanguage(path));
            };
            lang.parse::<SupportedLanguage>()?
        };
        let query = {
            let Some(query) = query else {
                return Err(Error::NoQuery(path));
            };
            if query.is_empty() {
                return Err(Error::EmptyQuery(path));
            }
            Arc::new(Query::new(lang.ts_language(), query)?)
        };

        if on_open_project.is_empty()
            && on_open_file.is_empty()
            && on_match.is_empty()
            && on_close_file.is_empty()
            && on_close_project.is_empty()
        {
            return Err(Error::NoCallbacks(path));
        }
        if on_match.is_empty() {
            return Err(Error::NoMatch(path));
        }
        let on_open_project = Arc::new(
            on_open_project
                .iter()
                .map(Dupe::dupe)
                .map(OwnedFrozenValue::alloc)
                .map(OpenProjectObserver::new)
                .collect(),
        );
        let on_open_file = Arc::new(
            on_open_file
                .iter()
                .map(Dupe::dupe)
                .map(OwnedFrozenValue::alloc)
                .map(OpenFileObserver::new)
                .collect(),
        );
        let on_match = Arc::new(
            on_match
                .iter()
                .map(Dupe::dupe)
                .map(OwnedFrozenValue::alloc)
                .map(MatchObserver::new)
                .collect(),
        );
        let on_close_file = Arc::new(
            on_close_file
                .iter()
                .map(Dupe::dupe)
                .map(OwnedFrozenValue::alloc)
                .map(CloseFileObserver::new)
                .collect(),
        );
        let on_close_project = Arc::new(
            on_close_project
                .iter()
                .map(Dupe::dupe)
                .map(OwnedFrozenValue::alloc)
                .map(CloseProjectObserver::new)
                .collect(),
        );
        Ok(ScriptletObserverData {
            path,
            lang,
            query,
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
