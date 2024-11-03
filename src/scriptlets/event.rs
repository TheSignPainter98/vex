use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
    fmt::Display,
    str::FromStr,
};

use allocative::Allocative;
use derive_new::new;
use dupe::{Dupe, OptionDupedExt};
use smallvec::{smallvec, SmallVec};
use starlark::{
    starlark_simple_value,
    values::{
        dict::AllocDict, AllocValue, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Trace,
        Value, ValueError,
    },
};
use starlark_derive::starlark_value;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::{
    error::Error, irritation::Irritation, result::Result, scriptlets::QueryCaptures,
    source_path::PrettyPath, suggestion::suggest,
};

const PATH_ATTR_NAME: &str = "path";
const NAME_ATTR_NAME: &str = "name";

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, Display, Allocative, Dupe)]
pub enum EventKind {
    OpenProject,
    OpenFile,
    Match,
    PreTestRun,
    PostTestRun,
}

impl EventKind {
    pub fn parseable(&self) -> bool {
        match self {
            Self::OpenProject | Self::OpenFile | Self::PreTestRun | Self::PostTestRun => true,
            Self::Match => false,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::OpenProject => "open_project",
            Self::OpenFile => "open_file",
            Self::Match => "match",
            Self::PreTestRun => "pre_test_run",
            Self::PostTestRun => "post_test_run",
        }
    }

    pub fn pretty_name(&self) -> &'static str {
        match self {
            Self::OpenProject => "opening project",
            Self::OpenFile => "opening file",
            Self::Match => "handling match",
            Self::PreTestRun => "setting up test run",
            Self::PostTestRun => "inspecting test run",
        }
    }
}

impl FromStr for EventKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "open_project" => Ok(Self::OpenProject),
            "open_file" => Ok(Self::OpenFile),
            "pre_test_run" => Ok(Self::PreTestRun),
            "post_test_run" => Ok(Self::PostTestRun),
            _ => Err(Error::UnknownEvent {
                name: s.to_owned(),
                suggestion: suggest(
                    s,
                    Self::iter()
                        .filter(Self::parseable)
                        .map(|event| event.name()),
                ),
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
pub struct PreTestRunEvent;

impl PreTestRunEvent {
    pub fn kind(&self) -> EventKind {
        EventKind::PreTestRun
    }
}

impl Display for PreTestRunEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[starlark_value(type = "PreTestRunEvent")]
impl<'v> StarlarkValue<'v> for PreTestRunEvent {
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

impl<'v> AllocValue<'v> for PreTestRunEvent {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_simple(self)
    }
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct PostTestRunEvent<'v> {
    irritations: Value<'v>,
}

impl<'v> PostTestRunEvent<'v> {
    const COLLATED_IRRITATIONS_ATTR_NAME: &'static str = "warnings";

    pub fn new(
        irritations_iter: impl IntoIterator<Item = (Irritation, bool)>,
        heap: &'v Heap,
    ) -> Self {
        let irritations = heap.alloc(IrritationsByFile::new(irritations_iter, heap));
        Self { irritations }
    }

    pub fn kind(&self) -> EventKind {
        EventKind::PostTestRun
    }
}

impl Display for PostTestRunEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[starlark_value(type = "PostTestRunEvent")]
impl<'v> StarlarkValue<'v> for PostTestRunEvent<'v> {
    fn dir_attr(&self) -> Vec<String> {
        [NAME_ATTR_NAME, Self::COLLATED_IRRITATIONS_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            NAME_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind().name()))),
            Self::COLLATED_IRRITATIONS_ATTR_NAME => Some(self.irritations),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [NAME_ATTR_NAME, Self::COLLATED_IRRITATIONS_ATTR_NAME].contains(&attr)
    }
}

impl<'v> AllocValue<'v> for PostTestRunEvent<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

#[derive(
    Clone, Debug, Allocative, ProvidesStaticType, NoSerialize, Trace, derive_more::Display,
)]
#[display(fmt = "{entries}")]
struct IrritationsByFile<'v> {
    entries: Value<'v>,
}

