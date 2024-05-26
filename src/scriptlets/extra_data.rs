use allocative::Allocative;
use derive_more::Display;
use starlark::{
    environment::{FrozenModule, Module},
    values::{AllocValue, Freeze, ProvidesStaticType, StarlarkValue, ValueLike},
};
use starlark_derive::{starlark_value, NoSerialize, Trace};

use crate::{
    scriptlets::{
        query_cache::QueryCache,
        action::Action,
        intents::{UnfrozenIntent, UnfrozenIntents},
        Intents,
    },
    source_path::PrettyPath,
};

#[derive(Debug, Display, ProvidesStaticType, NoSerialize, Allocative, Trace)]
#[display(fmt = "InvocationData")]
pub struct UnfrozenInvocationData<'v> {
    action: Action,
    vex_path: PrettyPath,
    #[allocative(visit = QueryCache::visit)]
    query_cache: &'v QueryCache,
    intents: UnfrozenIntents<'v>,
}

impl<'v> UnfrozenInvocationData<'v> {
    pub fn new(action: Action, vex_path: PrettyPath, query_cache: &'v QueryCache) -> Self {
        let intents = UnfrozenIntents::new();
        Self {
            action,
            vex_path,
            query_cache,
            intents,
        }
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

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn vex_path(&self) -> &PrettyPath {
        &self.vex_path
    }

    pub fn query_cache(&self) -> &QueryCache {
        &self.query_cache
    }

    pub fn declare_intent(&self, intent: UnfrozenIntent<'v>) {
        self.intents.declare(intent)
    }
}

#[starlark_value(type = "InvocationData")]
impl<'v> StarlarkValue<'v> for UnfrozenInvocationData<'v> {}

impl<'v> AllocValue<'v> for UnfrozenInvocationData<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for UnfrozenInvocationData<'_> {
    type Frozen = InvocationData;

    fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        let Self {
            action,
            vex_path,
            query_cache: _,
            intents,
        } = self;
        let intents = intents.freeze(freezer)?;
        Ok(InvocationData {
            action,
            vex_path,
            intents,
        })
    }
}

#[derive(Debug, NoSerialize, ProvidesStaticType, Allocative, Display)]
#[display(fmt = "InvocationData")]
pub struct InvocationData {
    action: Action,
    vex_path: PrettyPath,
    intents: Intents,
}

impl InvocationData {
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

#[starlark_value(type = "InvocationData")]
impl<'v> StarlarkValue<'v> for InvocationData {}

impl<'v> AllocValue<'v> for InvocationData {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_simple(self)
    }
}
