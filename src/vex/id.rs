use std::{
    borrow::Borrow, cmp::Ordering, collections::BTreeMap, fmt::Display, ops::RangeTo, sync::Mutex,
};

use allocative::Allocative;
use dupe::{Dupe, OptionDupedExt};
use serde::Serialize;

use crate::source_path::PrettyPath;

#[derive(Copy, Clone, Debug, Allocative, Dupe)]
pub struct VexId(usize);

static ID_STORE: Mutex<IdStore> = Mutex::new(IdStore::new());

impl VexId {
    pub fn new(path: PrettyPath) -> Self {
        ID_STORE
            .lock()
            .expect("internal error: failed to lock ID store")
            .create_id(path)
    }

    pub fn retrieve(pretty: &PrettyVexId) -> Option<Self> {
        Self::retrieve_str(pretty.as_str())
    }

    pub fn retrieve_str(raw: &str) -> Option<Self> {
        ID_STORE
            .lock()
            .expect("internal error: failed to lock ID store")
            .get_id(raw)
    }

    pub fn to_pretty(self) -> PrettyVexId {
        ID_STORE
            .lock()
            .expect("internal error: failed to lock ID store")
            .get_pretty_id(self)
            .expect("internal error: invalid ID")
    }
}

#[cfg(test)]
impl VexId {
    pub fn into_inner(self) -> usize {
        self.0
    }
}

impl PartialEq for VexId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for VexId {}

impl Display for VexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_pretty().fmt(f)
    }
}

#[derive(Debug, Clone, Allocative)]
pub enum PrettyVexId {
    Raw(String),
    Path {
        path: PrettyPath,

        #[allocative(skip)]
        byte_range: RangeTo<usize>,
    },
}

impl PrettyVexId {
    pub fn from_path(path: PrettyPath) -> Self {
        let byte_range = if let Some(stripped) = path.as_str().strip_suffix(".star") {
            ..stripped.len()
        } else {
            ..path.as_str().len()
        };
        Self::Path { path, byte_range }
    }

    pub fn from_raw(raw: String) -> Self {
        Self::Raw(raw)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Raw(s) => s,
            Self::Path { path, byte_range } => &path.as_str()[*byte_range],
        }
    }
}

impl PartialEq for PrettyVexId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for PrettyVexId {}

impl PartialOrd for PrettyVexId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrettyVexId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl AsRef<str> for PrettyVexId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for PrettyVexId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Display for PrettyVexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Serialize for PrettyVexId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl Dupe for PrettyVexId {
    // Fields:
    // .path: Dupe
    // .byte_range: !Dupe but cheap
}

#[derive(Debug, Default)]
struct IdStore {
    id_map: BTreeMap<PrettyVexId, VexId>,
    pretty_map: Vec<PrettyVexId>,
}

impl IdStore {
    pub const fn new() -> Self {
        Self {
            id_map: BTreeMap::new(),
            pretty_map: Vec::new(),
        }
    }

    pub fn create_id(&mut self, path: PrettyPath) -> VexId {
        let Self {
            ref mut id_map,
            ref mut pretty_map,
        } = self;
        let id = VexId(pretty_map.len());
        let pretty = PrettyVexId::from_path(path);
        pretty_map.push(pretty.dupe());
        id_map.insert(pretty, id);
        id
    }

    pub fn get_id(&self, pretty: &str) -> Option<VexId> {
        self.id_map.get(pretty).duped()
    }

    pub fn get_pretty_id(&self, id: VexId) -> Option<PrettyVexId> {
        self.pretty_map.get(id.0).duped()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn unique_ids() {
        let path = "foo/bar/baz.star".into();
        let x = VexId::new(PrettyPath::new(path));
        let y = VexId::new(PrettyPath::new(path));
        assert_ne!(x.into_inner(), y.into_inner());
    }

    #[test]
    fn pretty() {
        let path = "foo/bar/baz.star";
        let id = VexId::new(PrettyPath::new(path.into()));
        assert_eq!(id.to_pretty().as_str(), "foo/bar/baz");
    }
}
