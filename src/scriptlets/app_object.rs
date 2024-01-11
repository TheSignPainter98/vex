use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::Value,
    values::{none::NoneType, NoSerialize, ProvidesStaticType, StarlarkValue, ValueLike},
};
use starlark_derive::starlark_value;

use crate::{error::Error, scriptlets::Stage};

use super::extra_data::ExtraData;

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
            println!("vex.language({lang})");

            // let lang: SupportedLanguage = lang.parse()?;
            // let store = eval
            //     .extra
            //     .expect("extra not set")
            //     .downcast_ref::<HandlerStore>()
            //     .expect("extra has wrong type");
            // store.0.borrow_mut().language = Some(lang);
            Ok(NoneType)
        }

        fn query<'v>(
            #[starlark(this)] _this: Value<'v>,
            query: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Query)?;
            // TODO(kcza): don't rely on language being set already here!
            // TODO(kcza): attach the id in errors somewhere?
            println!("vex.seek({query:?}) called");
            Ok(NoneType)
        }

        fn observe<'v>(
            #[starlark(this)] _this: Value<'v>,
            event: &str,
            handler: Value<'v>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Observe)?;
            let event = event.parse::<EventType>()?;
            println!("vex.observe({event:?}, ...) called");
            Ok(NoneType)
        }

        fn warn<'v>(
            #[starlark(this)] _this: Value<'v>,
            msg: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_available(eval, AttrName::Warn)?;
            println!("vex.warn({msg}) called");
            todo!();
            // Ok(NoneType)
        }
    }

    fn check_available(eval: &Evaluator<'_, '_>, attr: AttrName) -> anyhow::Result<()> {
        let extra = eval
            .extra
            .as_ref()
            .expect("extra unset")
            .downcast_ref::<ExtraData>()
            .expect("extra has wrong type");
        extra.check_available(AppObject::TYPE, attr)
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

#[derive(Debug)]
pub enum EventType {
    Start,
    Match,
    EoF,
    End,
}

impl EventType {
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
