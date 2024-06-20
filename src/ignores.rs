use std::ops::Range;

pub struct Ignores {
    ignores: Vec<Range<usize>>,
}

impl Ignores {
    pub fn builder() -> IgnoresBuilder {
        IgnoresBuilder::new()
    }

    pub fn contains(&self, index: usize) -> bool {
        let possible_ignores_end = self.ignores.partition_point(|range| range.start <= index);
        self.ignores[..possible_ignores_end]
            .iter()
            .any(|range| index < range.end)
    }
}

pub struct IgnoresBuilder {
    ignores: Vec<Range<usize>>,
}

impl IgnoresBuilder {
    pub fn new() -> Self {
        Self {
            ignores: Vec::new(),
        }
    }

    pub fn add(&mut self, range: Range<usize>) {
        self.ignores.push(range)
    }

    pub fn build(self) -> Ignores {
        let Self { mut ignores } = self;
        ignores.sort_by_key(|range| (range.start, -(range.end as i64)));
        Ignores { ignores }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ignores() {
        let mut ignores_builder = Ignores::builder();
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
