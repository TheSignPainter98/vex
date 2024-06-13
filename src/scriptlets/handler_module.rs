use std::ops::Deref;

use starlark::{environment::Module, values::FrozenHeap};

use crate::{
    result::Result,
    scriptlets::{
        action::Action,
        event::EventKind,
        extra_data::{DataStore, UnfrozenDataStore},
        query_cache::QueryCache,
        Intents,
    },
};

pub struct HandlerModule {
    module: Module,
}

impl HandlerModule {
    pub fn new(event_kind: EventKind, query_cache: &QueryCache) -> Self {
        let action = Action::Vexing(event_kind);

        let module = Module::new();
        let data = UnfrozenDataStore::new(action, query_cache);
        data.insert_into(&module);

        Self { module }
    }

    pub fn into_intents(self, frozen_heap: &FrozenHeap) -> Result<Intents> {
        let Self { module, .. } = self;
        let module = module.freeze()?;
        frozen_heap.add_reference(module.frozen_heap());

        let data = DataStore::get_from(&module);
        Ok(data.intents().clone())
    }
}

impl Deref for HandlerModule {
    type Target = Module;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}
