use std::ops::Range;

#[derive(Debug)]
pub struct IgnoreMarkers {
    ignore_ranges: Vec<Range<usize>>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn contains(&self, index: usize) -> bool {
        let possible_ignores_end = self
            .ignore_ranges
            .partition_point(|range| range.start <= index);
        self.ignore_ranges[..possible_ignores_end]
            .iter()
            .any(|range| index < range.end)
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
        ignore_ranges.sort_by_key(|range| (range.start, -(range.end as i64)));
        IgnoreMarkers { ignore_ranges }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ignores() {
        let mut ignores_builder = IgnoreMarkers::builder();
        ignores_builder.add(3..10);
        ignores_builder.add(4..9);
        ignores_builder.add(4..10);
        ignores_builder.add(11..13);
        let ignores = ignores_builder.build();

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
        tests.into_iter().for_each(|(index, expect_contained)| {
            assert_eq!(
                ignores.contains(index),
                expect_contained,
                "index {index}: expected {expect_contained}, got {}",
                ignores.contains(index)
            );
        });
    }
}
