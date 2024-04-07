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
use starlark_derive::{starlark_value, StarlarkAttrs};
use strum::{EnumIter, IntoEnumIterator};

use crate::{error::Error, scriptlets::QueryCaptures, source_path::PrettyPath, trigger::TriggerId};

const PATH_ATTR_NAME: &str = "path";
const NAME_ATTR_NAME: &str = "name";
const TRIGGER_ID_NAME: &str = "trigger_id";

pub trait Event {
    const TYPE: EventType;
}

#[derive(Copy, Clone, Debug, EnumIter, PartialEq, Eq, Allocative)]
pub enum EventType {
    OpenProject,
    OpenFile,
    QueryMatch,
    CloseFile,
    CloseProject,
}

impl EventType {
    fn name(&self) -> &'static str {
        match self {
            EventType::OpenProject => "open_project",
            EventType::OpenFile => "open_file",
            EventType::QueryMatch => "query_match",
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
            "query_match" => Ok(EventType::QueryMatch),
            "close_file" => Ok(EventType::CloseFile),
            "close_project" => Ok(EventType::CloseProject),
            _ => Err(Error::UnknownEvent {
                name: s.to_owned(),
                suggestion: {
                    let (event, distance) = EventType::iter()
                        .map(|event| (event, strsim::damerau_levenshtein(s, event.name())))
                        .min_by_key(|(_, distance)| *distance)
                        .unwrap();
                    if distance <= 2 {
                        Some(event.name())
                    } else {
                        None
                    }
                },
            }
            .into()),
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
    path: PrettyPath,
}
starlark_simple_value!(OpenProjectEvent);

impl Event for OpenProjectEvent {
    const TYPE: EventType = EventType::OpenProject;
}

#[starlark_value(type = "OpenProjectEvent")]
impl<'v> StarlarkValue<'v> for OpenProjectEvent {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, PATH_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(<Self as Event>::TYPE.name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [NAME_ATTR_NAME, PATH_ATTR_NAME].contains(&attr)
    }
}

impl Display for OpenProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct OpenFileEvent {
    #[allocative(skip)]
    path: PrettyPath,

    trigger_id: Option<TriggerId>,
}
starlark_simple_value!(OpenFileEvent);

impl Event for OpenFileEvent {
    const TYPE: EventType = EventType::OpenFile;
}

#[starlark_value(type = "OpenFileEvent")]
impl<'v> StarlarkValue<'v> for OpenFileEvent {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, PATH_ATTR_NAME, TRIGGER_ID_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(<Self as Event>::TYPE.name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            TRIGGER_ID_NAME => {
                Some(heap.alloc(self.trigger_id.as_ref().map(|id| id.as_str().to_owned())))
            }
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [NAME_ATTR_NAME, PATH_ATTR_NAME, TRIGGER_ID_NAME].contains(&attr)
    }
}

impl Display for OpenFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct QueryMatchEvent<'v> {
    #[allocative(skip)]
    path: PrettyPath,

    #[allocative(skip)]
    query_captures: QueryCaptures<'v>,

    trigger_id: Option<TriggerId>,
}

impl QueryMatchEvent<'_> {
    const QUERY_CAPTURES_ATTR_NAME: &'static str = "captures";
}

impl Event for QueryMatchEvent<'_> {
    const TYPE: EventType = EventType::QueryMatch;
}

