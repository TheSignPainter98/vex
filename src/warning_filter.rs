use std::collections::HashSet;

use crate::vex_id::VexId;

#[derive(Clone, Debug)]
pub struct WarningFilter {
    active_lints: ActiveIds,
    active_groups: ActiveIds,
}

impl WarningFilter {
    pub fn new(active_lints: ActiveIds, active_groups: ActiveIds) -> Self {
        Self {
            active_lints,
            active_groups,
        }
    }

    pub fn all() -> Self {
        Self {
            active_lints: ActiveIds::all(),
            active_groups: ActiveIds::all(),
        }
    }

    pub fn is_active(&self, lint_id: &VexId) -> bool {
        self.active_lints.is_active(lint_id)
    }

    pub fn is_active_with_group(&self, lint_id: &VexId, group_id: &VexId) -> bool {
        self.active_groups.is_active(group_id) && self.is_active(lint_id)
    }
}

#[derive(Clone, Debug)]
pub struct ActiveIds {
    inactive: HashSet<VexId>,
}

impl ActiveIds {
    pub fn from_inactive(inactive: impl IntoIterator<Item = VexId>) -> Self {
        Self {
            inactive: inactive.into_iter().collect(),
        }
    }

    pub fn all() -> Self {
        Self {
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
        let active_lints = ActiveIds::from_inactive(inactive.clone());
        let active_groups = ActiveIds::from_inactive(inactive);
        let warning_filter = WarningFilter::new(active_lints, active_groups);
        assert!(warning_filter.is_active(&active_id));
        assert!(!warning_filter.is_active(&inactive_id));
    }

    #[test]
    fn all() {
        let id = VexId::try_from("some-id".to_owned()).unwrap();
        let warning_filter = WarningFilter::all();
        assert!(warning_filter.is_active(&id));
    }
}
