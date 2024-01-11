mod id;

use crate::scriptlets::{stage::Vexing, ScriptletRef};

pub use self::id::Id;

pub struct Vex<'s> {
    pub id: Id,
    pub scriptlet: ScriptletRef<'s, Vexing>,
}
