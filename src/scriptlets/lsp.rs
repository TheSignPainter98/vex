use std::fmt::Display;

use allocative::Allocative;
use dupe::Dupe;
use starlark::values::{AllocValue, Heap, StarlarkValue, Value};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType, Trace};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative, Trace)]
pub struct Lsp<'v> {
    pub language: Value<'v>,
}

impl Lsp<'_> {
    const NAME: &'static str = "Lsp";
    const LANGUAGE_ATTR_NAME: &'static str = "language";
}

impl Display for Lsp<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::NAME)
    }
}

#[starlark_value(type = "Lsp")]
impl<'v> StarlarkValue<'v> for Lsp<'v> {
    fn dir_attr(&self) -> Vec<String> {
        vec![Self::LANGUAGE_ATTR_NAME.to_owned()]
    }

    fn get_attr(&self, attr: &str, _heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::LANGUAGE_ATTR_NAME => Some(self.language.dupe()),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        attr == Self::LANGUAGE_ATTR_NAME
    }
}

impl<'v> AllocValue<'v> for Lsp<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use starlark::{
        environment::{Globals, Module},
        eval::Evaluator,
        syntax::{AstModule, Dialect},
    };

    use crate::{supported_language::SupportedLanguage, vextest::VexTest};

    use super::*;

    #[test]
    fn properties() {
        let module = Module::new();
        let language = module.heap().alloc(SupportedLanguage::Rust.to_string());
        let lsp = Lsp { language };
        module.set("lsp", module.heap().alloc(lsp));

        let test_code = indoc! {r#"
            def test():
                expected = "Lsp"
                actual = type(lsp)
                if actual != expected:
                    fail('expected type name %r but got %r' % (expected, actual))

                if not hasattr(lsp, 'language'):
                    fail("lsp has no 'language' field")
                if 'language' not in dir(lsp):
                    fail("lsp 'language' field improperly declared")
                if lsp.language != 'rust':
                    fail('incorrect lsp language: got %s' % lsp.language)
            test()
        "#};
        let ast = AstModule::parse("test.star", test_code.to_owned(), &Dialect::Standard).unwrap();

        let mut eval = Evaluator::new(&module);
        let globals = Globals::standard();
        eval.eval_module(ast, &globals).unwrap();
    }

    #[test]
    fn availability() {
        VexTest::new("enabled")
            .with_manifest(indoc! {r#"
                [vex]
                version = "1"
                enable-lsp = true
            "#})
            .with_scriptlet(
                "vexes/test.star",
                indoc! {"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        for language in ['rust', 'go', 'python']:
                            if vex.lsp_for(language).language != language:
                                fail('language server %r language server reported incorrect language' % language)
                "},
            )
            .assert_irritation_free();

        VexTest::new("disabled")
            .with_manifest(indoc! {r#"
                [vex]
                version = "1"
                enable-lsp = false
            "#})
            .with_scriptlet(
                "vexes/test.star",
                indoc! {"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.lsp_for('rust').language
                "},
            )
            .returns_error("lsp disabled");
    }
}
