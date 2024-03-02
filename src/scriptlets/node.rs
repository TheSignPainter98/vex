use std::{fmt::Display, hash::Hasher, ops::Deref};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    collections::StarlarkHasher,
    environment::{Methods, MethodsBuilder, MethodsStatic},
    starlark_simple_value,
    values::{
        none::NoneType, AllocValue, Demand, Freeze, Heap, NoSerialize, ProvidesStaticType,
        StarlarkValue, Trace, UnpackValue, Value,
    },
};
use starlark_derive::{starlark_attrs, starlark_module, starlark_value, StarlarkAttrs};
use tree_sitter::{Node as TSNode, Point};

use crate::{error::Error, source_file::SourceFile};

#[derive(new, Clone, Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative, Dupe)]
pub struct Node<'v> {
    #[allocative(skip)]
    ts_node: &'v TSNode<'v>,

    #[allocative(skip)]
    source_file: &'v SourceFile,
}

unsafe impl<'v> Trace<'v> for Node<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl Node<'_> {
    const KIND_ATTR_NAME: &'static str = "kind";
    const LOCATION_ATTR_NAME: &'static str = "location";

    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn text<'v>(this: Node<'v>) -> anyhow::Result<&'v str> {
            Ok(this.utf8_text(this.source_file.content.as_bytes())?)
        }
    }
}

impl<'v> Deref for Node<'v> {
    type Target = TSNode<'v>;

    fn deref(&self) -> &Self::Target {
        self.ts_node
    }
}

#[starlark_value(type = "Node")]
impl<'v> StarlarkValue<'v> for Node<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn equals(&self, other: Value<'v>) -> anyhow::Result<bool> {
        let Some(other) = other.request_value::<&Self>() else {
            return Ok(false);
        };
        Ok(self == other)
    }

    fn write_hash(&self, hasher: &mut StarlarkHasher) -> anyhow::Result<()> {
        hasher.write_usize(self.id());
        Ok(())
    }

    fn dir_attr(&self) -> Vec<String> {
        [Self::KIND_ATTR_NAME, Self::LOCATION_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::KIND_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.kind()))),
            Self::LOCATION_ATTR_NAME => Some(heap.alloc(Location::of(self))),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [Self::KIND_ATTR_NAME, Self::LOCATION_ATTR_NAME].contains(&attr)
    }

    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(Self::methods)
    }
}

impl<'v> UnpackValue<'v> for Node<'v> {
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        value.request_value::<&Node>().map(Dupe::dupe)
    }
}

impl<'v> AllocValue<'v> for Node<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for Node<'_> {
    type Frozen = NoneType;

    fn freeze(self, _freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        Err(Error::Unfreezable(Self::TYPE).into())
    }
}

impl Display for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_sexp().fmt(f)
    }
}

#[derive(
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Allocative,
    NoSerialize,
    ProvidesStaticType,
    StarlarkAttrs,
)]
struct Location {
    start_row: usize,
    start_column: usize,
    end_row: usize,
    end_column: usize,
}
starlark_simple_value!(Location);

impl Location {
    fn of(node: &Node<'_>) -> Self {
        let Point {
            row: start_row,
            column: start_column,
        } = node.start_position();
        let Point {
            row: end_row,
            column: end_column,
        } = node.end_position();
        Self {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }
}

#[starlark_value(type = "Location")]
impl<'v> StarlarkValue<'v> for Location {
    starlark_attrs!();
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            start_row,
            start_column,
            end_row,
            end_column,
        } = self;
        write!(
            f,
            "[{start_row}, {start_column}] - [{end_row}, {end_column}]"
        )
    }
}

#[cfg(test)]
mod test {
    use indoc::{formatdoc, indoc};

    use crate::vextest::VexTest;

    #[test]
    fn r#type() {
        VexTest::new("type")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression left: (integer_literal) @l_int) @bin_expr')
                            vex.observe('match', on_match)

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']
                            check['type'](bin_expr, 'Node')
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
    fn repr() {
        VexTest::new("repr")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression left: (integer_literal) @l_int) @bin_expr')
                            vex.observe('match', on_match)

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']

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
                        let x = 1 + (2 + 3);
                        println!("{x}");
                    }
                "#},
            )
            .assert_irritation_free();
    }

    #[test]
    fn attr_consistency() {
        VexTest::new("repr")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('(binary_expression left: (integer_literal) @l_int) @bin_expr')
                            vex.observe('match', on_match)

                        def on_match(event):
                            check['attrs'](event.captures['bin_expr'], ['kind', 'location', 'text'])
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
    fn kind() {
        VexTest::new("kind")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('''
                                (binary_expression
                                    left: (integer_literal) @l_int
                                    right: (parenthesized_expression)
                                ) @bin_expr
                            ''')
                            vex.observe('match', on_match)

                        def on_match(event):
                            captures = event.captures
                            check['eq'](captures['bin_expr'].kind, 'binary_expression')
                            check['eq'](captures['l_int'].kind, 'integer_literal')
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
    fn location() {
        VexTest::new("location")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('''
                                (binary_expression
                                    left: (integer_literal) @l_int
                                    right: (parenthesized_expression)
                                ) @bin_expr
                            ''')
                            vex.observe('match', on_match)

                        def on_match(event):
                            location = event.captures['bin_expr'].location

                            check['type'](location, 'Location')
                            check['eq'](str(location), '[1, 12] - [1, 23]')
                            check['eq'](str(location), repr(location))
                            check['eq'](location.start_row, 1)
                            check['eq'](location.start_column, 12)
                            check['eq'](location.end_row, 1)
                            check['eq'](location.end_column, 23)
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
    fn text() {
        VexTest::new("text")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.language('rust')
                            vex.query('''
                                (binary_expression
                                    left: (integer_literal) @l_int
                                    right: (parenthesized_expression)
                                ) @bin_expr
                            ''')
                            vex.observe('match', on_match)

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']
                            check['eq'](bin_expr.text(), '1 + (2 + 3)')
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
}
