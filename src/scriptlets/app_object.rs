use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::Value,
    values::{none::NoneType, NoSerialize, ProvidesStaticType, StarlarkValue},
};
use starlark_derive::starlark_value;

use crate::{
    error::Error,
    scriptlets::extra_data::{EvaluatorData, HandlerDataBuilder},
};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct AppObject;

impl AppObject {
    pub const NAME: &'static str = "vex";

    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn language<'v>(
            #[starlark(this)] _this: Value<'v>,
            lang: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Language)?;

            HandlerDataBuilder::get_from(eval.module()).set_language(lang.into());

            Ok(NoneType)
        }

        fn query<'v>(
            #[starlark(this)] _this: Value<'v>,
            query: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Query)?;

            // TODO(kcza): attach the id in errors somewhere?
            HandlerDataBuilder::get_from(eval.module()).set_query(query.into());

            Ok(NoneType)
        }

        fn observe<'v>(
            #[starlark(this)] _this: Value<'v>,
            event: &str,
            handler: Value<'v>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Observe)?;

            let event = event.parse()?;
            HandlerDataBuilder::get_from(eval.module()).add_observer(event, handler);

            Ok(NoneType)
        }

        fn warn<'v>(
            #[starlark(this)] _this: Value<'v>,
            _msg: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Warn)?;

            todo!();
            // Ok(NoneType)
        }
    }

    fn check_available(eval: &Evaluator<'_, '_>, attr: AttrName) -> anyhow::Result<()> {
        EvaluatorData::get_from(eval).check_available(Self::TYPE, attr)
    }
}

starlark::starlark_simple_value!(AppObject);
#[starlark_value(type = "vex")]
impl<'v> StarlarkValue<'v> for AppObject {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(AppObject::methods)
    }
}

impl Display for AppObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        Self::NAME.fmt(f)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Allocative)]
pub enum AttrName {
    Language,
    Observe,
    Query,
    Warn,
}

impl AttrName {
    #[allow(unused)]
    fn name(&self) -> &str {
        match self {
            AttrName::Language => "language",
            AttrName::Observe => "observe",
            AttrName::Query => "query",
            AttrName::Warn => "warn",
        }
    }
}

impl Display for AttrName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}

// TODO(kcza): move EventType to its own event module
#[derive(Debug)]
pub enum EventType {
    Start,
    Match,
    EoF,
    End,
}

impl EventType {
    #[allow(unused)]
    fn name(&self) -> &str {
        match self {
            EventType::Start => "start",
            EventType::Match => "match",
            EventType::EoF => "eof",
            EventType::End => "end",
        }
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "start" => Ok(EventType::Start),
            "match" => Ok(EventType::Match),
            "eof" => Ok(EventType::EoF),
            "end" => Ok(EventType::End),
            _ => Err(Error::UnknownEvent(s.to_owned()).into()),
        }
    }
}
