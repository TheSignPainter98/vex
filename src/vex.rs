pub mod id;

use std::marker::PhantomData;

use self::id::VexId;

#[allow(unused)]
pub struct Vex<'s> {
    pub id: VexId,
    _marker: PhantomData<&'s ()>,
    // pub scriptlet: ScriptletRef<'s>,
}
