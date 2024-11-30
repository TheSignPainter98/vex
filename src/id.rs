use std::{
    cmp::Ordering,
    collections::hash_map::DefaultHasher,
    fmt::Display,
    hash::{Hash, Hasher},
};

use allocative::Allocative;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;

use crate::{
    error::{Error, InvalidIDReason},
    result::Result,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Allocative, Serialize)]
pub struct LintId(Id);

impl LintId {
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl TryFrom<String> for LintId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Ok(Self(value.try_into()?))
    }
}

impl AsRef<Id> for LintId {
    fn as_ref(&self) -> &Id {
        &self.0
    }
}

impl Display for LintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Allocative, Serialize)]
pub struct GroupId(Id);

impl GroupId {
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl TryFrom<String> for GroupId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Ok(Self(value.try_into()?))
    }
}

impl AsRef<Id> for GroupId {
    fn as_ref(&self) -> &Id {
        &self.0
    }
}

impl Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

#[derive(Debug, Clone, Allocative, Eq, PartialEq, Serialize)]
struct Id {
    hash: u64,
    name: String,
}

impl Id {
    fn new_raw(name: String) -> Self {
        let hash = {
            let mut hasher = DefaultHasher::new();
            name.hash(&mut hasher);
            hasher.finish()
        };
        Self { hash, name }
    }
}

impl TryFrom<String> for Id {
    type Error = Error;

    fn try_from(raw_id: String) -> Result<Self> {
        let invalid_id = |reason| Error::InvalidID {
            raw_id: raw_id.to_string(),
            reason,
        };

        const MIN_ID_LEN: usize = 3;
        const MAX_ID_LEN: usize = 25;
        if raw_id.len() < MIN_ID_LEN {
            return Err(invalid_id(InvalidIDReason::TooShort {
                len: raw_id.len(),
                min_len: MIN_ID_LEN,
            }));
        }
        if raw_id.len() > MAX_ID_LEN {
            return Err(invalid_id(InvalidIDReason::TooLong {
                len: raw_id.len(),
                max_len: MAX_ID_LEN,
            }));
        }

        lazy_static! {
            static ref VALID_VEX_ID: Regex = Regex::new("^[a-z0-9:-]*$").unwrap();
        }
        if !VALID_VEX_ID.is_match(&raw_id) {
            return Err(invalid_id(InvalidIDReason::IllegalChar));
        }
        let first_char = raw_id.chars().next().unwrap();
        match first_char {
            '0'..='9' | ':' | '-' => {
                return Err(invalid_id(InvalidIDReason::IllegalStartChar(first_char)))
            }
            _ => {}
        }
        let last_char = raw_id.chars().next_back().unwrap();
        match last_char {
            ':' | '-' => return Err(invalid_id(InvalidIDReason::IllegalEndChar(last_char))),
            _ => {}
        }

        for ugly_substring in ["::", "--", ":-", "-:"] {
            if let Some(index) = raw_id.find(ugly_substring) {
                return Err(invalid_id(InvalidIDReason::UglySubstring {
                    found: ugly_substring.to_string(),
                    index,
                }));
            }
        }

        Ok(Self::new_raw(raw_id))
    }
}

impl Ord for Id {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Id {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Id {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from() {
        let check_valid =
            |raw_id: &str| assert_eq!(Id::try_from(raw_id.to_string()).unwrap().as_ref(), raw_id);
        check_valid("hello");
        check_valid("hello1234");
        check_valid("hello:world:1234");
        check_valid("hello:world-1234");

        macro_rules! check_invalid {
            ($raw_id:literal, $pred:expr $(,)?) => {
                let Error::InvalidID { raw_id, reason } =
                    Id::try_from($raw_id.to_string()).unwrap_err()
                else {
                    panic!("incorrect error");
                };
                assert_eq!(raw_id, $raw_id);
                let pred = $pred; // Placate clippy.
                assert!(pred(reason));
            };
        }
        check_invalid!("", |reason| matches!(
            reason,
            InvalidIDReason::TooShort { len: 0, .. }
        ));
        check_invalid!("i-am-very-very-very-very-long", |reason| matches!(
            reason,
            InvalidIDReason::TooLong { len: 29, .. }
        ));
        check_invalid!("asdf_fdas", |reason| matches!(
            reason,
            InvalidIDReason::IllegalChar
        ));
        check_invalid!("asdf/fdas", |reason| matches!(
            reason,
            InvalidIDReason::IllegalChar
        ));
        check_invalid!("🏁🏁🏁🏁🏁", |reason| matches!(
            reason,
            InvalidIDReason::IllegalChar
        ));
        check_invalid!("hello world", |reason| matches!(
            reason,
            InvalidIDReason::IllegalChar
        ));
        check_invalid!("-hello", |reason| matches!(
            reason,
            InvalidIDReason::IllegalStartChar('-')
        ));
        check_invalid!(":hello", |reason| matches!(
            reason,
            InvalidIDReason::IllegalStartChar(':')
        ));
        check_invalid!("5hello", |reason| matches!(
            reason,
            InvalidIDReason::IllegalStartChar('5')
        ));
        check_invalid!("hello-", |reason| matches!(
            reason,
            InvalidIDReason::IllegalEndChar('-')
        ));
        check_invalid!("hello:", |reason| matches!(
            reason,
            InvalidIDReason::IllegalEndChar(':')
        ));
        check_invalid!("hello:", |reason| matches!(
            reason,
            InvalidIDReason::IllegalEndChar(':')
        ));
        check_invalid!("hello--world", |reason| match reason {
            InvalidIDReason::UglySubstring { found, .. } => found == "--",
            _ => false,
        });
        check_invalid!("hello:-world", |reason| match reason {
            InvalidIDReason::UglySubstring { found, .. } => found == ":-",
            _ => false,
        });
        check_invalid!("hello-:world", |reason| match reason {
            InvalidIDReason::UglySubstring { found, .. } => found == "-:",
            _ => false,
        });
        check_invalid!("hello::world", |reason| match reason {
            InvalidIDReason::UglySubstring { found, .. } => found == "::",
            _ => false,
        });
    }
}
