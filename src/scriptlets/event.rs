use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    starlark_simple_value,
    values::{AllocValue, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Trace, Value},
};
use starlark_derive::starlark_value;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::{
    error::Error, result::Result, scriptlets::QueryCaptures, source_path::PrettyPath,
    suggestion::suggest,
};

const PATH_ATTR_NAME: &str = "path";
const NAME_ATTR_NAME: &str = "name";

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, Display, Allocative, Dupe)]
pub enum EventKind {
    OpenProject,
    OpenFile,
    Match,
    Test,
}

impl EventKind {
    pub fn parseable(&self) -> bool {
        match self {
            Self::OpenProject | Self::OpenFile | Self::Test => true,
            Self::Match => false,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::OpenProject => "open_project",
            Self::OpenFile => "open_file",
            Self::Match => "match",
            Self::Test => "test",
        }
    }

    pub fn pretty_name(&self) -> &'static str {
        match self {
            Self::OpenProject => "opening project",
            Self::OpenFile => "opening file",
            Self::Match => "handling match",
            Self::Test => "testing",
        }
    }
}

impl FromStr for EventKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "open_project" => Ok(Self::OpenProject),
            "open_file" => Ok(Self::OpenFile),
            "test" => Ok(Self::Test),
            _ => Err(Error::UnknownEvent {
                name: s.to_owned(),
                suggestion: {
                    suggest(
                        s,
                        Self::iter()
                            .filter(Self::parseable)
                            .map(|event| event.name()),
                    )
                },
            }),
        }
    }
}

#[derive(new, Clone, Debug, Dupe, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct OpenProjectEvent {
    #[allocative(skip)]
    path: PrettyPath,
}
starlark_simple_value!(OpenProjectEvent);

impl OpenProjectEvent {
    pub fn kind(&self) -> EventKind {
        EventKind::OpenProject
    }
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
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind().name()))),
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
}
starlark_simple_value!(OpenFileEvent);

impl OpenFileEvent {
    pub fn kind(&self) -> EventKind {
        EventKind::OpenFile
    }
}

#[starlark_value(type = "OpenFileEvent")]
impl<'v> StarlarkValue<'v> for OpenFileEvent {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, PATH_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind().name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        [NAME_ATTR_NAME, PATH_ATTR_NAME].contains(&attr)
    }
}

impl Display for OpenFileEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Dupe, Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct MatchEvent<'v> {
    #[allocative(skip)]
    path: PrettyPath,

    #[allocative(skip)]
    query_captures: QueryCaptures<'v>,
}

impl MatchEvent<'_> {
    const QUERY_CAPTURES_ATTR_NAME: &'static str = "captures";

    pub fn kind(&self) -> EventKind {
        EventKind::Match
    }
}

#[starlark_value(type = "MatchEvent")]
impl<'v> StarlarkValue<'v> for MatchEvent<'v> {
    fn dir_attr(&self) -> Vec<String> {
        [
            NAME_ATTR_NAME,
            PATH_ATTR_NAME,
            Self::QUERY_CAPTURES_ATTR_NAME,
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind().name()))),
            PATH_ATTR_NAME => Some(heap.alloc(self.path.dupe())),
            Self::QUERY_CAPTURES_ATTR_NAME => Some(heap.alloc(self.query_captures.dupe())),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [
            NAME_ATTR_NAME,
            PATH_ATTR_NAME,
            Self::QUERY_CAPTURES_ATTR_NAME,
        ]
        .contains(&attr)
    }
}

impl<'v> AllocValue<'v> for MatchEvent<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for MatchEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(new, Clone, Dupe, Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct TestEvent;

impl TestEvent {
    pub fn kind(&self) -> EventKind {
        EventKind::Test
    }
}

impl Display for TestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[starlark_value(type = "TestEvent")]
impl<'v> StarlarkValue<'v> for TestEvent {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME].into_iter().map(Into::into).collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind().name()))),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [NAME_ATTR_NAME].contains(&attr)
    }
}

impl<'v> AllocValue<'v> for TestEvent {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_simple(self)
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
            .with_test_event(event_name == "test")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            if '{event_name}' == 'match':
                                vex.observe('open_project', on_open_project)
                            else:
                                vex.observe('{event_name}', on_{event_name})

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression) @bin_expr',
                                on_{event_name},
                            )

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
            .with_test_event(event_name == "test")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            if '{event_name}' == 'match':
                                vex.observe('open_project', on_open_project)
                            else:
                                vex.observe('{event_name}', on_{event_name})

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression) @bin_expr',
                                on_{event_name},
                            )

                        def on_{event_name}(event):
                            check['type'](event, '{type_name}')
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free();
        VexTest::new("attrs")
            .with_test_event(event_name == "test")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            if '{event_name}' == 'match':
                                vex.observe('open_project', on_open_project)
                            else:
                                vex.observe('{event_name}', on_{event_name})

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression) @bin_expr',
                                on_{event_name},
                            )

                        def on_{event_name}(event):
                            check['attrs'](event, ['{attrs_repr}'])
                            check['eq'](event.name, '{event_name}')

                            if 'path' in ['{attrs_repr}']:
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
        test_event_common_properties("open_file", "OpenFileEvent", &["name", "path"]);

        let run_data = VexTest::new("many-matching-triggers-one-event")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_file', on_open_file)

                    def on_open_file(event):
                        if 'main.rs' not in event.path:
                            return

                        vex.warn("test", "opened file %s" % event.path)
                "#},
            )
            .with_source_file("src/main.rs", r#"fn main() { println!("hello, world!"); }"#)
            .with_source_file("src/unused.rs", r#"fn other() { }"#)
            .try_run()
            .unwrap();
        assert_eq!(1, run_data.irritations.len());
    }

    #[test]
    fn on_match_event() {
        test_event_common_properties("match", "MatchEvent", &["name", "captures", "path"]);

        VexTest::new("captures")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression left: (integer_literal) @l_int) @bin_expr',
                                on_match,
                            )

                        def on_match(event):
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
    fn on_test_event() {
        test_event_common_properties("test", "TestEvent", &["name"]);
    }
}
