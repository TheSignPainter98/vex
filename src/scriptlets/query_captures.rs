use std::fmt::Display;

use allocative::Allocative;
use dupe::Dupe;
use smallvec::SmallVec;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    values::{
        dict::{AllocDict, DictRef},
        list::AllocList,
        AllocValue, Demand, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Trace, Value,
    },
};
use starlark_derive::{starlark_module, starlark_value};
use tree_sitter::{CaptureQuantifier, Query, QueryMatch};

use crate::{scriptlets::node::Node, source_file::ParsedSourceFile};

#[derive(Clone, Debug, Dupe, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct QueryCaptures<'v> {
    captures: Value<'v>, // This is a dict.
}

impl<'v> QueryCaptures<'v> {
    pub fn new(
        query: &Query,
        qmatch: QueryMatch<'v, '_>,
        source_file: &'v ParsedSourceFile,
        heap: &'v Heap,
    ) -> Self {
        let names = query.capture_names();
        let quantifiers = query.capture_quantifiers(qmatch.pattern_index);

        let mut captures: SmallVec<[_; 10]> = names
            .iter()
            .zip(quantifiers)
            .map(|(name, quantifier)| (name, Capture::new(*quantifier)))
            .collect();
        qmatch.captures.iter().for_each(|r#match| {
            let (_, ref mut capture) = captures[r#match.index as usize];
            capture.push(Node::new(&r#match.node, source_file))
        });
        captures.sort_by(|cap1, cap2| cap1.0.cmp(cap2.0));

        let captures = heap.alloc(AllocDict(
            captures
                .into_iter()
                .map(|(name, capture)| (name, capture.into_value_on(heap))),
        ));
        Self { captures }
    }
}

impl<'v> QueryCaptures<'v> {
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn keys<'v>(this: Value<'v>) -> starlark::Result<Vec<Value<'v>>> {
            let this = this
                .request_value::<&QueryCaptures<'_>>()
                .expect("internal error: incorrect receiver");
            Ok(DictRef::from_value(this.captures.dupe())
                .expect("internal error: captures not a dict")
                .keys()
                .collect())
        }

        fn values<'v>(this: Value<'v>) -> starlark::Result<Vec<Value<'v>>> {
            let this = this
                .request_value::<&QueryCaptures<'_>>()
                .expect("internal error: incorrect receiver");
            Ok(DictRef::from_value(this.captures.dupe())
                .expect("internal error: captures not a dict")
                .values()
                .collect())
        }

        fn items<'v>(this: Value<'v>, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
            let this = this
                .request_value::<&QueryCaptures<'_>>()
                .expect("internal error: incorrect receiver");
            Ok(DictRef::from_value(this.captures.dupe())
                .expect("internal error: captures not a dict")
                .iter()
                .map(|kv| heap.alloc(kv))
                .collect())
        }
    }
}

#[starlark_value(type = "QueryCaptures")]
impl<'v> StarlarkValue<'v> for QueryCaptures<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn length(&self) -> starlark::Result<i32> {
        self.captures.length()
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        self.captures.is_in(other)
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        self.captures.at(index, heap)
    }

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(self
            .captures
            .iterate(heap)
            .expect("internal error: captures not iterable")
            .collect())
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

#[derive(Clone, Debug, Allocative)]
struct Capture<'v> {
    #[allocative(skip)]
    quantifier: CaptureQuantifier,
    #[allocative(skip)]
    matches: SmallVec<[Node<'v>; Capture::CHUNK_SIZE]>,
}

impl<'v> Capture<'v> {
    const CHUNK_SIZE: usize = 10;

    fn new(quantifier: CaptureQuantifier) -> Self {
        let capacity = match quantifier {
            CaptureQuantifier::Zero => 0,
            CaptureQuantifier::ZeroOrOne | CaptureQuantifier::One => 1,
            CaptureQuantifier::ZeroOrMore | CaptureQuantifier::OneOrMore => Self::CHUNK_SIZE,
        };
        let matches = SmallVec::with_capacity(capacity);
        Self {
            quantifier,
            matches,
        }
    }

    fn push(&mut self, node: Node<'v>) {
        match (self.quantifier, self.matches.len()) {
            (CaptureQuantifier::Zero, 0) => {
                panic!("internal error: zero-quantified capture yielded")
            }
            (CaptureQuantifier::One, 1) | (CaptureQuantifier::ZeroOrOne, 1) => {
                panic!("internal error: one-max quantified capture yielded more than once")
            }
            _ => {}
        }

        self.matches.push(node);
    }

