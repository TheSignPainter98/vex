use std::fmt::Display;

use allocative::Allocative;
use dupe::Dupe;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::{none::NoneType, NoSerialize, ProvidesStaticType, StarlarkValue, Value},
};
use starlark_derive::starlark_value;

use crate::{
    error::Error,
    irritation::IrritationRenderer,
    result::Result,
    scriptlets::{
        action::Action,
        event::EventType,
        extra_data::{InvocationData, ObserverDataBuilder},
        Node,
    },
};

type StarlarkSourceAnnotation<'v> = (Node<'v>, &'v str);

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct AppObject;

impl AppObject {
    pub const NAME: &'static str = "vex";

    #[allow(clippy::type_complexity)]
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn language<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] lang: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.language", &[Action::Initing])?;

            ObserverDataBuilder::get_from(eval.module()).set_language(lang.into());

            Ok(NoneType)
        }

        fn query<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] query: &'v str,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.query", &[Action::Initing])?;

            ObserverDataBuilder::get_from(eval.module()).set_query(query.into());

            Ok(NoneType)
        }

        fn observe<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] event: &str,
            #[starlark(require=pos)] handler: Value<'v>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.observe", &[Action::Initing])?;

            let event = event.parse()?;
            ObserverDataBuilder::get_from(eval.module()).add_observer(event, handler);

            Ok(NoneType)
        }

        fn warn<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] message: &'v str,
            #[starlark(require=named)] at: Option<StarlarkSourceAnnotation>,
            #[starlark(require=named)] show_also: Option<Vec<StarlarkSourceAnnotation>>,
            #[starlark(require=named)] extra_info: Option<&'v str>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(
                eval,
                "vex.warn",
                &[
                    Action::Vexing(EventType::OpenProject),
                    Action::Vexing(EventType::Match),
                    Action::Vexing(EventType::CloseFile),
                    Action::Vexing(EventType::CloseProject),
                ],
            )?;

            if matches!((&at, &show_also), (None, Some(_))) {
                return Err(Error::InvalidWarnCall(
                    "cannot display `show_also` without an `at` argument",
                )
                .into());
            }

            let inv_data = InvocationData::get_from(eval);
            let vex_path = inv_data.path();
            let mut renderer = IrritationRenderer::new(vex_path.dupe(), message);
            if let Some(at) = at {
                renderer.set_source(at)
            }
            if let Some(show_also) = show_also {
                renderer.set_show_also(show_also);
            }
            if let Some(extra_info) = extra_info {
                renderer.set_extra_info(extra_info);
            }
            inv_data.add_irritation(renderer.render());

            Ok(NoneType)
        }
    }

    fn check_attr_available(
        eval: &Evaluator<'_, '_>,
        attr_path: &'static str,
        available_actions: &'static [Action],
    ) -> Result<()> {
        let curr_action = InvocationData::get_from(eval).action();
        if !available_actions.contains(&curr_action) {
            return Err(Error::ActionUnavailable {
                what: attr_path,
                action: curr_action,
            });
        }
        Ok(())
    }
}

starlark::starlark_simple_value!(AppObject);
#[starlark_value(type = "Vex")]
impl<'v> StarlarkValue<'v> for AppObject {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(AppObject::methods)
    }
}

impl Display for AppObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::NAME.fmt(f)
    }
}

#[cfg(test)]
mod test {
    use indoc::{formatdoc, indoc};
    use insta::assert_yaml_snapshot;
    use joinery::JoinableIterator;

    use crate::vextest::VexTest;

