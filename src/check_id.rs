use std::fmt::Display;

use dupe::Dupe;

use crate::{error::Error, result::Result, source_path::PrettyPath};

#[derive(Debug)]
pub struct CheckId<'s>(&'s str);

impl<'s> CheckId<'s> {
    pub fn as_str(&self) -> &'s str {
        self.0
    }
}

impl<'s> AsRef<str> for CheckId<'s> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl<'s> TryFrom<&'s PrettyPath> for CheckId<'s> {
    type Error = Error;

    fn try_from(path: &'s PrettyPath) -> Result<Self> {
        Ok(Self(
            path.as_str()
                .strip_suffix(".star")
                .ok_or_else(|| Error::NotACheckPath(path.dupe()))?,
        ))
    }
}

impl Display for CheckId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn try_from_valid() {
        let path = PrettyPath::from("foo/bar.star");
        assert_eq!(CheckId::try_from(&path).unwrap().as_str(), "foo/bar");
    }

    #[test]
    fn try_from_invalid() {
        let path = PrettyPath::from("nope.avi");
        assert_eq!(
            CheckId::try_from(&path).unwrap_err().to_string(),
            "nope.avi is not a check path"
        );
    }
}
