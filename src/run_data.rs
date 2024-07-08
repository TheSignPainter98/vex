use std::fmt::Display;

use allocative::Allocative;
use starlark::values::{list::AllocList, AllocValue, Heap, StarlarkValue, Value};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

use crate::irritation::Irritation;

#[derive(Debug, Default, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct RunData {
    pub irritations: Vec<Irritation>,
    pub num_files_scanned: usize,
}

impl RunData {
    const IRRITATIONS_ATTR_NAME: &'static str = "irritations";
    const NUM_FILES_SCANNED: &'static str = "num_files_scanned";
}

#[cfg(test)]
impl RunData {
    pub fn into_irritations(self) -> Vec<Irritation> {
        self.irritations
    }
}

impl Display for RunData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::TYPE.fmt(f)
    }
}

#[starlark_value(type = "RunData")]
impl<'v> StarlarkValue<'v> for RunData {
    fn dir_attr(&self) -> Vec<String> {
        [Self::IRRITATIONS_ATTR_NAME, Self::NUM_FILES_SCANNED]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::IRRITATIONS_ATTR_NAME => Some(heap.alloc(AllocList(self.irritations))),
            Self::NUM_FILES_SCANNED => Some(heap.alloc(self.num_files_scanned)),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [Self::IRRITATIONS_ATTR_NAME, Self::NUM_FILES_SCANNED].contains(&attr)
    }
}

impl<'v> AllocValue<'v> for RunData {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_simple(self)
    }
}