    #[test]
    fn argument_kinds() {
        let lines = indoc! {r#"
            def init():
                vex.language('rust')
                vex.query('(binary_expression) @bin_expr')
                vex.observe('match', on_match)

            def on_match(event):
                vex.warn('okay')
        "#}
        .lines()
        .collect::<Vec<_>>();
        let line_replacements = [
            ("language", 2, "vex.language(lang = 'rust')"),
            ("query", 3, "vex.query(query='(binary_expression)')"),
            ("observe-event", 4, "vex.query(on_match, event='match')"),
            (
                "observe-observer",
                4,
                "vex.query('match', observer=on_match)",
            ),
            ("warn-message", 7, "vex.warn(message='oh no')"),
            (
                "warn-at",
                7,
                "vex.warn('oh no', (event.captures['bin_expr'], 'bin_expr'))",
            ),
            (
                "warn-show-also",
                7,
                "vex.warn('oh no', [(event.captures['bin_expr'], 'bin_expr')], at=(event.captures['bin_expr'], 'bin_expr'))"
            ),
            (
                "warn-show-also",
                7,
                "vex.warn('oh no', 'extra_info', at=(event.captures['bin_expr'], 'bin_expr'))"
            ),
        ];
        let error_messages = line_replacements
            .into_iter()
            .map(|(name, replace_line_num, replacement)| {
                VexTest::new(name)
                    .with_scriptlet(format!("vexes/{name}.star"), {
                        lines[..replace_line_num - 1]
                            .iter()
                            .chain(&[&textwrap::indent(replacement, "    ")[..]])
                            .chain(&lines[replace_line_num..])
                            .join_with("\n")
                            .to_string()
                    })
                    .with_source_file(
                        "src/main.rs",
                        indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                    "#},
                    )
                    .try_run()
                    .unwrap_err()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_yaml_snapshot!(error_messages);
    }

    #[test]
    fn warn_valid() {
        const VEX_NAME: &str = "name_of_vex";
        const AT: &str = "node bin_expr found here";
        const SHOW_ALSO_L: &str = "node l found here";
        const SHOW_ALSO_R: &str = "node r found here";
        const EXTRA_INFO: &str = "some hopefully useful extra info";

        let irritations = VexTest::new("arg-combinations")
            .with_scriptlet(
                format!("vexes/{VEX_NAME}.star"),
                formatdoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr')
                        vex.observe('match', on_match)

                    def on_match(event):
                        bin_expr = event.captures['bin_expr']
                        l = event.captures['l']
                        r = event.captures['r']

                        at = (bin_expr, '{AT}')
                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]
                        extra_info = '{EXTRA_INFO}'

                        vex.warn('test-0')
                        vex.warn('test-1', extra_info=extra_info)
                        vex.warn('test-2', at=at)
                        vex.warn('test-3', at=at, show_also=show_also)
                        vex.warn('test-4', at=at, show_also=show_also, extra_info=extra_info)
                "#},
            )
            .with_source_file(
                "main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                .into_iter()
                "#},
            )
            .try_run()
            .unwrap()
            .into_irritations()
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>();
        assert_eq!(irritations.len(), 5);

        println!("{irritations:?}");

        let assert_contains = |irritation: &str, strings: &[&str]| {
            [VEX_NAME]
                .as_ref()
                .iter()
                .chain(strings)
                .for_each(|string| {
                    assert!(
                        irritation.contains(string),
                        "could not find {string} in {irritation}"
                    )
                })
        };
        assert_contains(&irritations[0], &[VEX_NAME, "test-0"]);
        assert_contains(&irritations[1], &[VEX_NAME, "test-1", EXTRA_INFO]);
        assert_contains(&irritations[2], &[VEX_NAME, "test-2", AT]);
        assert_contains(
            &irritations[3],
            &[VEX_NAME, "test-3", AT, SHOW_ALSO_L, SHOW_ALSO_R],
        );
        assert_contains(
            &irritations[4],
            &[VEX_NAME, "test-4", AT, SHOW_ALSO_L, SHOW_ALSO_R, EXTRA_INFO],
        );

        assert_yaml_snapshot!(irritations);
    }

    #[test]
    fn warn_invalid() {
        const VEX_NAME: &str = "name_of_vex";
        const SHOW_ALSO_L: &str = "node l found here";
        const SHOW_ALSO_R: &str = "node r found here";
        VexTest::new("show-also-without-at")
            .with_scriptlet(
                format!("vexes/{VEX_NAME}.star"),
                formatdoc! {r#"
                    def init():
                        vex.language('rust')
                        vex.query('(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr')
                        vex.observe('match', on_match)

                    def on_match(event):
                        l = event.captures['l']
                        r = event.captures['r']

                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]

                        vex.warn('test-2', show_also=show_also)
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                .into_iter()
                "#},
            )
            .returns_error("cannot display `show_also` without an `at` argument")
    }

    #[test]
    fn warn_sorting() {
        const VEX_1_NAME: &str = "vex_1";
        const VEX_2_NAME: &str = "vex_2";
        const AT: &str = "node bin_expr found here";
        const SHOW_ALSO_L: &str = "node l found here";
        const SHOW_ALSO_R: &str = "node r found here";
        const EXTRA_INFO: &str = "some hopefully useful extra info";

        let vex_source = formatdoc! {r#"
            def init():
                vex.language('rust')
                vex.query('(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr')
                vex.observe('match', on_match)

            def on_match(event):
                bin_expr = event.captures['bin_expr']
                l = event.captures['l']
                r = event.captures['r']

                at = (bin_expr, '{AT}')
                show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]
                extra_info = '{EXTRA_INFO}'

                # Emit warnings in opposite order to expected.
                vex.warn('message-six', at=(bin_expr, 'bin_expr'), show_also=[(r, 'r')])
                vex.warn('message-four', at=(bin_expr, 'bin_expr'), show_also=[(l, 'l')])
                vex.warn('message-seven', at=(bin_expr, 'bin_expr'), show_also=[(r, 'r')], extra_info=extra_info)
                vex.warn('message-five', at=(bin_expr, 'bin_expr'), show_also=[(l, 'l')], extra_info=extra_info)
                vex.warn('message-eight', at=(r, 'r'))
                vex.warn('message-three', at=(l, 'l'))
                vex.warn('message-one')
                vex.warn('message-two')
        "#};
        let irritations = VexTest::new("many-origins")
            .with_scriptlet(format!("vexes/{VEX_2_NAME}.star"), &vex_source)
            .with_scriptlet(format!("vexes/{VEX_1_NAME}.star"), &vex_source)
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                .into_iter()
                "#},
            )
            .try_run()
            .unwrap()
            .into_irritations()
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>();

        assert_eq!(irritations.len(), 8 * 2);

        let text_numbers = [
            "one", "two", "three", "four", "five", "six", "seven", "eight",
        ];
        let text_number_indices = text_numbers
            .iter()
            .map(|text_num| irritations.iter().position(|irr| irr.contains(text_num)))
            .map(Option::unwrap)
            .collect::<Vec<_>>();
        text_number_indices
            .iter()
            .scan(None, |prev, e| {
                let ret = prev.iter().map(|prev| *prev < e).next().unwrap_or(true);
                *prev = Some(e);
                Some(ret)
            })
            .for_each(|lt| {
                assert!(
                    lt,
                    "indices do not monotonically increase: {text_number_indices:?}"
                )
            });

        assert_yaml_snapshot!(irritations);
    }
}
