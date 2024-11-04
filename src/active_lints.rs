use std::collections::HashSet;

use crate::vex_id::VexId;

#[derive(Clone, Debug)]
pub struct ActiveLints {
    inactive: HashSet<VexId>,
}

impl ActiveLints {
    pub fn all() -> Self {
        ActiveLints {
            inactive: HashSet::new(),
        }
    }

    pub fn is_active(&self, id: &VexId) -> bool {
        !self.inactive.contains(id)
    }
}

impl Default for ActiveLints {
    fn default() -> Self {
        let inactive = [VexId::try_from("pedantic".to_owned()).unwrap()]
            .into_iter()
            .collect();
        ActiveLints { inactive }
    }
}
