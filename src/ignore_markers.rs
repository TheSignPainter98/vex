use std::ops::Range;

#[derive(Debug)]
pub struct IgnoreMarkers {
    ignore_ranges: Vec<Range<usize>>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn check_marked(&self, index: usize) -> bool {
        let relevant_range_cap = self
            .ignore_ranges
            .partition_point(|range| range.start <= index);
        self.ignore_ranges[..relevant_range_cap]
            .iter()
            .any(|range| index < range.end)
    }

    #[cfg(test)]
    pub fn ignore_ranges(&self) -> &[Range<usize>] {
        &self.ignore_ranges
    }
}

pub struct IgnoreMarkersBuilder {
    ignore_ranges: Vec<Range<usize>>,
}

impl IgnoreMarkersBuilder {
    pub fn new() -> Self {
        Self {
            ignore_ranges: Vec::new(),
        }
    }

    pub fn add(&mut self, range: Range<usize>) {
        self.ignore_ranges.push(range)
    }

    pub fn build(self) -> IgnoreMarkers {
        let Self { mut ignore_ranges } = self;
        ignore_ranges.sort_by_key(|range| range.start);
        IgnoreMarkers { ignore_ranges }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ignore_markers() {
        let mut ignore_markers_builder = IgnoreMarkers::builder();
        ignore_markers_builder.add(3..10);
        ignore_markers_builder.add(4..9);
        ignore_markers_builder.add(4..10);
        ignore_markers_builder.add(11..13);
        let ignore_markers = ignore_markers_builder.build();

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
                ignore_markers.check_marked(index),
                expected,
                "index {index}: expected {expected}, got {}",
                ignore_markers.check_marked(index)
            );
        });
    }
}
