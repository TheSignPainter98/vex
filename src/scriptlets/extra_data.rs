use std::fmt::Display;

use allocative::Allocative;
use starlark::values::StarlarkValue;
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

use crate::error::Error;

use super::{app_object::AttrName, Stage};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ExtraData {
    stage_name: &'static str,
    available_fields: &'static [AttrName],
    // TODO(kcza): add the frozen handlers here!
}

impl ExtraData {
    pub fn new<S: Stage>() -> Self {
        Self {
            stage_name: S::NAME,
            available_fields: S::APP_OBJECT_ATTRS,
        }
    }

    pub fn check_available(&self, recv_name: &'static str, attr: AttrName) -> anyhow::Result<()> {
        println!("checking {attr:?} in {:?}", self.available_fields);
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

starlark::starlark_simple_value!(ExtraData);
#[starlark_value(type = "extra")]
impl<'v> StarlarkValue<'v> for ExtraData {}

impl Display for ExtraData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", ExtraData::TYPE)
    }
}
