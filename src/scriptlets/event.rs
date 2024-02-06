use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    starlark_simple_value,
    values::{NoSerialize, ProvidesStaticType, StarlarkValue},
};
use starlark_derive::{starlark_attrs, starlark_value, StarlarkAttrs};
use strum::EnumIter;

use crate::{error::Error, source_path::PrettyPath};

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

#[derive(
    new,
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
    StarlarkAttrs,
)]
pub struct OpenProjectEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(OpenProjectEvent);

impl Event for OpenProjectEvent {
    const TYPE: EventType = EventType::OpenProject;
}

#[starlark_value(type = "OpenProjectEvent")]
impl<'v> StarlarkValue<'v> for OpenProjectEvent {
    starlark_attrs!();
}

impl Display for OpenProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(
    new,
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
    StarlarkAttrs,
)]
pub struct OpenFileEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(OpenFileEvent);

impl Event for OpenFileEvent {
    const TYPE: EventType = EventType::OpenFile;
}

#[starlark_value(type = "OpenFileEvent")]
impl<'v> StarlarkValue<'v> for OpenFileEvent {
    starlark_attrs!();
}

impl Display for OpenFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(
    new,
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
    StarlarkAttrs,
)]
pub struct MatchEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(MatchEvent);

impl Event for MatchEvent {
    const TYPE: EventType = EventType::Match;
}

#[starlark_value(type = "MatchEvent")]
impl<'v> StarlarkValue<'v> for MatchEvent {
    starlark_attrs!();
}

impl Display for MatchEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(
    new,
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
    StarlarkAttrs,
)]
pub struct CloseFileEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(CloseFileEvent);

impl Event for CloseFileEvent {
    const TYPE: EventType = EventType::CloseFile;
}

#[starlark_value(type = "CloseFileEvent")]
impl<'v> StarlarkValue<'v> for CloseFileEvent {
    starlark_attrs!();
}

impl Display for CloseFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(
    new,
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
    StarlarkAttrs,
)]
pub struct CloseProjectEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(CloseProjectEvent);

impl Event for CloseProjectEvent {
    const TYPE: EventType = EventType::CloseProject;
}

#[starlark_value(type = "CloseProjectEvent")]
impl<'v> StarlarkValue<'v> for CloseProjectEvent {
    starlark_attrs!();
}

impl Display for CloseProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[cfg(test)]
mod test {
    use indoc::{formatdoc, indoc};

    use crate::vextest::VexTest;

    fn test_event_common_properties(event_name: &'static str, type_name: &'static str) {
        VexTest::new("is-triggered")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                    load('check.star', 'check')

                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression) @bin_expr')
                        vex.observe('match', lambda x: x) # Make the error checker happy.
                        vex.observe('{event_name}', on_{event_name})

                    def on_{event_name}(event):
                        fail('error-marker')
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println("{x}");
                    }
                "#},
            )
            .returns_error("error-marker");
        VexTest::new("type-name")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                    load('check.star', 'check')

                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression) @bin_expr')
                        vex.observe('match', lambda x: x) # Make the error checker happy.
                        vex.observe('{event_name}', on_{event_name})

                    def on_{event_name}(event):
                        check['eq'](type(event), '{type_name}')
                "#},
            )
            .assert_irritation_free();
        VexTest::new("common-attrs")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                    load('check.star', 'check')

                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression) @bin_expr')
                        vex.observe('match', lambda x: x) # Make the error checker happy.
                        vex.observe('{event_name}', on_{event_name})

                    def on_{event_name}(event):
                        check['hasattr'](event, 'path')
                        if 'project' in '{event_name}':
                            check['is_path'](str(event.path))
                        else:
                            check['eq'](str(event.path), 'src/main.rs')
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println("{x}");
                    }
                "#},
            )
            .assert_irritation_free();
    }

    #[test]
    fn on_open_project_event() {
        test_event_common_properties("open_project", "OpenProjectEvent");
    }

    #[test]
    fn on_open_file_event() {
        test_event_common_properties("open_file", "OpenFileEvent");
    }

    #[test]
    fn on_match_event() {
        test_event_common_properties("match", "MatchEvent");
    }

    #[test]
    fn on_close_file_event() {
        test_event_common_properties("close_file", "CloseFileEvent");
    }

    #[test]
    fn on_close_project_event() {
        test_event_common_properties("close_project", "CloseProjectEvent");
    }
}
