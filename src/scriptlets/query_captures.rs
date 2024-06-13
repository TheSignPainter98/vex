use std::{fmt::Display, ops::Deref, rc::Rc};

use allocative::Allocative;
use dupe::Dupe;
use smallvec::SmallVec;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    values::{
        list::AllocList, AllocValue, Demand, Heap, NoSerialize, ProvidesStaticType, StarlarkValue,
        Trace, Value, ValueError,
    },
};
use starlark_derive::{starlark_module, starlark_value};
use tree_sitter::{CaptureQuantifier, Query, QueryMatch};

use crate::{scriptlets::node::Node, source_file::ParsedSourceFile};

#[derive(Clone, Debug, Dupe, ProvidesStaticType, NoSerialize, Allocative)]
pub struct QueryCaptures<'v> {
    captures: Rc<Vec<Capture<'v>>>,
}

impl<'v> QueryCaptures<'v> {
    pub fn new(
        query: &Query,
        qmatch: QueryMatch<'v, '_>,
        source_file: &'v ParsedSourceFile,
    ) -> Self {
        let names = query.capture_names();
        let quantifiers = query.capture_quantifiers(qmatch.pattern_index);

        let mut captures: Vec<Capture<'v>> = names
            .iter()
            .zip(quantifiers)
            .map(|(name, quantifier)| Capture::new(name.clone(), *quantifier))
            .collect();
        qmatch.captures.iter().for_each(|capture| {
            captures[capture.index as usize].push(Node::new(&capture.node, source_file))
        });
        captures.sort_by(|cap1, cap2| cap1.name.cmp(&cap2.name));

        let captures = Rc::new(captures);
        Self { captures }
    }
}

unsafe impl<'v> Trace<'v> for QueryCaptures<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {
        // Safety: Capture<'v> contains no Values
    }
}

impl<'v> QueryCaptures<'v> {
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn keys<'v>(this: Value<'v>) -> starlark::Result<QueryCapturesKeys<'v>> {
            let this = this
                .request_value::<&QueryCaptures>()
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

#[starlark_value(type = "QueryCaptures")]
impl<'v> StarlarkValue<'v> for QueryCaptures<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(self.captures.len() as i32)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        let Some(key) = other.unpack_str() else {
            return Ok(false);
        };
        Ok(self.captures.iter().any(|capture| capture.name == key))
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        let Some(key) = index.unpack_str() else {
            return ValueError::unsupported_with(self, "[]", index);
        };
        let Some(capture) = self.captures.iter().find(|capture| capture.name == key) else {
            return Err(ValueError::KeyNotFound(key.into()).into());
        };
        Ok(capture.to_value_on(heap))
    }

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(self
            .captures
            .iter()
            .map(|capture| &capture.name)
            .map(|name| heap.alloc(name))
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
    name: String,
    #[allocative(skip)]
    quantifier: CaptureQuantifier,
    #[allocative(skip)]
    matches: SmallVec<[Node<'v>; Capture::CHUNK_SIZE]>,
}

unsafe impl<'v> Trace<'v> for Capture<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl<'v> Capture<'v> {
    const CHUNK_SIZE: usize = 4;

    fn new(name: String, quantifier: CaptureQuantifier) -> Self {
        let capacity = match quantifier {
            CaptureQuantifier::Zero => 0,
            CaptureQuantifier::ZeroOrOne | CaptureQuantifier::One => 1,
            CaptureQuantifier::ZeroOrMore | CaptureQuantifier::OneOrMore => Self::CHUNK_SIZE,
        };
        let matches = SmallVec::with_capacity(capacity);
        Self {
            name,
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

    fn to_value_on(&self, heap: &'v Heap) -> Value<'v> {
        match self.quantifier {
            CaptureQuantifier::Zero => Value::new_none(),
            CaptureQuantifier::One => {
                let first = self
                    .matches
                    .iter()
                    .next()
                    .expect("internal error: one-quantified capture never matched");
                heap.alloc(first.dupe())
            }
            CaptureQuantifier::ZeroOrOne => self
                .matches
                .iter()
                .next()
                .map(|n| heap.alloc(n.dupe()))
                .unwrap_or_else(Value::new_none),
            CaptureQuantifier::ZeroOrMore => heap.alloc(AllocList(
                self.matches.iter().map(Node::dupe).map(|n| heap.alloc(n)),
            )),
            CaptureQuantifier::OneOrMore => {
                assert!(
                    self.matches.len() >= 1,
                    "internal error: one-or-more quantified capture never matched"
                );
                heap.alloc(AllocList(
                    self.matches.iter().map(Node::dupe).map(|n| heap.alloc(n)),
                ))
            }
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative, Trace)]
struct QueryCapturesKeys<'v>(QueryCaptures<'v>);

impl<'v> Deref for QueryCapturesKeys<'v> {
    type Target = QueryCaptures<'v>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[starlark_value(type = "QueryCapturesKeys")]
impl<'v> StarlarkValue<'v> for QueryCapturesKeys<'v> {
    type Canonical = Self;

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        self.0.iterate_collect(heap)
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
        Ok(self
            .0
            .captures
            .iter()
            .map(|captures| captures.to_value_on(heap))
            .collect())
    }
}

impl<'v> Deref for QueryCapturesValues<'v> {
    type Target = QueryCaptures<'v>;

    fn deref(&self) -> &Self::Target {
        &self.0
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

impl<'v> Deref for QueryCapturesItems<'v> {
    type Target = QueryCaptures<'v>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[starlark_value(type = "QueryCapturesItems")]
impl<'v> StarlarkValue<'v> for QueryCapturesItems<'v> {
    type Canonical = Self;

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(self
            .0
            .captures
            .iter()
            .map(|capture| (heap.alloc(&capture.name), capture.to_value_on(heap)))
            .map(|pair| heap.alloc(pair))
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
                                ''',
                                on_match,
                            )

                        def on_match(event):
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
                                ''',
                                on_match,
                            )

                        def on_match(event):
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

    #[test]
    fn captures() {
        let src_path = SourcePath::new_in(&Utf8Path::new("main.rs"), &Utf8Path::new("./"));
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
            let tree = parser.parse(&content, None).unwrap();
            assert!(!tree.root_node().has_error());
            tree
        };
        println!(
            "{:#?}",
            QueryCursor::new()
                .matches(&query, tree.root_node(), content.as_bytes())
                .map(|qmatch| QueryCaptures::new(&query, qmatch, &src_file).captures)
                .collect::<Vec<_>>()
        );
        let mut cursor = QueryCursor::new();
        let qmatch = cursor
            .matches(&query, tree.root_node(), content.as_bytes())
            .next()
            .unwrap();
        let heap = Heap::new();
        let captures = heap.alloc(QueryCaptures::new(&query, qmatch, &src_file));

        let property_types = [
            ("absent", Some("NoneType")),
            ("optional_block_comments", Some("list")),
            ("mandatory_line_comments", Some("list")),
            ("optional_absent_str", Some("NoneType")),
            ("optional_present_let", Some("Node")),
            ("mandatory_expression", Some("Node")),
            ("no_such_attr", None),
        ];
        for (property, typ) in property_types {
            let capture = captures.at(heap.alloc(property), &heap);
            if let Some(typ) = typ {
                assert_eq!(
                    capture.unwrap().get_type(),
                    typ,
                    "wrong type for {property}"
                );
            } else {
                assert!(capture.is_err(), "expected error for {property}");
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
            .into_iter()
            .all(|elem| elem.get_type() == "Node"));
    }
}
