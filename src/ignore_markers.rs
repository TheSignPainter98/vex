use std::ops::Range;

#[derive(Debug)]
pub struct IgnoreMarkers {
    markers: Vec<IgnoreMarker>,
}

impl IgnoreMarkers {
    pub fn builder() -> IgnoreMarkersBuilder {
        IgnoreMarkersBuilder::new()
    }

    pub fn check_marked(&self, index: usize) -> bool {
        let relevant_range_cap = self
            .markers
            .partition_point(|marker| marker.byte_range.start <= index);
        self.markers[..relevant_range_cap]
            .iter()
            .any(|marker| index < marker.byte_range.end)
    }

    #[cfg(test)]
    pub fn ignore_ranges<'a>(&'a self) -> impl Iterator<Item = Range<usize>> + 'a {
        self.markers.iter().map(|marker| marker.byte_range.clone())
    }
}

pub struct IgnoreMarkersBuilder {
    markers: Vec<IgnoreMarker>,
}

impl IgnoreMarkersBuilder {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
        }
    }

    pub fn add(&mut self, byte_range: Range<usize>) {
        self.markers.push(IgnoreMarker { byte_range })
    }

    pub fn build(self) -> IgnoreMarkers {
        let Self { mut markers } = self;
        markers.sort_by_key(|range| range.byte_range.start);
        IgnoreMarkers { markers }
    }
}

#[derive(Debug)]
struct IgnoreMarker {
    byte_range: Range<usize>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ignore_markers() {
        let ignore_markers = {
            let mut builder = IgnoreMarkers::builder();
            builder.add(3..10);
            builder.add(4..9);
            builder.add(4..10);
            builder.add(11..13);
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
                ignore_markers.check_marked(index),
                expected,
                "index {index}: expected {expected}, got {}",
                ignore_markers.check_marked(index)
            );
        });
    }
}
