use std::fmt::Display;

use allocative::Allocative;
use dupe::Dupe;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::{none::NoneType, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueError},
};
use starlark_derive::starlark_value;
use tree_sitter::Query;

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
    supported_language::SupportedLanguage,
    trigger::{ContentTrigger, RawFilePattern, Trigger, TriggerId},
};

type StarlarkSourceAnnotation<'v> = (Node<'v>, &'v str);

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub struct AppObject;

impl AppObject {
    pub const NAME: &'static str = "vex";

    #[allow(clippy::type_complexity)]
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn add_trigger<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=named)] id: Option<&'v str>,
            #[starlark(require=named)] language: Option<&'v str>,
            #[starlark(require=named)] query: Option<&'v str>,
            #[starlark(require=named)] path: Option<Value<'v>>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.add_trigger", &[Action::Initing])?;

            let builder = ObserverDataBuilder::get_from(eval.module());

            // TODO(kcza): test me!
            if language.is_none() && query.is_none() && path.is_none() {
                return Err(Error::EmptyTrigger(builder.vex_path.dupe()).into());
            }

            let id = id.map(TriggerId::new);
            let content_trigger = {
                if language.is_none() && query.is_some() {
                    return Err(Error::QueryWithoutLanguage.into());
                }

                if let Some(language) = language {
                    let language = language.parse::<SupportedLanguage>()?;
                    let query = query
                        .map(|query| {
                            if query.is_empty() {
                                return Err(Error::EmptyQuery(builder.vex_path.dupe()));
                            }
                            Ok(Query::new(language.ts_language(), query)?)
                        })
                        .transpose()?;
                    Some(ContentTrigger { language, query })
                } else {
                    None
                }
            };
            let path_patterns = if let Some(path) = path {
                if let Some(path_patterns) = path.request_value::<&[Value<'v>]>() {
                    path_patterns
                        .iter()
                        .map(|path_pattern| {
                            let Some(path_pattern) = path_pattern.unpack_str() else {
                                return Err(anyhow::Error::from(
                                    ValueError::IncorrectParameterTypeWithExpected(
                                        path_pattern.get_type().into(),
                                        "str".into(),
                                    ),
                                ));
                            };
                            Ok(RawFilePattern(path_pattern.into())
                                .compile(&builder.project_root)?)
                        })
                        .collect::<anyhow::Result<_>>()?
                } else if let Some(path_pattern) = path.unpack_str() {
                    vec![RawFilePattern(path_pattern.into()).compile(&builder.project_root)?]
                } else {
                    return Err(ValueError::IncorrectParameterTypeWithExpected(
                        path.get_type().into(),
                        "str|[str]".into(),
                    )
                    .into());
                }
            } else {
                vec![]
            };
            builder.add_trigger(Trigger {
                id,
                content_trigger,
                path_patterns,
            })?;

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
                    Action::Vexing(EventType::OpenFile),
                    Action::Vexing(EventType::QueryMatch),
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
            // panic!("{inv_data:?}");

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
                vex.add_trigger(
                    language='rust',
                    query='(binary_expression) @bin_expr',
                )
                vex.observe('query_match', on_query_match)

            def on_query_match(event):
                vex.warn('okay')
        "#}
        .lines()
        .collect::<Vec<_>>();
        let line_replacements = [
            ("language", 2..=5, "vex.add_trigger('rust')"),
            ("query-without-language", 2..=5, "vex.add_trigger(query='(binary_expression)')"),
            ("observe-event", 6..=6, "vex.observe(on_query_match, event='query_match')"),
            (
                "observe-observer",
                6..=6,
                "vex.observe('query_match', observer=on_query_match)",
            ),
            ("warn-message", 9..=9, "vex.warn(message='oh no')"),
            (
                "warn-at",
                9..=9,
                "vex.warn('oh no', (event.captures['bin_expr'], 'bin_expr'))",
            ),
            (
                "warn-show-also",
                9..=9,
                "vex.warn('oh no', [(event.captures['bin_expr'], 'bin_expr')], at=(event.captures['bin_expr'], 'bin_expr'))"
            ),
            (
                "warn-show-also",
                9..=9,
                "vex.warn('oh no', 'extra_info', at=(event.captures['bin_expr'], 'bin_expr'))"
            ),
        ];
        let error_messages = line_replacements
            .into_iter()
            .map(|(name, replacement_range, replacement)| {
                let (replacement_start_line, replacement_end_line) = replacement_range.into_inner();
                VexTest::new(name)
                    .with_scriptlet(format!("vexes/{name}.star"), {
                        lines[..replacement_start_line - 1]
                            .iter()
                            .chain(&[&textwrap::indent(replacement, "    ")[..]])
                            .chain(&lines[replacement_end_line..])
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
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
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
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
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
                vex.add_trigger(
                    language='rust',
                    query='(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                )
                vex.observe('query_match', on_query_match)

            def on_query_match(event):
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
