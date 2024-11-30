use std::ops::Range;

use smallvec::SmallVec;

use crate::{error::Error, id::LintId, result::RecoverableResult};

#[derive(Debug)]
pub struct IgnoreMarkers {
    markers: Vec<IgnoreMarker>,
    marker_ends: Vec<MarkerEnd>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn is_ignored(&self, byte_index: usize, id: &LintId) -> bool {
        if self.markers.is_empty() {
            return false;
        }

        if byte_index < self.markers[0].byte_range.start {
            return false;
        }
        if byte_index >= self.marker_ends[self.marker_ends.len() - 1].byte_index {
            return false;
        }

        let first_possible_index = {
            let end_index = self
                .marker_ends
                .partition_point(|end| end.byte_index < byte_index);
            self.marker_ends[end_index].marker_index
        };
        let last_possible_index = first_possible_index
            + self.markers[first_possible_index..]
                .partition_point(|marker| marker.byte_range.start <= byte_index);
        self.markers[first_possible_index..last_possible_index]
            .iter()
            .filter(|marker| marker.filter.covers(id))
            .any(|marker| marker.byte_range.contains(&byte_index))
    }

    #[cfg(test)]
    pub fn markers(&self) -> impl Iterator<Item = &IgnoreMarker> {
        self.markers.iter()
    }

    #[cfg(test)]
    pub fn ignore_ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.markers.iter().map(|marker| marker.byte_range.clone())
    }
}

#[derive(Debug, Default)]
pub struct IgnoreMarkersBuilder {
    markers: Vec<IgnoreMarker>,
}

impl IgnoreMarkersBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, byte_range: Range<usize>, filter: LintIdFilter) {
        self.markers.push(IgnoreMarker { byte_range, filter })
    }

    pub fn build(self) -> IgnoreMarkers {
        let Self { mut markers } = self;
        markers.sort_by_key(|range| (range.byte_range.start, range.byte_range.end));

        let marker_ends = {
            let mut marker_ends: Vec<_> = markers
                .iter()
                .enumerate()
                .map(|(i, range)| MarkerEnd {
                    byte_index: range.byte_range.end,
                    marker_index: i,
                })
                .collect();
            marker_ends.sort();
            if !marker_ends.is_empty() {
                for i in 0..marker_ends.len() - 1 {
                    if marker_ends[i].marker_index > marker_ends[i + 1].marker_index {
                        marker_ends[i].marker_index = marker_ends[i + 1].marker_index;
                    }
                }
            }
            marker_ends
        };
        debug_assert_eq!(markers.len(), marker_ends.len());

        IgnoreMarkers {
            markers,
            marker_ends,
        }
    }
}

#[derive(Debug)]
pub struct IgnoreMarker {
    byte_range: Range<usize>,
    filter: LintIdFilter,
}

#[cfg(test)]
impl IgnoreMarker {
    pub fn filter(&self) -> &LintIdFilter {
        &self.filter
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintIdFilter {
    All,
    Specific(SmallVec<[LintId; 2]>),
}

impl LintIdFilter {
    // This function creates a new `LintIdFilter` from a comma-separated list of stringified
    // pretty vex ids. If any vex ids are unknown, the first unknown one will be returned as an
    // error.
    pub fn try_from_iter<'a>(
        mut raw_ids: impl Iterator<Item = &'a str>,
    ) -> RecoverableResult<Self> {
        let (min, max) = raw_ids.size_hint();
        let capacity = max.unwrap_or(min);

        let mut ids = SmallVec::with_capacity(capacity);
        let mut errs = vec![];
        let mut star_found = false;
        for raw_id in &mut raw_ids {
            if raw_id == "*" {
                star_found = true;
                continue;
            }
            match LintId::try_from(raw_id.to_string()) {
                Ok(id) => ids.push(id),
                Err(err) => errs.push(err),
            };
        }

        if star_found && !ids.is_empty() {
            errs.push(Error::RedundantIgnore)
        }

        let ret = if star_found {
            Self::All
        } else {
            Self::Specific(ids)
        };
        if !errs.is_empty() {
            return RecoverableResult::Recovered(ret, errs);
        }
        RecoverableResult::Ok(ret)
    }

    fn covers(&self, id: &LintId) -> bool {
        match self {
            Self::All => true,
            Self::Specific(ids) => ids.contains(id),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::All => false,
            Self::Specific(ids) => ids.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MarkerEnd {
    byte_index: usize,
    marker_index: usize,
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use crate::error::InvalidIDReason;

    use super::*;

    #[test]
    fn ignore_ranges() {
        let lint_id = LintId::try_from("foo-bar".to_string()).unwrap();
        let ignore_markers = {
            let filter = LintIdFilter::Specific(smallvec![lint_id.clone()]);
            let mut builder = IgnoreMarkers::builder();
            builder.add(3..10, filter.clone());
            builder.add(4..9, filter.clone());
            builder.add(4..10, filter.clone());
            builder.add(11..13, filter.clone());
            builder.build()
        };

        let tests = [
            (1, false),
            (2, false),
            (3, true),
            (4, true),
            (5, true),
            (6, true),
            (7, true),
            (8, true),
            (9, true),
            (10, false),
            (11, true),
            (12, true),
            (13, false),
        ];
        tests.into_iter().for_each(|(index, expected)| {
            assert_eq!(
                ignore_markers.is_ignored(index, &lint_id),
                expected,
                "index {index}: expected {expected}, got {}",
                ignore_markers.is_ignored(index, &lint_id)
            );
        });
    }

    #[test]
    fn try_from_iter() {
        use RecoverableResult::*;

        const ID1: &str = "some-okay-id";
        const ID2: &str = "some-okay-id";
        match LintIdFilter::try_from_iter([ID1, ID2].into_iter()) {
            Ok(filter) => assert_eq!(
                filter,
                LintIdFilter::Specific(smallvec![
                    LintId::try_from(ID1.to_string()).unwrap(),
                    LintId::try_from(ID2.to_string()).unwrap()
                ])
            ),
            Recovered(_, errs) => panic!("unexpected errors: {errs:?}"),

            Err(err) => panic!("unexpected unrecoverable error: {err}"),
        }

        match LintIdFilter::try_from_iter(["hello", "*", "<><><><>"].into_iter()) {
            Ok(_) => panic!("expected result"),
            Recovered(filter, errs) => {
                assert!(!errs.is_empty());
                assert!(errs.iter().any(|err| matches!(err, Error::RedundantIgnore)));
                assert!(errs.iter().any(|err| matches!(
                    err,
                    Error::InvalidID {
                        reason: InvalidIDReason::IllegalChar,
                        ..
                    }
                )));

                assert_eq!(filter, LintIdFilter::All);
            }
            Err(err) => panic!("unexpected unrecoverable error: {err}"),
        }
    }
}