#[starlark_value(type = "QueryMatchEvent")]
impl<'v> StarlarkValue<'v> for QueryMatchEvent<'v> {
    fn dir_attr(&self) -> Vec<String> {
        [
            NAME_ATTR_NAME,
            PATH_ATTR_NAME,
            Self::QUERY_CAPTURES_ATTR_NAME,
            TRIGGER_ID_NAME,
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(<Self as Event>::TYPE.name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            Self::QUERY_CAPTURES_ATTR_NAME => Some(heap.alloc(self.query_captures.dupe())),
            TRIGGER_ID_NAME => {
                Some(heap.alloc(self.trigger_id.as_ref().map(|id| id.as_str().to_owned())))
            }
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [
            NAME_ATTR_NAME,
            PATH_ATTR_NAME,
            Self::QUERY_CAPTURES_ATTR_NAME,
            TRIGGER_ID_NAME,
        ]
        .contains(&attr)
    }
}

impl<'v> AllocValue<'v> for QueryMatchEvent<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl<'v> Freeze for QueryMatchEvent<'v> {
    type Frozen = NoneType;

    fn freeze(self, _freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        Err(Error::Unfreezable(<Self as StarlarkValue>::TYPE).into())
    }
}

impl Display for QueryMatchEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct CloseFileEvent {
    #[allocative(skip)]
    path: PrettyPath,

    trigger_id: Option<TriggerId>,
}
starlark_simple_value!(CloseFileEvent);

impl Event for CloseFileEvent {
    const TYPE: EventType = EventType::CloseFile;
}

#[starlark_value(type = "CloseFileEvent")]
impl<'v> StarlarkValue<'v> for CloseFileEvent {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, PATH_ATTR_NAME, TRIGGER_ID_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(<Self as Event>::TYPE.name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            TRIGGER_ID_NAME => {
                Some(heap.alloc(self.trigger_id.as_ref().map(|id| id.as_str().to_owned())))
            }
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [NAME_ATTR_NAME, PATH_ATTR_NAME, TRIGGER_ID_NAME].contains(&attr)
    }
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
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, PATH_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(<Self as Event>::TYPE.name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [NAME_ATTR_NAME, PATH_ATTR_NAME].contains(&attr)
    }
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

    fn test_event_common_properties(
        event_name: &'static str,
        type_name: &'static str,
        attrs: &'static [&'static str],
    ) {
        VexTest::new("is-triggered")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression) @bin_expr',
                            )
                            if '{event_name}' != 'query_match':
                                vex.observe('query_match', lambda x: x) # Make the error checker happy.
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
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression) @bin_expr',
                            )
                            if '{event_name}' != 'query_match':
                                vex.observe('query_match', lambda x: x) # Make the error checker happy.
                            vex.observe('{event_name}', on_{event_name})

                        def on_{event_name}(event):
                            check['type'](event, '{type_name}')
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free();
        VexTest::new("attrs")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression) @bin_expr',
                            )
                            if '{event_name}' != 'query_match':
                                vex.observe('query_match', lambda x: x) # Make the error checker happy.
                            vex.observe('{event_name}', on_{event_name})

                        def on_{event_name}(event):
                            check['attrs'](event, ['{attrs_repr}'])
                            check['eq'](event.name, '{event_name}')

                            if 'project' in '{event_name}':
                                check['is_path'](str(event.path))
                            else:
                                check['in'](str(event.path), ['src/main.rs', 'src\\main.rs'])
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                    attrs_repr = attrs.join("', '"),
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
        test_event_common_properties("open_project", "OpenProjectEvent", &["name", "path"]);
    }

    #[test]
    fn on_open_file_event() {
        test_event_common_properties(
            "open_file",
            "OpenFileEvent",
            &["name", "path", "trigger_id"],
        );
    }

    #[test]
    fn on_match_event() {
        test_event_common_properties(
            "query_match",
            "QueryMatchEvent",
            &["name", "captures", "path", "trigger_id"],
        );

        VexTest::new("captures")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='(binary_expression left: (integer_literal) @l_int) @bin_expr',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures

                            expected_fields = ['l_int', 'bin_expr']
                            for expected_field in expected_fields:
                                check['in'](expected_field, captures)
                            for field in captures:
                                check['in'](field, expected_fields)
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + (2 + 3);
                        println!("{x}");
                    }
                "#},
            )
            .assert_irritation_free();
    }

    #[test]
    fn on_close_file_event() {
        test_event_common_properties(
            "close_file",
            "CloseFileEvent",
            &["name", "path", "trigger_id"],
        );
    }

    #[test]
    fn on_close_project_event() {
        test_event_common_properties("close_project", "CloseProjectEvent", &["name", "path"]);
    }
}
