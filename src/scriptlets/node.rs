use std::{fmt::Display, ops::Deref};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::{
    // environment::{Methods, MethodsBuilder, MethodsStatic},
    values::{
        none::NoneType, AllocValue, Demand, Freeze, Heap, NoSerialize, ProvidesStaticType,
        StarlarkValue, Trace, Value,
    },
};
use starlark_derive::starlark_value;
use tree_sitter::Node as TSNode;

use crate::error::Error;

#[derive(new, Clone, Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative, Dupe)]
pub struct Node<'v>(#[allocative(skip)] &'v TSNode<'v>);

unsafe impl<'v> Trace<'v> for Node<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl<'v> Deref for Node<'v> {
    type Target = TSNode<'v>;

    fn deref(&self) -> &Self::Target {
        self.0
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
}
