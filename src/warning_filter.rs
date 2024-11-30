use std::{collections::HashSet, hash::Hash};

use crate::id::{GroupId, LintId};

#[derive(Clone, Debug)]
pub struct WarningFilter {
    active_lints: ExclusionSet<LintId>,
    active_groups: ExclusionSet<GroupId>,
}

impl WarningFilter {
    pub fn new(active_lints: ExclusionSet<LintId>, active_groups: ExclusionSet<GroupId>) -> Self {
        Self {
            active_lints,
            active_groups,
        }
    }

    pub fn all() -> Self {
        Self {
            active_lints: ExclusionSet::all(),
            active_groups: ExclusionSet::all(),
        }
    }

    pub fn is_active(&self, id: &LintId) -> bool {
        self.active_lints.is_active(id)
    }

    pub fn is_active_with_group(&self, lint_id: &LintId, group_id: &GroupId) -> bool {
        self.active_groups.is_active(group_id) && self.is_active(lint_id)
    }
}

/// A set in which all unspecified elements are present.
#[derive(Clone, Debug)]
pub struct ExclusionSet<T> {
    excluded: HashSet<T>,
}

impl<T: Eq + Hash> ExclusionSet<T> {
    pub fn from_excluded(excluded: impl IntoIterator<Item = T>) -> Self {
        Self {
            excluded: excluded.into_iter().collect(),
        }
    }

    pub fn all() -> Self {
        Self {
            excluded: HashSet::new(),
        }
    }

    pub fn is_active(&self, id: &T) -> bool {
        !self.excluded.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::result::Result;

    use super::*;

    #[test]
    fn from_inactive() {
        let raw_inactive_id = "inactive";
        let active_lints = ExclusionSet::from_excluded(
            [raw_inactive_id]
                .iter()
                .map(|id| String::from(*id))
                .map(LintId::try_from)
                .collect::<Result<Vec<_>>>()
                .unwrap(),
        );
        let active_groups = ExclusionSet::from_excluded(
            [raw_inactive_id]
                .iter()
                .map(|id| String::from(*id))
                .map(GroupId::try_from)
                .collect::<Result<Vec<_>>>()
                .unwrap(),
        );

        let raw_active_id = "active";
        let warning_filter = WarningFilter::new(active_lints, active_groups);
        assert!(warning_filter.is_active(&LintId::try_from(raw_active_id.to_owned()).unwrap()));
        assert!(!warning_filter.is_active(&LintId::try_from(raw_inactive_id.to_owned()).unwrap()));
    }

    #[test]
    fn all() {
        let warning_filter = WarningFilter::all();
        let id = LintId::try_from("some-id".to_owned()).unwrap();
        assert!(warning_filter.is_active(&id));
    }
}
