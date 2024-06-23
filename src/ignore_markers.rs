use std::ops::Range;

use log::{log_enabled, warn};
use smallvec::SmallVec;

use crate::{scriptlets::Location, source_path::PrettyPath, vex::id::VexId};

#[derive(Debug)]
pub struct IgnoreMarkers {
    markers: Vec<IgnoreMarker>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn marked(&self, byte_index: usize, vex_id: VexId) -> bool {
        let relevant_range_cap = self
            .markers
            .partition_point(|marker| marker.byte_range.start <= byte_index);
        self.markers[..relevant_range_cap]
            .iter()
            .filter(|marker| byte_index < marker.byte_range.end)
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
        markers.sort_by_key(|range| range.byte_range.start);
        IgnoreMarkers { markers }
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
    // This function created a new `VexIdFilter` from a comma-separated list of stringified
    // pretty vex ids. If any vex ids are unknown, the first unknown one will be returned as an
    // error.
    pub fn new(raw: &str, opts: NewVexIdFilterOpts<'_>) -> Self {
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
pub struct NewVexIdFilterOpts<'path> {
    pub path: &'path PrettyPath,
    pub location: Location,
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
