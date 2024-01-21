use std::fmt::Display;

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
    scriptlets::{
        action::Action,
        event::EventType,
        extra_data::{HandlerDataBuilder, InvocationData},
    },
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
            AppObject::check_attr_available(eval, "vex.language", &[Action::Initing])?;

            HandlerDataBuilder::get_from(eval.module()).set_language(lang.into());

            Ok(NoneType)
        }

        fn query<'v>(
            #[starlark(this)] _this: Value<'v>,
            query: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.query", &[Action::Initing])?;

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
            AppObject::check_attr_available(eval, "vex.observe", &[Action::Initing])?;

            let event = event.parse()?;
            HandlerDataBuilder::get_from(eval.module()).add_observer(event, handler);

            Ok(NoneType)
        }

        fn warn<'v>(
            #[starlark(this)] _this: Value<'v>,
            _msg: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(
                eval,
                "vex.warn",
                &[
                    Action::Vexing(EventType::Start),
                    Action::Vexing(EventType::Match),
                    Action::Vexing(EventType::EoF),
                    Action::Vexing(EventType::End),
                ],
            )?;

            todo!();
            // Ok(NoneType)
        }
    }

    fn check_attr_available(
        eval: &Evaluator<'_, '_>,
        attr_path: &'static str,
        available_actions: &[Action],
    ) -> anyhow::Result<()> {
        let curr_action = InvocationData::get_from(eval).action();
        if !available_actions.contains(&curr_action) {
            return Err(Error::Unavailable {
                what: attr_path,
                action: curr_action,
            }
            .into());
        }
        Ok(())
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