    fn into_value_on(self, heap: &'v Heap) -> Value<'v> {
        match self.quantifier {
            CaptureQuantifier::Zero => Value::new_none(),
            CaptureQuantifier::One => {
                let first = self
                    .matches
                    .into_iter()
                    .next()
                    .expect("internal error: one-quantified capture never matched");
                heap.alloc(first)
            }
            CaptureQuantifier::ZeroOrOne => self
                .matches
                .iter()
                .next()
                .map(|n| heap.alloc(n.dupe()))
                .unwrap_or_else(Value::new_none),
            CaptureQuantifier::ZeroOrMore => {
                heap.alloc(AllocList(self.matches.into_iter().map(|n| heap.alloc(n))))
            }
            CaptureQuantifier::OneOrMore => {
                assert!(
                    !self.matches.is_empty(),
                    "internal error: one-or-more quantified capture never matched"
                );
                heap.alloc(AllocList(self.matches.into_iter().map(|n| heap.alloc(n))))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use camino::Utf8Path;
    use indoc::{formatdoc, indoc};
    use starlark::values::Heap;
    use tree_sitter::{Parser, Query, QueryCursor};

    use crate::{
        scriptlets::QueryCaptures, source_file::ParsedSourceFile, source_path::SourcePath,
        supported_language::SupportedLanguage, vextest::VexTest,
    };

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
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                    (unary_expression) @unary
                                    (line_comment) @line_comment
                                    (block_comment) @block_comment
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            captures = event.captures

                            check['type'](captures.keys(), "list")

                            for key in captures.keys():
                                check['in'](key, captures)
                            check['sorted'](list(captures.keys()))
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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @l_int
                                    ) @bin_expr
                                    (unary_expression) @unary
                                    (line_comment) @line_comment
                                    (block_comment) @block_comment
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            captures = event.captures

                            check['type'](captures.values(), "list")

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
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (integer_literal) @a
                                        right: (parenthesized_expression
                                            (binary_expression
                                                left: (integer_literal) @b
                                                right: (integer_literal) @c
                                            ) @d
                                        ) @e
                                    ) @all
                                    (unary_expression) @unary
                                    (line_comment) @line_comment
                                    (block_comment) @block_comment
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            captures = event.captures

                            check['type'](captures.items(), "list")

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

    #[test]
    fn quantifiers() {
        let src_path = SourcePath::new_in(Utf8Path::new("main.rs"), Utf8Path::new("./"));
        let content = indoc! {r#"
            fn main() {
                // some
                // comment
                let x = "hello";
                func(x);
            }
        "#};
        let src_file =
            ParsedSourceFile::new_with_content(src_path, content, SupportedLanguage::Rust).unwrap();

        let language = tree_sitter_rust::language();
        let query_source = indoc! {r"
            (binary_expression) @absent ; zero-quantified
            (
                (block_comment)* @optional_block_comments    ; zero-or-more (absent)
                (line_comment)+ @mandatory_line_comments     ; one-or-more
                (string_literal)? @optional_absent_str       ; zero-or-one (absent)
                (let_declaration)? @optional_present_let     ; zero-or-one (present)
                (expression_statement) @mandatory_expression ; one
            )
        "};
        let query = Query::new(language, query_source).unwrap();
        let tree = {
            let mut parser = Parser::new();
            parser.set_language(language).unwrap();
            let tree = parser.parse(content, None).unwrap();
            assert!(!tree.root_node().has_error());
            tree
        };
        let mut cursor = QueryCursor::new();
        let qmatch = cursor
            .matches(&query, tree.root_node(), content.as_bytes())
            .next()
            .unwrap();
        let heap = Heap::new();
        let captures = heap.alloc(QueryCaptures::new(&query, qmatch, &src_file, &heap));

        enum Expectatation {
            AttrType(&'static str),
            NoSuchAttr,
        }
        use Expectatation::*;
        let property_types = [
            ("absent", AttrType("NoneType")),
            ("optional_block_comments", AttrType("list")),
            ("mandatory_line_comments", AttrType("list")),
            ("optional_absent_str", AttrType("NoneType")),
            ("optional_present_let", AttrType("Node")),
            ("mandatory_expression", AttrType("Node")),
            ("no_such_attr", NoSuchAttr),
        ];
        for (property, expected) in property_types {
            let capture = captures.at(heap.alloc(property), &heap);
            match expected {
                AttrType(typ) => assert_eq!(
                    capture.unwrap().get_type(),
                    typ,
                    "wrong type for {property}"
                ),
                NoSuchAttr => assert!(capture.is_err(), "expected error for {property}"),
            }
        }

        assert_eq!(
            captures
                .at(heap.alloc("optional_block_comments"), &heap)
                .unwrap()
                .length()
                .unwrap(),
            0
        );

        let line_comments = captures
            .at(heap.alloc("mandatory_line_comments"), &heap)
            .unwrap();
        assert_eq!(line_comments.length().unwrap(), 2);
        assert!(line_comments
            .iterate(&heap)
            .unwrap()
            .all(|elem| elem.get_type() == "Node"));
    }
}
