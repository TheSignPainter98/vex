use allocative::Allocative;

use crate::scriptlets::event::EventType;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Allocative)]
pub enum Action {
    Preiniting,
    Initing,
    Vexing(EventType),
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preiniting => "preiniting",
            Self::Initing => "initing",
            Self::Vexing(_) => "vexing",
        }
    }
}
