use allocative::Allocative;
use derive_more::Display;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{AllocValue, Freeze, ProvidesStaticType, StarlarkValue, ValueLike},
};
use starlark_derive::{starlark_value, NoSerialize, Trace};

use crate::{
    ignore_markers::IgnoreMarkers,
    scriptlets::{
        action::Action,
        intents::{UnfrozenIntent, UnfrozenIntents},
        query_cache::QueryCache,
        Intents,
    },
    warning_filter::WarningFilter,
};

#[derive(Debug, Display, ProvidesStaticType, NoSerialize, Allocative, Trace)]
#[display(fmt = "RetainedData")]
pub struct UnfrozenRetainedData<'v> {
    intents: UnfrozenIntents<'v>,
}

impl<'v> UnfrozenRetainedData<'v> {
    pub fn new() -> Self {
        let intents = UnfrozenIntents::new();
        Self { intents }
    }

    pub fn insert_into(self, module: &'v Module) {
        module.set_extra_value(module.heap().alloc(self))
    }

    pub fn get_from(module: &'v Module) -> &Self {
        module
            .extra_value()
            .expect("Module extra not set")
            .downcast_ref()
            .expect("Module extra has wrong type")
    }

    pub fn intent_count(&self) -> usize {
        self.intents.len()
    }

    pub fn declare_intent(&self, intent: UnfrozenIntent<'v>) {
        self.intents.declare(intent)
    }
}

#[starlark_value(type = "RetainedData")]
impl<'v> StarlarkValue<'v> for UnfrozenRetainedData<'v> {}

impl<'v> AllocValue<'v> for UnfrozenRetainedData<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for UnfrozenRetainedData<'_> {
    type Frozen = RetainedData;

    fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        let Self { intents } = self;
        let intents = intents.freeze(freezer)?;
        Ok(RetainedData { intents })
    }
}

#[derive(Debug, NoSerialize, ProvidesStaticType, Allocative, Display)]
#[display(fmt = "DataStore")]
pub struct RetainedData {
    intents: Intents,
}

impl RetainedData {
    pub fn get_from(module: &FrozenModule) -> &Self {
        module
            .extra_value()
            .expect("FrozenModule extra not set")
            .downcast_ref()
            .expect("FrozenModule extra has wrong type")
    }

    pub fn intents(&self) -> &Intents {
        &self.intents
    }
}

#[starlark_value(type = "RetainedData")]
impl<'v> StarlarkValue<'v> for RetainedData {}

impl<'v> AllocValue<'v> for RetainedData {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_simple(self)
    }
}

#[derive(Debug, ProvidesStaticType)]
pub struct TempData<'v> {
    pub action: Action,
    pub query_cache: Option<&'v QueryCache>,
    pub ignore_markers: Option<&'v IgnoreMarkers>,
    pub active_lints: Option<&'v WarningFilter>,
}

impl<'v> TempData<'v> {
    pub fn get_from(eval: &Evaluator<'_, 'v>) -> &'v Self {
        eval.extra
            .expect("internal error: Evaluator extra not set")
            .downcast_ref()
            .expect("internal erro: Evaluator extra has wrong type")
    }
}
