use allocative::Allocative;
use dupe::Dupe;

use crate::scriptlets::event::EventKind;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Allocative, Dupe)]
pub enum Action {
    Preiniting,
    Initing,
    Vexing(EventKind),
}

impl Action {
    pub fn pretty_name(&self) -> &'static str {
        match self {
            Self::Preiniting => "preiniting",
            Self::Initing => "initing",
            Self::Vexing(e) => e.pretty_name(),
        }
    }
}
