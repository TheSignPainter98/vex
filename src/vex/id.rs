use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt::Display,
    ops::{Deref, RangeTo},
    sync::Mutex,
};

use allocative::Allocative;
use dupe::Dupe;
use serde::Serialize;

use crate::source_path::PrettyPath;

#[derive(Clone, Debug, Allocative, Dupe, Serialize)]
pub struct VexId {
    #[serde(skip)]
    id: u32,

    pub pretty: PrettyVexId,
}

static ID_STORE: IdStore = IdStore::new();

impl VexId {
    pub fn new(path: PrettyPath) -> Self {
        ID_STORE.create_id(path)
    }

    pub fn retrieve(pretty: &PrettyVexId) -> Self {
        ID_STORE.get(pretty)
    }
}

#[cfg(test)]
impl VexId {
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl PartialEq for VexId {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for VexId {}

impl PartialOrd for VexId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VexId {
    fn cmp(&self, other: &Self) -> Ordering {
        let Self { id, pretty } = self;
        let Self {
            id: other_id,
            pretty: other_pretty,
        } = other;
        (pretty, id).cmp(&(other_pretty, other_id))
    }
}

impl Deref for VexId {
    type Target = PrettyVexId;

    fn deref(&self) -> &Self::Target {
        &self.pretty
    }
}

impl Display for VexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty.fmt(f)
    }
}

#[derive(Debug, Clone, Allocative)]
pub struct PrettyVexId {
    path: PrettyPath,
    #[allocative(skip)]
    byte_range: RangeTo<usize>,
}

impl PrettyVexId {
    pub fn new(path: PrettyPath) -> Self {
        let byte_range = if let Some(stripped) = path.as_str().strip_suffix(".star") {
            ..stripped.len()
        } else {
            ..path.as_str().len()
        };
        Self { path, byte_range }
    }

    pub fn as_str(&self) -> &str {
        &self.path.as_str()[self.byte_range.clone()]
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

impl Display for PrettyVexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Serialize for PrettyVexId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
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
    id_map: Mutex<BTreeMap<PrettyVexId, VexId>>,
}

impl IdStore {
    pub const fn new() -> Self {
        Self {
            id_map: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn create_id(&self, path: PrettyPath) -> VexId {
        let mut id_map = self
            .id_map
            .lock()
            .expect("internal error: failed to lock ID map");
        let id = id_map.len() as u32;
        let pretty = PrettyVexId::new(path);
        let ret = VexId {
            id,
            pretty: pretty.dupe(),
        };
        id_map.insert(pretty, ret.dupe());
        ret
    }

    pub fn get(&self, pretty: &PrettyVexId) -> VexId {
        self.id_map
            .lock()
            .expect("internal error: failed to lock ID map")
            .get(pretty)
            .expect("internal error: invalid vex ID")
            .dupe()
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
        assert_ne!(x.id(), y.id());
    }

    #[test]
    fn pretty() {
        let path = "foo/bar/baz.star";
        let id = VexId::new(PrettyPath::new(path.into()));
        assert_eq!(id.as_str(), "foo/bar/baz");
        assert_eq!(id.as_str(), id.pretty.as_str());
    }
}
