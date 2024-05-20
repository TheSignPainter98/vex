use allocative::Allocative;

use crate::scriptlets::event::EventKind;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Allocative)]
pub enum Action {
    Preiniting,
    Initing,
    Vexing(EventKind),
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preiniting => "preiniting",
            Self::Initing => "initing",
            Self::Vexing(e) => e.pretty_name(),
        }
    }
}
