use std::ops::Deref;

use starlark::{environment::Module, values::FrozenHeap};

use crate::{
    result::Result,
    scriptlets::{
        extra_data::{RetainedData, UnfrozenRetainedData},
        Intents,
    },
};

pub struct HandlerModule {
    module: Module,
}

impl HandlerModule {
    pub fn new() -> Self {
        let module = Module::new();
        let ret_data = UnfrozenRetainedData::new();
        ret_data.insert_into(&module);

        Self { module }
    }

    pub fn intent_count(&self) -> usize {
        let ret_data = UnfrozenRetainedData::get_from(&self.module);
        ret_data.intent_count()
    }

    pub fn into_intents_on(self, frozen_heap: &FrozenHeap) -> Result<Intents> {
        let Self { module, .. } = self;
        let module = module.freeze()?;
        frozen_heap.add_reference(module.frozen_heap());

        let ret_data = RetainedData::get_from(&module);
        Ok(ret_data.intents().clone())
    }

    pub fn into_module(self) -> Module {
        self.module
    }
}

impl Deref for HandlerModule {
    type Target = Module;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}
