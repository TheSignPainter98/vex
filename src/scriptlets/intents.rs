use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use allocative::Allocative;
use derive_more::Display;
use starlark::values::{AllocValue, Freeze, StarlarkValue, Value};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType, Trace};

use crate::source_path::PrettyPath;
use crate::{
    irritation::Irritation,
    query::Query,
    scriptlets::{event::EventKind, observers::UnfrozenObserver, Observer},
    supported_language::SupportedLanguage,
};

#[derive(Debug, Display, ProvidesStaticType, NoSerialize, Allocative, Trace)]
#[display(fmt = "Intents")]
pub struct UnfrozenIntents<'v>(RefCell<Vec<UnfrozenIntent<'v>>>);

impl<'v> UnfrozenIntents<'v> {
    pub fn new() -> Self {
        Self(RefCell::new(Vec::with_capacity(10)))
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn declare(&self, intent: UnfrozenIntent<'v>) {
        self.0.borrow_mut().push(intent)
    }
}

impl<'v> Deref for UnfrozenIntents<'v> {
    type Target = RefCell<Vec<UnfrozenIntent<'v>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'v> DerefMut for UnfrozenIntents<'v> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[starlark_value(type = "Intents")]
impl<'v> StarlarkValue<'v> for UnfrozenIntents<'v> {}

impl<'v> AllocValue<'v> for UnfrozenIntents<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl<'v> Freeze for UnfrozenIntents<'v> {
    type Frozen = Intents;

    fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        Ok(Intents(
            self.0
                .into_inner()
                .into_iter()
                .map(|intent| intent.freeze(freezer))
                .collect::<anyhow::Result<Vec<_>>>()?,
        ))
    }
}

#[derive(Debug, Trace, Allocative)]
pub enum UnfrozenIntent<'v> {
    Find {
        language: SupportedLanguage,
        #[allocative(skip)]
        query: Arc<Query>,
        on_match: UnfrozenObserver<'v>,
    },
    Observe {
        event_kind: EventKind,
        observer: UnfrozenObserver<'v>,
    },
    Warn(Irritation),
    ScanFile {
        file_name: PrettyPath,
        language: SupportedLanguage,
        content: String,
    },
}

impl<'v> Freeze for UnfrozenIntent<'v> {
    type Frozen = Intent;

    fn freeze(self, freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        Ok(match self {
            Self::Find {
                language,
                query,
                on_match,
            } => {
                let on_match = on_match.freeze(freezer)?;
                Intent::Find {
                    language,
                    query,
                    on_match,
                }
            }
            Self::Observe {
                event_kind,
                observer,
            } => {
                let observer = observer.freeze(freezer)?;
                Intent::Observe {
                    event_kind,
                    observer,
                }
            }
            Self::Warn(irr) => Intent::Warn(irr),
            Self::ScanFile {
                file_name,
                language,
                content,
            } => Intent::ScanFile {
                file_name,
                language,
                content,
            },
        })
    }
}

#[derive(Clone, Debug, Display, ProvidesStaticType, NoSerialize, Allocative, Trace)]
#[display(fmt = "Intents")]
pub struct Intents(Vec<Intent>);

impl IntoIterator for Intents {
    type Item = Intent;

    type IntoIter = <Vec<Intent> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Deref for Intents {
    type Target = [Intent];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromIterator<Intent> for Intents {
    fn from_iter<T: IntoIterator<Item = Intent>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FromIterator<Intents> for Intents {
    fn from_iter<T: IntoIterator<Item = Intents>>(iter: T) -> Self {
        Self(iter.into_iter().flat_map(|Intents(is)| is).collect())
    }
}

#[starlark_value(type = "Intents")]
impl<'v> StarlarkValue<'v> for Intents {}

#[derive(Debug, Clone, Allocative)]
pub enum Intent {
    Find {
        language: SupportedLanguage,
        #[allocative(skip)]
        query: Arc<Query>,
        on_match: Observer,
    },
    Observe {
        event_kind: EventKind,
        observer: Observer,
    },
    Warn(Irritation),
    ScanFile {
        file_name: PrettyPath,
        language: SupportedLanguage,
        content: String,
    },
}
