use std::collections::HashSet;

use crate::vex_id::VexId;

#[derive(Clone, Debug)]
pub struct ActiveLints {
    inactive: HashSet<VexId>,
}

impl ActiveLints {
    pub fn from_inactive(inactive: impl IntoIterator<Item = VexId>) -> Self {
        Self {
            inactive: inactive.into_iter().collect(),
        }
    }

    pub fn all() -> Self {
        ActiveLints {
            inactive: HashSet::new(),
        }
    }

    pub fn is_active(&self, id: &VexId) -> bool {
        !self.inactive.contains(id)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn from_inactive() {
        let active_id = VexId::try_from("active".to_owned()).unwrap();
        let inactive_id = VexId::try_from("inactive".to_owned()).unwrap();

        let inactive = [inactive_id.clone()];
        let active_lints = ActiveLints::from_inactive(inactive);
        assert!(active_lints.is_active(&active_id));
        assert!(!active_lints.is_active(&inactive_id));
    }

    #[test]
    fn all() {
        let id = VexId::try_from("some-id".to_owned()).unwrap();
        let active_lints = ActiveLints::all();
        assert!(active_lints.is_active(&id));
    }
}
