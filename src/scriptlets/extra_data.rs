use allocative::Allocative;
use derive_more::Display;
use starlark::{
    environment::{FrozenModule, Module},
    eval::Evaluator,
    values::{AllocValue, Freeze, ProvidesStaticType, StarlarkValue, ValueLike},
};
use starlark_derive::{starlark_value, NoSerialize, Trace};

use crate::{
    scriptlets::{
        action::Action,
        intents::{UnfrozenIntent, UnfrozenIntents},
        query_cache::QueryCache,
        Intents,
    },
    source_path::PrettyPath,
};

#[derive(Debug, Display, ProvidesStaticType, NoSerialize, Allocative, Trace)]
#[display(fmt = "UnfrozenDataStore")]
pub struct UnfrozenDataStore<'v> {
    action: Action,
    #[allocative(visit = QueryCache::visit)]
    query_cache: &'v QueryCache,
    intents: UnfrozenIntents<'v>,
}

impl<'v> UnfrozenDataStore<'v> {
    pub fn new(action: Action, query_cache: &'v QueryCache) -> Self {
        let intents = UnfrozenIntents::new();
        Self {
            action,
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

    pub fn query_cache(&self) -> &QueryCache {
        self.query_cache
    }

    pub fn declare_intent(&self, intent: UnfrozenIntent<'v>) {
        self.intents.declare(intent)
    }
}

#[starlark_value(type = "DataStore")]
impl<'v> StarlarkValue<'v> for UnfrozenDataStore<'v> {}

impl<'v> AllocValue<'v> for UnfrozenDataStore<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for UnfrozenDataStore<'_> {
    type Frozen = DataStore;

    fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        let Self {
            action: _,
            query_cache: _,
            intents,
        } = self;
        let intents = intents.freeze(freezer)?;
        Ok(DataStore { intents })
    }
}

#[derive(Debug, NoSerialize, ProvidesStaticType, Allocative, Display)]
#[display(fmt = "DataStore")]
pub struct DataStore {
    intents: Intents,
}

impl DataStore {
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

#[starlark_value(type = "DataStore")]
impl<'v> StarlarkValue<'v> for DataStore {}

impl<'v> AllocValue<'v> for DataStore {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_simple(self)
    }
}

#[derive(Debug, ProvidesStaticType)]
pub struct InvocationData {
    pub vex_path: PrettyPath,
}

impl InvocationData {
    pub fn get_from<'a>(eval: &Evaluator<'_, 'a>) -> &'a Self {
        eval.extra
            .expect("internal error: Evaluator extra not set")
            .downcast_ref()
            .expect("internal erro: Evaluator extra has wrong type")
    }
}
