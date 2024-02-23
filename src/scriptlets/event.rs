use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    starlark_simple_value,
    values::{
        none::NoneType, AllocValue, Freeze, Heap, NoSerialize, ProvidesStaticType, StarlarkValue,
        Trace, Value,
    },
};
use starlark_derive::{starlark_attrs, starlark_value, StarlarkAttrs};
use strum::EnumIter;

use crate::{error::Error, scriptlets::QueryCaptures, source_path::PrettyPath};

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

#[derive(new, Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct MatchEvent<'v> {
    #[allocative(skip)]
    path: PrettyPath,

    #[allocative(skip)]
    query_captures: QueryCaptures<'v>,
}

impl MatchEvent<'_> {
    const PATH_ATTR_NAME: &'static str = "path";
    const QUERY_CAPTURES_ATTR_NAME: &'static str = "captures";
}

impl Event for MatchEvent<'_> {
    const TYPE: EventType = EventType::Match;
}

#[starlark_value(type = "MatchEvent")]
impl<'v> StarlarkValue<'v> for MatchEvent<'v> {
    fn dir_attr(&self) -> Vec<String> {
        [Self::PATH_ATTR_NAME, Self::QUERY_CAPTURES_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            Self::QUERY_CAPTURES_ATTR_NAME => Some(heap.alloc(self.query_captures.dupe())),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [Self::PATH_ATTR_NAME, Self::QUERY_CAPTURES_ATTR_NAME].contains(&attr)
    }
}

impl<'v> AllocValue<'v> for MatchEvent<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl<'v> Freeze for MatchEvent<'v> {
    type Frozen = NoneType;

    fn freeze(self, _freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        panic!("{} should never get frozen", <Self as StarlarkValue>::TYPE);
    }
}

impl Display for MatchEvent<'_> {
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
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression) @bin_expr')
                            if '{event_name}' != 'match':
                                vex.observe('match', lambda x: x) # Make the error checker happy.
                            vex.observe('{event_name}', on_{event_name})

                        def on_{event_name}(event):
                            fail('error-marker')
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                "#},
            )
            .returns_error("error-marker");
        VexTest::new("type-name")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression) @bin_expr')
                            if '{event_name}' != 'match':
                                vex.observe('match', lambda x: x) # Make the error checker happy.
                            vex.observe('{event_name}', on_{event_name})

                        def on_{event_name}(event):
                            check['type'](event, '{type_name}')
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free();
        VexTest::new("common-attrs")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression) @bin_expr')
                            if '{event_name}' != 'match':
                                vex.observe('match', lambda x: x) # Make the error checker happy.
                            vex.observe('{event_name}', on_{event_name})

                        def on_{event_name}(event):
                            attrs = dir(event)
                            for attr in attrs:
                                check['hasattr'](event, attr) # For consistency.
                            check['in']('path', attrs)
                            if 'project' in '{event_name}':
                                check['is_path'](str(event.path))
                            else:
                                check['in'](str(event.path), ['src/main.rs', 'src\\main.rs'])
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
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

        VexTest::new("captures")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression) @bin_expr')
                            vex.observe('match', on_match)

                        def on_match(event):
                            check['dir'](event, 'captures')
                            check['hasattr'](event, 'captures')

                            captures = event.captures
                            check['type'](captures, 'QueryCaptures')
                            check['eq'](len(captures), 1)

                            check['in']('bin_expr', captures)
                            bin_expr = captures['bin_expr']
                            check['type'](bin_expr, 'Node')
                            check['true'](str(bin_expr).startswith('(')) # Looks like an s-expression
                            check['true'](str(bin_expr).endswith(')'))   # Looks like an s-expression
                            check['eq'](str(bin_expr), repr(bin_expr))
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                "#},
            )
            .assert_irritation_free();
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