impl<'v> IrritationsByFile<'v> {
    fn new(irritations: impl IntoIterator<Item = (Irritation, bool)>, heap: &'v Heap) -> Self {
        let mut entry_map: BTreeMap<_, BTreeMap<_, SmallVec<[_; 2]>>> = BTreeMap::new();
        for (irritation, lenient) in irritations {
            let key = irritation
                .path()
                .map(|path| Cow::Owned(path.to_string()))
                .unwrap_or(Cow::Borrowed("no-file"));
            match entry_map.entry(key) {
                Entry::Occupied(mut entry) => {
                    match entry.get_mut().entry(irritation.vex_id().to_string()) {
                        Entry::Occupied(mut entry) => entry.get_mut().push((irritation, lenient)),
                        Entry::Vacant(entry) => {
                            entry.insert(smallvec![(irritation, lenient)]);
                        }
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(BTreeMap::from_iter([(
                        irritation.vex_id().to_string(),
                        smallvec![(irritation, lenient)],
                    )]));
                }
            }
        }
        let entries = heap.alloc(AllocDict(entry_map.into_iter().map(|(path, path_irrs)| {
            (path.to_string(), IrritationsById::new(path_irrs, heap))
        })));
        Self { entries }
    }
}

#[starlark_value(type = "WarningsByFile")]
impl<'v> StarlarkValue<'v> for IrritationsByFile<'v> {
    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        self.entries.at(index, heap)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        self.entries.is_in(other)
    }
}

impl<'v> AllocValue<'v> for IrritationsByFile<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

#[derive(
    Clone, Debug, Allocative, Dupe, NoSerialize, ProvidesStaticType, derive_more::Display, Trace,
)]
struct IrritationsById<'v> {
    entries: Value<'v>,
}

impl<'v> IrritationsById<'v> {
    fn new(
        iter: impl IntoIterator<Item = (String, SmallVec<[(Irritation, bool); 2]>)>,
        heap: &'v Heap,
    ) -> Self {
        let entries = heap.alloc(AllocDict(
            iter.into_iter()
                .map(|(id, irrs)| (id, Irritations::new(irrs, heap))),
        ));
        Self { entries }
    }
}

#[starlark_value(type = "WarningsById")]
impl<'v> StarlarkValue<'v> for IrritationsById<'v> {
    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        self.entries.at(index, heap)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        self.entries.is_in(other)
    }
}

impl<'v> AllocValue<'v> for IrritationsById<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

#[derive(Clone, Debug, Allocative, NoSerialize, ProvidesStaticType, Trace)]
struct Irritations<'v>(Vec<Value<'v>>);

impl<'v> Irritations<'v> {
    fn new(iter: impl IntoIterator<Item = (Irritation, bool)>, heap: &'v Heap) -> Self {
        Self(
            iter.into_iter()
                .map(|(irr, lenient)| irr.to_value_on(lenient, heap))
                .collect(),
        )
    }
}

impl Display for Irritations<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        self.0.iter().try_for_each(|v| write!(f, "{v}"))?;
        write!(f, "]")
    }
}

#[starlark_value(type = "Warnings")]
impl<'v> StarlarkValue<'v> for Irritations<'v> {
    fn at(&self, index: Value<'v>, _heap: &'v Heap) -> starlark::Result<Value<'v>> {
        index
            .dupe()
            .unpack_i32()
            .ok_or_else(|| ValueError::unsupported_with::<(), _>(self, "[]", index).unwrap_err()) // Wtf.
            .and_then(|index| {
                self.0
                    .get(index as usize)
                    .duped()
                    .ok_or(ValueError::IndexOutOfBound(index).into())
            })
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(self.0.len() as i32)
    }

    fn iterate_collect(&self, _heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(self.0.clone())
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        Ok(self.0.iter().any(|v| v == &other))
    }
}

impl<'v> AllocValue<'v> for Irritations<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
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
            .with_test_events(event_name.ends_with("_test_run"))
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
            .with_test_events(event_name.ends_with("_test_run"))
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
            .with_test_events(event_name.ends_with("_test_run"))
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

        let run = VexTest::new("many-matching-triggers-one-event")
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
        assert_eq!(1, run.irritations.len());
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
    fn on_pre_test_run_event() {
        test_event_common_properties("pre_test_run", "PreTestRunEvent", &["name"]);
    }

    #[test]
    fn on_post_test_run_event() {
        test_event_common_properties("post_test_run", "PostTestRunEvent", &["name", "warnings"]);
    }
}
