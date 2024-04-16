use std::fmt::Display;

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    values::{
        string::StarlarkStr, AllocValue, Demand, Heap, NoSerialize, ProvidesStaticType,
        StarlarkValue, Trace, Value, ValueError,
    },
};
use starlark_derive::{starlark_module, starlark_value};
use tree_sitter::{Query, QueryMatch as TSQueryMatch};

use crate::{scriptlets::node::Node, source_file::ParsedSourceFile};

#[derive(new, Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Dupe)]
pub struct QueryCaptures<'v> {
    #[allocative(skip)]
    query: &'v Query,

    #[allocative(skip)]
    pub query_match: &'v TSQueryMatch<'v, 'v>,

    #[allocative(skip)]
    source_file: &'v ParsedSourceFile<'v>,
}

impl QueryCaptures<'_> {
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn keys<'v>(this: Value<'v>) -> starlark::Result<QueryCapturesKeys<'v>> {
            let this = this
                .request_value::<&'v QueryCaptures>()
                .expect("receiver has wrong type");
            Ok(QueryCapturesKeys(this.dupe()))
        }

        fn values<'v>(this: Value<'v>) -> starlark::Result<QueryCapturesValues<'v>> {
            let this = this
                .request_value::<&QueryCaptures>()
                .expect("receiver has wrong type");
            Ok(QueryCapturesValues(this.dupe()))
        }

        fn items<'v>(this: Value<'v>) -> starlark::Result<QueryCapturesItems<'v>> {
            let this = this
                .request_value::<&QueryCaptures>()
                .expect("receiver has wrong type");
            Ok(QueryCapturesItems(this.dupe()))
        }
    }
}

unsafe impl<'v> Trace<'v> for QueryCaptures<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

#[starlark_value(type = "QueryCaptures")]
impl<'v> StarlarkValue<'v> for QueryCaptures<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(self.query.capture_names().len() as i32)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        let Some(name) = other.unpack_starlark_str() else {
            return Ok(false);
        };
        Ok(self.query.capture_index_for_name(name).is_some())
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        let Some(name) = index.unpack_starlark_str().map(StarlarkStr::as_str) else {
            return ValueError::unsupported_with(self, "[]", index);
        };
        let Some(index) = self.query.capture_index_for_name(name) else {
            return Err(ValueError::KeyNotFound(name.into()).into());
        };
        let capture = self
            .query_match
            .captures
            .iter()
            .find(|c| c.index == index)
            .unwrap();
        let node = Node::new(&capture.node, self.source_file);
        Ok(heap.alloc(node))
    }

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(QueryCapturesKeys::iterate_collect_names(
            self.query.capture_names(),
            heap,
        ))
    }

    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(Self::methods)
    }
}

impl<'v> AllocValue<'v> for QueryCaptures<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for QueryCaptures<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
struct QueryCapturesKeys<'v>(QueryCaptures<'v>);

impl<'v> QueryCapturesKeys<'v> {
    fn iterate_collect_names(capture_names: &'v [String], heap: &'v Heap) -> Vec<Value<'v>> {
        capture_names
            .iter()
            .map(|s| heap.alloc_str(s))
            .map(|s| heap.alloc(s))
            .collect()
    }
}

#[starlark_value(type = "QueryCapturesKeys")]
impl<'v> StarlarkValue<'v> for QueryCapturesKeys<'v> {
    type Canonical = Self;

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(Self::iterate_collect_names(
            self.0.query.capture_names(),
            heap,
        ))
    }
}

impl<'v> AllocValue<'v> for QueryCapturesKeys<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for QueryCapturesKeys<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.keys()", QueryCaptures::TYPE)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
struct QueryCapturesValues<'v>(QueryCaptures<'v>);

#[starlark_value(type = "QueryCapturesValues")]
impl<'v> StarlarkValue<'v> for QueryCapturesValues<'v> {
    type Canonical = Self;

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        let mut values = self
            .0
            .query_match
            .captures
            .iter()
            .map(|c| (c.index, Node::new(&c.node, self.0.source_file)))
            .collect::<Vec<_>>();
        values.sort_by_key(|(index, _)| *index);
        Ok(values
            .into_iter()
            .map(|(_, node)| heap.alloc(node))
            .collect())
    }
}

impl<'v> AllocValue<'v> for QueryCapturesValues<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for QueryCapturesValues<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.values()", QueryCaptures::TYPE)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
struct QueryCapturesItems<'v>(QueryCaptures<'v>);

#[starlark_value(type = "QueryCapturesItems")]
impl<'v> StarlarkValue<'v> for QueryCapturesItems<'v> {
    type Canonical = Self;

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        let values = {
            let mut captures = self
                .0
                .query_match
                .captures
                .iter()
                .map(|c| (c.index, &c.node))
                .collect::<Vec<_>>();
            captures.sort_by_key(|(index, _)| *index);
            captures
                .into_iter()
                .map(|(_, node)| Node::new(node, self.0.source_file))
        };
        Ok(self
            .0
            .query
            .capture_names()
            .iter()
            .map(|s| heap.alloc(heap.alloc_str(s)))
            .zip(values)
            .map(|p| heap.alloc(p))
            .collect())
    }
}

impl<'v> AllocValue<'v> for QueryCapturesItems<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

impl Display for QueryCapturesItems<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.items()", QueryCaptures::TYPE)
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
                            vex.add_trigger(
                                language = 'rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures
                            check['type'](captures, 'QueryCaptures')
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
    fn len() {
        VexTest::new("len")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                '''
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures
                            check['eq'](len(captures), 2)
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
    fn r#in() {
        VexTest::new("in")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures
                            check['in']('bin_expr', captures)
                            check['in']('l_int', captures)
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
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures
                            check['eq'](str(captures), "QueryCaptures")
                            check['eq'](str(captures), repr(captures))
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
    fn iter() {
        VexTest::new("iter")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures

                            expected_keys = sorted(['bin_expr', 'l_int'])
                            actual_keys = sorted(captures)
                            check['eq'](actual_keys, expected_keys)
                            for key in captures:
                                check['in'](key, captures)
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
    fn keys() {
        VexTest::new("keys")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures

                            check['type'](captures.keys(), "QueryCapturesKeys")
                            check['eq'](str(captures.keys()), "QueryCaptures.keys()")
                            for key in captures.keys():
                                check['in'](key, captures)
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
    fn values() {
        VexTest::new("values")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures

                            check['type'](captures.values(), "QueryCapturesValues")
                            check['eq'](str(captures.values()), "QueryCaptures.values()")
                            values = [captures[k] for k in captures.keys()]
                            for value in captures.values():
                                check['in'](value, values)
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
    fn items() {
        VexTest::new("items")
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.add_trigger(
                                language='rust',
                                query='''
                                    (binary_expression
                                        left: (integer_literal) @a
                                        right: (parenthesized_expression
                                            (binary_expression
                                                left: (integer_literal) @b
                                                right: (integer_literal) @c
                                            ) @d
                                        ) @e
                                    ) @all
                                ''',
                            )
                            vex.observe('query_match', on_query_match)

                        def on_query_match(event):
                            captures = event.captures

                            check['type'](captures.items(), "QueryCapturesItems")
                            check['eq'](str(captures.items()), "QueryCaptures.items()")

                            # All valid
                            for k,v in captures.items():
                                check['eq'](captures[k], v)

                            # Iterator orders are consistent
                            expected_items = zip(captures.keys(), captures.values())
                            actual_items = list(captures.items())
                            check['eq'](actual_items, expected_items)
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
