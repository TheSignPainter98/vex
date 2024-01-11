use sealed::sealed;

use crate::scriptlets::app_object::AttrName;

#[sealed]
pub trait Stage {
    const NAME: &'static str;
    const APP_OBJECT_ATTRS: &'static [AttrName];
}

pub struct Preiniting;

#[sealed]
impl Stage for Preiniting {
    const NAME: &'static str = "preiniting";
    const APP_OBJECT_ATTRS: &'static [AttrName] = &[];
}

pub struct Initing;

#[sealed]
impl Stage for Initing {
    const NAME: &'static str = "initing";
    const APP_OBJECT_ATTRS: &'static [AttrName] =
        &[AttrName::Language, AttrName::Observe, AttrName::Query];
}

pub struct Vexing;

#[sealed]
impl Stage for Vexing {
    const NAME: &'static str = "vexing";
    const APP_OBJECT_ATTRS: &'static [AttrName] = &[AttrName::Warn];
}
