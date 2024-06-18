use std::{cell::RefCell, fmt::Display, hash::Hasher, ops::Deref};

use allocative::Allocative;
use derive_more::Display;
use derive_new::new;
use dupe::Dupe;
use paste::paste;
use starlark::{
    collections::StarlarkHasher,
    environment::{Methods, MethodsBuilder, MethodsStatic},
    starlark_simple_value,
    values::{
        AllocValue, Demand, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Trace,
        UnpackValue, Value, ValueError,
    },
};
use starlark_derive::{starlark_attrs, starlark_module, starlark_value, StarlarkAttrs};
use tree_sitter::{Node as TSNode, Point, TreeCursor};

use crate::{error::Error, source_file::ParsedSourceFile};

#[derive(new, Clone, Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct Node<'v> {
    #[allocative(skip)]
    ts_node: TSNode<'v>,

    #[allocative(skip)]
    pub source_file: &'v ParsedSourceFile,
}

unsafe impl<'v> Trace<'v> for Node<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl<'v> Node<'v> {
    const KIND_ATTR_NAME: &'static str = "kind";
    const LOCATION_ATTR_NAME: &'static str = "location";

    #[inline]
    fn parent(&self) -> Option<Self> {
        self.ts_node
            .parent()
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[inline]
    fn next_sibling(&self) -> Option<Self> {
        self.ts_node
            .next_sibling()
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[inline]
    fn prev_sibling(&self) -> Option<Self> {
        self.ts_node
            .prev_sibling()
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[inline]
    fn children<'cursor>(
        &self,
        cursor: &'cursor mut TreeCursor<'v>,
    ) -> impl ExactSizeIterator<Item = Self> + 'cursor {
        self.ts_node
            .children(cursor)
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[inline]
    pub fn child_by_field_name(&self, field_name: impl AsRef<[u8]>) -> Option<Self> {
        self.ts_node
            .child_by_field_name(field_name)
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[inline]
    pub fn child(&self, i: usize) -> Option<Self> {
        self.ts_node
            .child(i)
            .map(|ts_node| Self::new(ts_node, self.source_file))
    }

    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn is_extra<'v>(this: Node<'v>) -> starlark::Result<bool> {
            Ok(this.is_extra())
        }

        fn is_named<'v>(this: Node<'v>) -> starlark::Result<bool> {
            Ok(this.is_named())
        }

        fn parent<'v>(this: Node<'v>) -> starlark::Result<Option<Node<'v>>> {
            Ok(this.parent())
        }

        fn parents<'v>(this: Node<'v>) -> starlark::Result<ParentsIterable<'v>> {
            Ok(ParentsIterable::new(this))
        }

        fn next_sibling<'v>(this: Node<'v>) -> starlark::Result<Option<Node<'v>>> {
            Ok(this.next_sibling())
        }

        fn next_siblings<'v>(this: Node<'v>) -> starlark::Result<NextSiblingsIterable<'v>> {
            Ok(NextSiblingsIterable::new(this))
        }

        fn previous_sibling<'v>(this: Node<'v>) -> starlark::Result<Option<Node<'v>>> {
            Ok(this.prev_sibling())
        }

        fn previous_siblings<'v>(this: Node<'v>) -> starlark::Result<PreviousSiblingsIterable<'v>> {
            Ok(PreviousSiblingsIterable::new(this))
        }

        fn children<'v>(this: Node<'v>) -> starlark::Result<Vec<Node<'v>>> {
            Ok(this.children(&mut this.walk()).collect())
        }

        fn text<'v>(this: Node<'v>) -> starlark::Result<&'v str> {
            this.utf8_text(this.source_file.content.as_bytes())
                .map_err(Error::Utf8)
                .map_err(starlark::Error::new_other)
        }
    }
}

impl<'v> Deref for Node<'v> {
    type Target = TSNode<'v>;

    fn deref(&self) -> &Self::Target {
        &self.ts_node
    }
}

impl Dupe for Node<'_> {
    // Cloning TSNode is cheap.
}

