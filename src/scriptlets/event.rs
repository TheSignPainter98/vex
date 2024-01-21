use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use strum::EnumIter;

use crate::error::Error;

#[derive(Copy, Clone, Debug, EnumIter, PartialEq, Eq, Allocative)]
pub enum EventType {
    Start,
    Match,
    EoF,
    End,
}

impl EventType {
    #[allow(unused)]
    fn name(&self) -> &str {
        match self {
            EventType::Start => "start",
            EventType::Match => "match",
            EventType::EoF => "eof",
            EventType::End => "end",
        }
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "start" => Ok(EventType::Start),
            "match" => Ok(EventType::Match),
            "eof" => Ok(EventType::EoF),
            "end" => Ok(EventType::End),
            _ => Err(Error::UnknownEvent(s.to_owned()).into()),
        }
    }
}

impl Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}
