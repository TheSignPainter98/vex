use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::Value,
    values::{none::NoneType, NoSerialize, ProvidesStaticType, StarlarkValue, ValueLike},
};

use crate::{error::Error, scriptlets::Stage};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct AppObject {
    stage_name: &'static str,
    available_fields: &'static [AttrName],
}

impl AppObject {
    pub const NAME: &'static str = "vex";

    pub fn new<S: Stage>() -> Self {
        Self {
            stage_name: S::NAME,
            available_fields: S::APP_OBJECT_ATTRS,
        }
    }

    fn check_available(&self, attr: AttrName) -> anyhow::Result<()> {
        if !self.available_fields.contains(&attr) {
            return Err(Error::Unavailable {
                attr,
                stage_name: self.stage_name,
            }
            .into());
        }
        Ok(())
    }

    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn language<'v>(
            this: Value<'v>,
            lang: &'v str,
            _eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            let this = this
                .downcast_ref::<AppObject>()
                .expect("expected app object receiver");
            this.check_available(AttrName::Language)?;
            // TODO(kcza): store available methods in the evaluator
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
            _eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            // let query = Query::new(lang.ts_language(), query)?;
            // TODO(kcza): don't rely on language being set already here!
            // TODO(kcza): attach the id in errors somewhere?
            println!("vex.seek({query:?}) called");
            Ok(NoneType)
        }

        fn observe<'v>(
            #[starlark(this)] _this: Value<'v>,
            event: &str,
            handler: Value<'v>,
            _eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            let event = event.parse::<EventType>()?;
            println!("vex.observe({event:?}, {handler}) called");
            Ok(NoneType)
        }
        //
        // fn warn<'v>(
        //     #[starlark(this)] _this: Value<'v>,
        //     msg: &'v str,
        //     eval: &mut Evaluator<'v, 'v>,
        // ) -> anyhow::Result<NoneType> {
        //     println!("vex.warn({msg}) called");
        //     todo!();
        //     // Ok(NoneType)
        // }
    }
}

starlark::starlark_simple_value!(AppObject);
impl<'v> StarlarkValue<'v> for AppObject {
    starlark::starlark_type!(AppObject::NAME);

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
            _ => Err(Error::UnknownEvent(s.to_owned()).into()),
        }
    }
}
