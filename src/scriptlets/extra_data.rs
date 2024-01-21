use std::{cell::RefCell, fmt::Display};

use allocative::Allocative;
use camino::Utf8Path;
use dupe::Dupe;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{
        AllocFrozenValue, AllocValue, Demand, Freeze, Freezer, FrozenHeap, FrozenValue, Heap,
        ProvidesStaticType, StarlarkValue, Trace, Tracer, Value, ValueLike,
    },
};
use starlark_derive::{starlark_value, NoSerialize};
use tree_sitter::Query;

use crate::{
    error::Error,
    scriptlets::{action::Action, event::EventType},
    supported_language::SupportedLanguage,
};

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct EvaluatorData {
    action: Action,
}

impl EvaluatorData {
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

starlark::starlark_simple_value!(EvaluatorData);
#[starlark_value(type = "EvaluatorData")]
impl<'v> StarlarkValue<'v> for EvaluatorData {}

impl Display for EvaluatorData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", EvaluatorData::TYPE)
    }
}

#[derive(Debug, Trace, Default, ProvidesStaticType, NoSerialize, Allocative)]
pub struct HandlerDataBuilder<'v> {
    #[allocative(skip)]
    pub lang: RefCell<Option<RawSupportedLanguage<'v>>>,
    #[allocative(skip)]
    pub query: RefCell<Option<RawQuery<'v>>>,
    pub on_start: RefCell<Vec<Value<'v>>>,
    pub on_match: RefCell<Vec<Value<'v>>>,
    pub on_eof: RefCell<Vec<Value<'v>>>,
    pub on_end: RefCell<Vec<Value<'v>>>,
}

impl<'v> HandlerDataBuilder<'v> {
    pub fn new() -> Self {
        Self::default()
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
            EventType::Start => self.on_start.borrow_mut().push(handler),
            EventType::Match => self.on_match.borrow_mut().push(handler),
            EventType::EoF => self.on_eof.borrow_mut().push(handler),
            EventType::End => self.on_end.borrow_mut().push(handler),
        }
    }
}

#[starlark_value(type = "HandlerDataBuilder")]
impl<'v> StarlarkValue<'v> for HandlerDataBuilder<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl<'v> Freeze for HandlerDataBuilder<'v> {
    type Frozen = FrozenHandlerDataBuilder;

    fn freeze(self, freezer: &Freezer) -> anyhow::Result<Self::Frozen> {
        let HandlerDataBuilder {
            lang,
            query,
            on_start,
            on_match,
            on_eof,
            on_end,
        } = self;
        let lang = lang.into_inner().map(|e| e.to_string());
        let query = query.into_inner().map(|e| e.to_string());
        let on_start = on_start
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_match = on_match
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_eof = on_eof
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        let on_end = on_end
            .into_inner()
            .into_iter()
            .map(|v| v.freeze(freezer))
            .collect::<anyhow::Result<_>>()?;
        Ok(FrozenHandlerDataBuilder {
            lang,
            query,
            on_start,
            on_match,
            on_eof,
            on_end,
        })
    }
}

impl<'v> AllocValue<'v> for HandlerDataBuilder<'v> {
    #[inline]
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl<'v> Display for HandlerDataBuilder<'v> {
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
pub struct FrozenHandlerDataBuilder {
    pub lang: Option<String>,
    pub query: Option<String>,
    pub on_start: Vec<FrozenValue>,
    pub on_match: Vec<FrozenValue>,
    pub on_eof: Vec<FrozenValue>,
    pub on_end: Vec<FrozenValue>,
}

impl FrozenHandlerDataBuilder {
    pub fn get_from(frozen_module: &FrozenModule) -> &Self {
        frozen_module
            .extra_value()
            .as_ref()
            .expect("FrozenModule extra not set")
            .downcast_ref()
            .expect("FrozenModule extra has wrong type")
    }

    pub fn build(&self, path: &Utf8Path) -> anyhow::Result<HandlerData> {
        let Self {
            lang,
            query,
            on_start,
            on_match,
            on_eof,
            on_end,
        } = self;

        if on_start.len() + on_match.len() + on_end.len() + on_end.len() == 0 {
            return Err(Error::NoCallbacks(path.to_owned()).into());
        }

        let lang = {
            let Some(lang) = lang else {
                return Err(Error::NoLanguage(path.to_owned()).into());
            };
            lang.parse::<SupportedLanguage>()?
        };
        let query = {
            let Some(query) = query else {
                return Err(Error::NoQuery(path.to_owned()).into());
            };
            Query::new(lang.ts_language(), &query)?
        };
        let on_start = on_start.iter().map(Dupe::dupe).collect();
        let on_match = on_match.iter().map(Dupe::dupe).collect();
        let on_eof = on_eof.iter().map(Dupe::dupe).collect();
        let on_end = on_end.iter().map(Dupe::dupe).collect();
        Ok(HandlerData {
            lang,
            query,
            on_start,
            on_match,
            on_eof,
            on_end,
        })
    }
}

#[starlark_value(type = "HandlerData")]
impl<'v> StarlarkValue<'v> for FrozenHandlerDataBuilder {
    type Canonical = HandlerDataBuilder<'v>;

    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl AllocFrozenValue for FrozenHandlerDataBuilder {
    #[inline]
    fn alloc_frozen_value(self, heap: &FrozenHeap) -> FrozenValue {
        heap.alloc_simple(self)
    }
}

impl Display for FrozenHandlerDataBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::TYPE.fmt(f)
    }
}

#[derive(Debug)]
pub struct HandlerData {
    pub lang: SupportedLanguage,
    pub query: Query,
    pub on_start: Vec<FrozenValue>,
    pub on_match: Vec<FrozenValue>,
    pub on_eof: Vec<FrozenValue>,
    pub on_end: Vec<FrozenValue>,
}