#[starlark_value(type = "Node")]
impl<'v> StarlarkValue<'v> for Node<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        let Some(other) = other.request_value::<&Self>() else {
            return Ok(false);
        };
        Ok(self == other)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        let ret = if let Some(field_name) = other.unpack_str() {
            self.child_by_field_name(field_name).is_some()
        } else if let Some(node) = other.request_value::<&Self>() {
            self.children(&mut self.walk()).any(|child| &child == node)
        } else {
            false
        };
        Ok(ret)
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(self.child_count() as i32)
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        if let Some(field_name) = index.unpack_str() {
            self.child_by_field_name(field_name.as_bytes())
                .map(|node| heap.alloc(node))
                .ok_or_else(|| ValueError::KeyNotFound(field_name.to_string()).into())
        } else if let Some(index) = index.unpack_i32() {
            let adjusted_index = if index < 0 {
                index + self.child_count() as i32
            } else {
                index
            };
            if adjusted_index < 0 {
                return Err(ValueError::IndexOutOfBound(index).into());
            }
            self.child(adjusted_index as usize)
                .map(|node| heap.alloc(node))
                .ok_or_else(|| ValueError::IndexOutOfBound(index).into())
        } else {
            ValueError::unsupported_with(self, "[]", index)
        }
    }

    fn write_hash(&self, hasher: &mut StarlarkHasher) -> starlark::Result<()> {
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
            Self::KIND_ATTR_NAME => Some(heap.alloc(heap.alloc_str(self.grammar_name()))),
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
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_sexp().fmt(f)
    }
}

