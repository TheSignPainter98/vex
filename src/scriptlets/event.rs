use std::{fmt::Display, str::FromStr, sync::Arc};

use allocative::Allocative;
use camino::Utf8Path;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    starlark_simple_value,
    values::{NoSerialize, ProvidesStaticType, StarlarkValue},
};
use starlark_derive::starlark_value;
use strum::EnumIter;

use crate::error::Error;

pub trait Event {
    const TYPE: EventType;
}

#[derive(Copy, Clone, Debug, EnumIter, PartialEq, Eq, Allocative)]
pub enum EventType {
    OpenProject,
    OpenFile,
    Match,
    CloseFile,
    CloseProject,
}

impl EventType {
    #[allow(unused)]
    fn name(&self) -> &str {
        match self {
            EventType::OpenProject => "open_project",
            EventType::OpenFile => "open_file",
            EventType::Match => "match",
            EventType::CloseFile => "close_file",
            EventType::CloseProject => "close_project",
        }
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "open_project" => Ok(EventType::OpenProject),
            "open_file" => Ok(EventType::OpenFile),
            "match" => Ok(EventType::Match),
            "close_file" => Ok(EventType::CloseFile),
            "close_project" => Ok(EventType::CloseProject),
            _ => Err(Error::UnknownEvent(s.to_owned()).into()),
        }
    }
}

impl Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct OpenProjectEvent {
    #[allocative(skip)]
    project_root: Arc<Utf8Path>,
}
starlark_simple_value!(OpenProjectEvent);

impl Event for OpenProjectEvent {
    const TYPE: EventType = EventType::OpenProject;
}

#[starlark_value(type = "OpenProjectEvent")]
impl<'v> StarlarkValue<'v> for OpenProjectEvent {}

impl Display for OpenProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct OpenFileEvent {
    #[allocative(skip)]
    path: Arc<Utf8Path>,
}
starlark_simple_value!(OpenFileEvent);

impl Event for OpenFileEvent {
    const TYPE: EventType = EventType::OpenFile;
}

#[starlark_value(type = "OpenFileEvent")]
impl<'v> StarlarkValue<'v> for OpenFileEvent {}

impl Display for OpenFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct MatchEvent {}
starlark_simple_value!(MatchEvent);

impl Event for MatchEvent {
    const TYPE: EventType = EventType::Match;
}

#[starlark_value(type = "MatchEvent")]
impl<'v> StarlarkValue<'v> for MatchEvent {}

impl Display for MatchEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct CloseFileEvent {
    #[allocative(skip)]
    path: Arc<Utf8Path>,
}
starlark_simple_value!(CloseFileEvent);

impl Event for CloseFileEvent {
    const TYPE: EventType = EventType::CloseFile;
}

#[starlark_value(type = "CloseFileEvent")]
impl<'v> StarlarkValue<'v> for CloseFileEvent {}

impl Display for CloseFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct CloseProjectEvent {
    #[allocative(skip)]
    project_root: Arc<Utf8Path>,
}
starlark_simple_value!(CloseProjectEvent);

impl Event for CloseProjectEvent {
    const TYPE: EventType = EventType::CloseProject;
}

#[starlark_value(type = "CloseProjectEvent")]
impl<'v> StarlarkValue<'v> for CloseProjectEvent {}

impl Display for CloseProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}
