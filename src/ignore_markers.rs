use std::ops::Range;

use log::{log_enabled, warn};
use smallvec::SmallVec;

use crate::{scriptlets::Location, source_path::PrettyPath, vex::id::VexId};

#[derive(Debug)]
pub struct IgnoreMarkers {
    markers: Vec<IgnoreMarker>,
    marker_ends: Vec<MarkerEnd>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn marked(&self, byte_index: usize, vex_id: VexId) -> bool {
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
            .filter(|marker| marker.byte_range.contains(&byte_index))
            .any(|marker| marker.filter.covers(vex_id))
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

    pub fn add(&mut self, byte_range: Range<usize>, filter: VexIdFilter) {
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
    filter: VexIdFilter,
}

#[cfg(test)]
impl IgnoreMarker {
    pub fn filter(&self) -> &VexIdFilter {
        &self.filter
    }
}

#[derive(Debug, Clone)]
pub enum VexIdFilter {
    All,
    Specific(SmallVec<[VexId; 2]>),
}

impl VexIdFilter {
    // This function creates a new `VexIdFilter` from a comma-separated list of stringified
    // pretty vex ids. If any vex ids are unknown, the first unknown one will be returned as an
    // error.
    pub fn new(raw: &str, opts: VexIdFilterOpts<'_>) -> Self {
        if raw == "*" {
            return Self::All;
        }
        Self::Specific(
            raw.split(',')
                .flat_map(|raw_id| {
                    let id = VexId::retrieve_str(raw_id);
                    if id.is_none() && log_enabled!(log::Level::Warn) {
                        warn!("{}:{}: unknown vex '{raw_id}'", opts.path, opts.location)
                    }
                    id
                })
                .collect(),
        )
    }

    fn covers(&self, vex_id: VexId) -> bool {
        match self {
            Self::All => true,
            Self::Specific(ids) => ids.contains(&vex_id),
        }
    }
}

#[derive(Debug)]
pub struct VexIdFilterOpts<'path> {
    pub path: &'path PrettyPath,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MarkerEnd {
    byte_index: usize,
    marker_index: usize,
}

#[cfg(test)]
mod test {
    use dupe::Dupe;
    use smallvec::smallvec;

    use crate::source_path::PrettyPath;

    use super::*;

    #[test]
    fn ignore_ranges() {
        let vex_id = VexId::new(PrettyPath::new("foo/bar.star".into()));
        let ignore_markers = {
            let filter = VexIdFilter::Specific(smallvec![vex_id.dupe()]);
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
                ignore_markers.marked(index, vex_id),
                expected,
                "index {index}: expected {expected}, got {}",
                ignore_markers.marked(index, vex_id)
            );
        });
    }
}
