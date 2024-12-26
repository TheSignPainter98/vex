use std::{collections::HashMap, ops::Deref};

use starlark::values::{dict::AllocDict, list::AllocList, FrozenHeap, FrozenValue};

use crate::{
    context::{ScriptArgKey, ScriptArgValue, ScriptArgs, ScriptArgsForId},
    id::Id,
};

#[derive(Debug, Default)]
pub struct ScriptArgsValueMap(HashMap<Id, FrozenValue>);

impl ScriptArgsValueMap {
    pub fn with_args(script_args: &ScriptArgs, heap: &FrozenHeap) -> Self {
        let map = script_args
            .iter()
            .map(|(k, v)| (k.clone(), v.to_frozen_value(heap)))
            .collect();
        Self(map)
    }
}

impl Deref for ScriptArgsValueMap {
    type Target = HashMap<Id, FrozenValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
impl ScriptArgsValueMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

trait ToFrozenValue {
    fn to_frozen_value(&self, heap: &FrozenHeap) -> FrozenValue;
}

impl ToFrozenValue for ScriptArgsForId {
    fn to_frozen_value(&self, heap: &FrozenHeap) -> FrozenValue {
        heap.alloc(AllocDict(
            self.iter()
                .map(|(k, v)| (k.to_frozen_value(heap), v.to_frozen_value(heap))),
        ))
    }
}

impl ToFrozenValue for ScriptArgKey {
    fn to_frozen_value(&self, heap: &FrozenHeap) -> FrozenValue {
        heap.alloc(self.deref())
    }
}

impl ToFrozenValue for ScriptArgValue {
    fn to_frozen_value(&self, heap: &FrozenHeap) -> FrozenValue {
        match self {
            Self::Bool(b) => FrozenValue::new_bool(*b),
            Self::Int(i) => heap.alloc(*i),
            Self::Float(f) => heap.alloc(*f),
            Self::String(s) => heap.alloc(s.clone()),
            Self::Sequence(s) => heap.alloc(AllocList(s.iter().map(|e| e.to_frozen_value(heap)))),
            Self::Table(t) => heap.alloc(AllocDict(
                t.iter()
                    .map(|(k, v)| (k.to_owned(), v.to_frozen_value(heap))),
            )),
        }
    }
}
