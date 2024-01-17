use std::{cell::RefCell, fmt::Display};

use allocative::Allocative;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{
        AllocFrozenValue, AllocValue, Demand, Freeze, Freezer, FrozenHeap, FrozenValue, Heap,
        ProvidesStaticType, StarlarkValue, Trace, Tracer, Value, ValueLike,
    },
};
use starlark_derive::{starlark_value, NoSerialize};

use crate::{error::Error, scriptlets::app_object::EventType};

use super::{app_object::AttrName, Stage};

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct EvaluatorData {
    stage_name: &'static str,
    available_fields: &'static [AttrName],
}

impl EvaluatorData {
    pub fn new<S: Stage>() -> Self {
        Self {
            stage_name: S::NAME,
            available_fields: S::APP_OBJECT_ATTRS,
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

    pub fn check_available(&self, recv_name: &'static str, attr: AttrName) -> anyhow::Result<()> {
        if !self.available_fields.contains(&attr) {
            return Err(Error::Unavailable {
                recv_name,
                attr,
                stage_name: self.stage_name,
            }
            .into());
        }
        Ok(())
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
    type Frozen = HandlerData;

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

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct HandlerData {
    pub lang: Option<String>,
    pub query: Option<String>,
    pub on_start: Vec<FrozenValue>,
    pub on_match: Vec<FrozenValue>,
    pub on_eof: Vec<FrozenValue>,
    pub on_end: Vec<FrozenValue>,
}

impl HandlerData {
    pub fn get_from(frozen_module: &FrozenModule) -> &Self {
        frozen_module
            .extra_value()
            .as_ref()
            .expect("FrozenModule extra not set")
            .downcast_ref()
            .expect("FrozenModule extra has wrong type")
    }
}

#[starlark_value(type = "HandlerData")]
impl<'v> StarlarkValue<'v> for HandlerData {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl AllocFrozenValue for HandlerData {
    #[inline]
    fn alloc_frozen_value(self, heap: &FrozenHeap) -> FrozenValue {
        heap.alloc_simple(self)
    }
}

impl Display for HandlerData {
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

// #[derive(Debug, Trace, ProvidesStaticType, NoSerialize, Allocative)]
// pub struct HandlerDataGen<V> {
//     // TODO(kcza): custom-implement complex value for frozen and unfrozen variants to allow RefCell
//     // usage on non-frozen only.
//     pub lang: RwLock<Option<String>>,
//     pub query: RwLock<Option<String>>,
//     pub on_start: Vec<V>,
//     pub on_match: Vec<V>,
//     pub on_eof: Vec<V>,
//     pub on_end: Vec<V>,
// }
// starlark_complex_value!(pub HandlerData);

// unsafe impl<'v, V, W> Coerce<&HandlerDataGen<V>> for &HandlerDataGen<W>
// where
//     V: ValueLike<'v> + 'v,
//     W: ValueLike<'v> + 'v,
// {
// }
//
// impl<V> HandlerDataGen<V> {
//     pub fn new() -> Self {
//         Self {
//             lang: RwLock::new(None),
//             query: RwLock::new(None),
//             on_start: Vec::new(),
//             on_match: Vec::new(),
//             on_eof: Vec::new(),
//             on_end: Vec::new(),
//         }
//     }
// }
//
// impl<'v> HandlerDataGen<Value<'v>> {
//     pub fn insert_into(self, module: &'v Module) {
//         module.set_extra_value(module.heap().alloc(self));
//     }
//
//     pub fn get_from(module: &'v Module) -> &'v Self
//     where
//         Self: ProvidesStaticType<'v>,
//         Value<'v>: ValueLike<'v> + 'v,
//     {
//         module
//             .extra_value()
//             .as_ref()
//             .expect("Module extra not set")
//             .request_value::<&Self>()
//             .expect("Module extra has wrong type")
//     }
//
//     pub fn as_mut(&self) -> anyhow::Result<&mut Self> {
//         // Ok(self.0.write().expect("handler data unwritable"))
//         todo!()
//     }
//
//     pub fn set_language(&self, language: String) {
//         *self.lang.write().expect("lang field not writable") = Some(language);
//     }
//
//     pub fn set_query(&self, query: String) {
//         *self.query.write().expect("query field not writable") = Some(query);
//     }
//
//     pub fn add_observer(&mut self, event: EventType, handler: Value<'v>) {
//         let field = match event {
//             EventType::Start => &mut self.on_start,
//             EventType::Match => &mut self.on_match,
//             EventType::EoF => &mut self.on_eof,
//             EventType::End => &mut self.on_end,
//         };
//         field.push(handler);
//     }
// }
//
// #[starlark_value(type = "HandlerData")]
// impl<'v, V: ValueLike<'v> + 'v> StarlarkValue<'v> for HandlerDataGen<V>
// where
//     Self: ProvidesStaticType<'v>,
// {
//     fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
//         demand.provide_value(self)
//     }
// }
//
// impl<'v> Freeze for HandlerData<'v> {
//     type Frozen = FrozenHandlerData;
//
//     fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
//         let HandlerDataGen {
//             lang,
//             query,
//             on_start,
//             on_match,
//             on_eof,
//             on_end,
//         } = self;
//         Ok(HandlerDataGen {
//             lang,
//             query,
//             on_start: on_start.freeze(freezer)?,
//             on_match: on_match.freeze(freezer)?,
//             on_eof: on_eof.freeze(freezer)?,
//             on_end: on_end.freeze(freezer)?,
//         })
//     }
// }
//
// impl<'v, V: ValueLike<'v> + 'v> Display for HandlerDataGen<V>
// where
//     Self: ProvidesStaticType<'v>,
// {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "<{}>", Self::TYPE)
//     }
// }
