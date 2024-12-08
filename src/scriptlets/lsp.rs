use std::fmt::Display;

use allocative::Allocative;
use starlark::values::StarlarkValue;
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct Lsp;

impl Lsp {
    const NAME: &'static str = "lsp";
}

impl Display for Lsp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::NAME.fmt(f)
    }
}

starlark::starlark_simple_value!(Lsp);
#[starlark_value(type = "Lsp")]
impl<'v> StarlarkValue<'v> for Lsp {}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use starlark::{
        environment::{Globals, Module},
        eval::Evaluator,
        syntax::{AstModule, Dialect},
    };

    use super::*;

    #[test]
    fn type_name() {
        let lsp = Lsp;

        let test_code = indoc! {r#"
            def test():
                expected = "Lsp"
                actual = type(lsp)
                if actual != expected:
                    fail('expected type name %r but got %r' % (expected, actual))
            test()
        "#};
        let ast = AstModule::parse("test.star", test_code.to_owned(), &Dialect::Standard).unwrap();
        let globals = Globals::standard();
        let module = Module::new();
        module.set("lsp", module.heap().alloc(lsp));
        let mut eval = Evaluator::new(&module);
        eval.eval_module(ast, &globals).unwrap();
    }
}
