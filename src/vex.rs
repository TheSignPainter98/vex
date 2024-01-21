mod id;

// use crate::scriptlets::ScriptletRef;

use std::marker::PhantomData;

pub use self::id::Id;

#[allow(unused)]
pub struct Vex<'s> {
    pub id: Id,
    _marker: PhantomData<&'s ()>,
    // pub scriptlet: ScriptletRef<'s>,
}
