use num_traits::Num;
use std::fmt::{Display, Formatter};

pub struct Plural<'a, N: Num> {
    num: N,
    singular: &'a str,
    plural: &'a str,
}

impl<'a, N: Num> Plural<'a, N> {
    #[allow(unused)]
    pub fn new(num: N, singular: &'a str, plural: &'a str) -> Self {
        Self {
            num,
            singular,
            plural,
        }
    }
}

impl<N: Num + Display> Display for Plural<'_, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.num == N::one() {
            write!(f, "{} {}", self.num, self.singular)
        } else {
            write!(f, "{} {}", self.num, self.plural)
        }
    }
}