macro_rules! walking_iterator {
    ($name:ident, $next:expr) => {
        paste! {
            #[derive(
                Clone, Debug, Display, Dupe, Allocative, NoSerialize, ProvidesStaticType, Trace,
            )]
            #[display(fmt = "" $name)]
            struct [<$name Iterable>]<'v> {
                current: Node<'v>,
            }

            impl<'v> [<$name Iterable>]<'v> {
                fn new(current: Node<'v>) -> Self {
                    Self { current }
                }
            }

            #[starlark_value(type = "" $name)]
            impl<'v> StarlarkValue<'v> for [<$name Iterable>]<'v> {
                unsafe fn iterate(&self, _: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
                    Ok(heap.alloc([<$name Iterator>] {
                        current: RefCell::new(self.current.dupe()),
                    }))
                }
            }

            impl<'v> AllocValue<'v> for [<$name Iterable>]<'v> {
                fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
                    heap.alloc_complex_no_freeze(self)
                }
            }

            #[derive(
                Clone, Debug, Display, Allocative, NoSerialize, ProvidesStaticType, Trace,
            )]
            #[display(fmt = "" $name)]
            struct [<$name Iterator>]<'v> {
                current: RefCell<Node<'v>>,
            }

            #[starlark_value(type = "" $name)]
            impl<'v> StarlarkValue<'v> for [<$name Iterator>]<'v> {
                unsafe fn iter_next(&self, _: usize, heap: &'v Heap) -> Option<Value<'v>> {
                    let next = $next(&self.current.borrow());
                    if let Some(next) = &next {
                        *self.current.borrow_mut() = next.dupe();
                    }
                    next.map(|node| heap.alloc(node))
                }

                unsafe fn iter_stop(&self) {}
            }

            impl<'v> AllocValue<'v> for [<$name Iterator>]<'v> {
                fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
                    heap.alloc_complex_no_freeze(self)
                }
            }
        }
    };
}
walking_iterator!(Parents, Node::parent);
walking_iterator!(NextSiblings, Node::next_sibling);
walking_iterator!(PreviousSiblings, Node::prev_sibling);

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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression left: (integer_literal) @l_int) @bin_expr',
                                on_match,
                            )

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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression left: (integer_literal) @l_int) @bin_expr',
                                on_match,
                            )

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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(binary_expression left: (integer_literal) @l_int) @bin_expr',
                                on_match,
                            )

                        def on_match(event):
                            expected_attrs = [
                                'children',
                                'is_extra',
                                'is_named',
                                'kind',
                                'location',
                                'next_sibling',
                                'next_siblings',
                                'parent',
                                'parents',
                                'previous_sibling',
                                'previous_siblings',
                                'text',
                            ]
                            check['attrs'](event.captures['bin_expr'], expected_attrs)
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                        right: (parenthesized_expression)
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

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
    fn is_extra() {
        VexTest::new("is_extra")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (
                                        (line_comment) @line_comment
                                        (call_expression) @call_expr
                                    )
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            line_comment = event.captures['line_comment']
                            call_expr = event.captures['call_expr']

                            check['true'](line_comment.is_extra())
                            check['false'](call_expr.is_extra())
                    ",
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {"
                    fn main() {
                        // line_comment
                        call_expr()
                    }
                "},
            )
            .assert_irritation_free();
    }

    #[test]
    fn is_named() {
        VexTest::new("is_named")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (
                                        (line_comment) @line_comment
                                        ("}}") @closing_brace
                                    )
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            line_comment = event.captures['line_comment']
                            closing_brace = event.captures['closing_brace']

                            check['true'](line_comment.is_named())
                            check['false'](closing_brace.is_named())
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {"
                    fn main() {
                        // line_comment
                    }
                "},
            )
            .assert_irritation_free();
    }

    #[test]
    fn tree_interaction() {
        VexTest::new("tree_interaction")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '(expression_statement (binary_expression)) @expr',
                                on_match,
                            )

                        def on_match(event):
                            expr = event.captures['expr']
                            check['true'](len(expr) > 1)

                            parent = expr.parent()
                            check['neq'](parent, None)
                            some_parent_is_none = False
                            for _ in range(25):
                                parent = parent.parent()
                                if parent == None:
                                    some_parent_is_none = True
                                    break
                            check['true'](some_parent_is_none)

                            bin_expr = expr[0]
                            check['eq'](bin_expr.kind, 'binary_expression')

                            check['not in']('non-existent-field', bin_expr)
                            check['not in'](True, bin_expr)
                            check['not in'](expr, bin_expr)
                            check['in'](bin_expr, expr)

                            check['in']('left', bin_expr)
                            check['eq'](bin_expr['left'].kind, 'integer_literal')
                            check['eq'](bin_expr['left'], bin_expr[0])
                            check['eq'](bin_expr['left'], bin_expr[-3])

                            check['in']('right', bin_expr)
                            check['eq'](bin_expr['right'].kind, 'char_literal')
                            check['eq'](bin_expr['right'], bin_expr[2])
                            check['eq'](bin_expr['right'], bin_expr[-1])

                            check['eq'](bin_expr[1].kind, '+')
                            check['eq'](bin_expr[1], bin_expr[-2])

                            line_comment = expr.previous_sibling()
                            check['eq'](line_comment.kind, 'line_comment')
                            check['eq'](line_comment.previous_sibling().kind, '{{')
                            check['eq'](line_comment.previous_sibling().previous_sibling(), None)
                            check['eq'](line_comment.next_sibling(), expr)

                            call_expr = expr.next_sibling()
                            check['eq'](call_expr.kind, 'call_expression')
                            check['eq'](call_expr.next_sibling().kind, '}}')
                            check['eq'](call_expr.next_sibling().next_sibling(), None)
                            check['eq'](call_expr.previous_sibling(), expr)

                            check['type'](expr.parents(), 'Parents')
                            curr = expr
                            for _ in range(len(list(expr.parents()))):
                                next_curr = curr.parent()
                                check['neq'](next_curr, None)
                                curr = next_curr
                            check['eq'](curr.parent(), None)

                            check['type'](expr.next_siblings(), 'NextSiblings')
                            curr = expr
                            for _ in range(len(list(expr.next_siblings()))):
                                next_curr = curr.next_sibling()
                                check['neq'](next_curr, None)
                                curr = next_curr
                            check['eq'](curr.next_sibling(), None)

                            check['type'](expr.previous_siblings(), 'PreviousSiblings')
                            curr = expr
                            for _ in range(len(list(expr.previous_siblings()))):
                                next_curr = curr.previous_sibling()
                                check['neq'](next_curr, None)
                                curr = next_curr
                            check['eq'](curr.previous_sibling(), None)

                            children = list(bin_expr.children())
                            check['eq'](len(bin_expr), len(children))
                            for i in range(len(children)):
                                check['in'](bin_expr[i], children)
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        // some comment
                        1 + 'a';
                        func()
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                        right: (parenthesized_expression)
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                        right: (parenthesized_expression)
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

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
