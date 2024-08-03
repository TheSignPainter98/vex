use std::fmt::Display;

use allocative::Allocative;
use camino::Utf8Path;
use derive_new::new;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    eval::Evaluator,
    starlark_module,
    values::{
        list::UnpackList, none::NoneType, Heap, NoSerialize, ProvidesStaticType, StarlarkValue,
        StringValue, Value,
    },
};
use starlark_derive::starlark_value;

use crate::{
    error::Error,
    irritation::IrritationRenderer,
    result::Result,
    scriptlets::{
        action::Action,
        event::EventKind,
        extra_data::{TempData, UnfrozenRetainedData},
        intents::UnfrozenIntent,
        main_annotation::MainAnnotation,
        observers::UnfrozenObserver,
        Node,
    },
    source_path::PrettyPath,
    supported_language::SupportedLanguage,
    vex::id::VexId,
};

#[derive(Debug, PartialEq, Eq, new, ProvidesStaticType, NoSerialize, Allocative)]
pub struct AppObject {
    lenient: bool,
}

impl AppObject {
    pub const NAME: &'static str = "vex";
    const LENIENT_ATTR_NAME: &'static str = "lenient";

    #[allow(clippy::type_complexity)]
    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn search<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] language: &'v str,
            #[starlark(require=pos)] query: StringValue<'v>,
            #[starlark(require=pos)] on_match: Value<'v>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(
                eval,
                "vex.search",
                &[
                    Action::Vexing(EventKind::OpenProject),
                    Action::Vexing(EventKind::OpenFile),
                ],
            )?;

            let ret_data = UnfrozenRetainedData::get_from(eval.module());
            let language = language.parse::<SupportedLanguage>()?;
            let query = {
                let temp_data = TempData::get_from(eval);
                temp_data.query_cache.get_or_create(language, query)?
            };
            let on_match = UnfrozenObserver::new(on_match);
            ret_data.declare_intent(UnfrozenIntent::Find {
                language,
                query,
                on_match,
            });

            Ok(NoneType)
        }

        fn observe<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] event: &str,
            #[starlark(require=pos)] observer: Value<'v>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(eval, "vex.observe", &[Action::Initing])?;

            let ret_data = UnfrozenRetainedData::get_from(eval.module());
            let event_kind = event.parse()?;
            let observer = UnfrozenObserver::new(observer);
            ret_data.declare_intent(UnfrozenIntent::Observe {
                event_kind,
                observer,
            });

            Ok(NoneType)
        }

        #[allow(clippy::too_many_arguments)]
        fn warn<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] vex_id: &'v str,
            #[starlark(require=pos)] message: &'v str,
            #[starlark(require=named)] at: Option<MainAnnotation<'v>>,
            #[starlark(require=named)] show_also: Option<UnpackList<(Node<'v>, &'v str)>>,
            #[starlark(require=named)] info: Option<&'v str>,
            eval: &mut Evaluator<'v, '_>,
        ) -> anyhow::Result<NoneType> {
            AppObject::check_attr_available(
                eval,
                "vex.warn",
                &[
                    Action::Vexing(EventKind::OpenProject),
                    Action::Vexing(EventKind::OpenFile),
                    Action::Vexing(EventKind::Match),
                ],
            )?;

            if matches!((&at, &show_also), (None, Some(_)))
                || matches!(
                    (&at, &show_also),
                    (Some(MainAnnotation::Path { .. }), Some(_))
                )
            {
                return Err(Error::InvalidWarnCall(
                    "cannot display `show_also` without an `at` argument containing a Node",
                )
                .into());
            }

            let vex_id = VexId::try_from(vex_id.to_string())?;

            let temp_data = TempData::get_from(eval);
            let ignored = at.as_ref().and_then(|at| at.node()).is_some_and(|node| {
                temp_data.ignore_markers.is_some_and(|ignore_markers| {
                    ignore_markers.is_ignored(node.byte_range().start, &vex_id)
                })
            });
            if ignored {
                return Ok(NoneType);
            }

            let ret_data = UnfrozenRetainedData::get_from(eval.module());
            let mut irritation_renderer = IrritationRenderer::new(vex_id, message);
            if let Some(at) = at {
                irritation_renderer.set_source(at)
            }
            if let Some(show_also) = show_also {
                irritation_renderer.set_show_also(show_also.items);
            }
            if let Some(info) = info {
                irritation_renderer.set_info(info);
            }
            ret_data.declare_intent(UnfrozenIntent::Warn(irritation_renderer.render()));

            Ok(NoneType)
        }

        fn scan_file<'v>(
            #[starlark(this)] _this: Value<'v>,
            #[starlark(require=pos)] file_name: &'v str,
            #[starlark(require=pos)] language: &'v str,
            #[starlark(require=pos)] content: &'v str,
            eval: &mut Evaluator<'_, '_>,
        ) -> anyhow::Result<NoneType> {
            let file_name = PrettyPath::new(Utf8Path::new(file_name));
            let language = language.parse()?;
            let content = content.to_string();
            let ret_data = UnfrozenRetainedData::get_from(eval.module());
            ret_data.declare_intent(UnfrozenIntent::ScanFile {
                file_name,
                language,
                content,
            });
            Ok(NoneType)
        }
    }

    fn check_attr_available(
        eval: &Evaluator<'_, '_>,
        attr_path: &'static str,
        available_actions: &'static [Action],
    ) -> Result<()> {
        let curr_action = TempData::get_from(eval).action;
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

    fn dir_attr(&self) -> Vec<String> {
        [Self::LENIENT_ATTR_NAME]
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn get_attr(&self, attr: &str, _: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::LENIENT_ATTR_NAME => Some(Value::new_bool(self.lenient)),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _: &'v Heap) -> bool {
        attr == Self::LENIENT_ATTR_NAME
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

    use crate::vextest::VexTest;

    #[test]
    fn warn_valid() {
        const VEX_NAME: &str = "name-of-vex";
        const VEX_FILE_NAME: &str = "name_of_vex";
        const FILE_NAME: &str = "main.rs";
        const AT_PATH_LABEL: &str = "file label";
        const AT_NODE_LABEL: &str = "node bin_expr";
        const SHOW_ALSO_L: &str = "node l";
        const SHOW_ALSO_R: &str = "node r";
        const INFO: &str = "some hopefully useful extra info";

        let irritations = VexTest::new("arg-combinations")
            .with_scriptlet(
                format!("vexes/{VEX_FILE_NAME}.star"),
                formatdoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                            on_match,
                        )

                    def on_match(event):
                        bin_expr = event.captures['bin_expr']
                        l = event.captures['l']
                        r = event.captures['r']

                        at_path_unlabelled = event.path
                        at_path_labelled = (event.path, '{AT_PATH_LABEL}')
                        at_node_unlabelled = bin_expr
                        at_node_labelled = (bin_expr, '{AT_NODE_LABEL}')
                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]
                        info = '{INFO}'

                        vex.warn('{VEX_NAME}', 'test-01')
                        vex.warn('{VEX_NAME}', 'test-00')
                        vex.warn('{VEX_NAME}', 'test-03', info=info)
                        vex.warn('{VEX_NAME}', 'test-02', info=info)
                        vex.warn('{VEX_NAME}', 'test-04', at=at_path_unlabelled)
                        vex.warn('{VEX_NAME}', 'test-05', at=at_path_labelled)
                        vex.warn('{VEX_NAME}', 'test-07', at=at_node_unlabelled)
                        vex.warn('{VEX_NAME}', 'test-06', at=at_node_unlabelled)
                        vex.warn('{VEX_NAME}', 'test-09', at=at_node_labelled)
                        vex.warn('{VEX_NAME}', 'test-08', at=at_node_labelled)
                        vex.warn('{VEX_NAME}', 'test-11', at=at_node_unlabelled, show_also=show_also)
                        vex.warn('{VEX_NAME}', 'test-10', at=at_node_unlabelled, show_also=show_also)
                        vex.warn('{VEX_NAME}', 'test-13', at=at_node_labelled, show_also=show_also)
                        vex.warn('{VEX_NAME}', 'test-12', at=at_node_labelled, show_also=show_also)
                        vex.warn('{VEX_NAME}', 'test-15', at=at_node_unlabelled, show_also=show_also, info=info)
                        vex.warn('{VEX_NAME}', 'test-14', at=at_node_unlabelled, show_also=show_also, info=info)
                        vex.warn('{VEX_NAME}', 'test-17', at=at_node_labelled, show_also=show_also, info=info)
                        vex.warn('{VEX_NAME}', 'test-16', at=at_node_labelled, show_also=show_also, info=info)
                "#},
            )
            .with_source_file(
                FILE_NAME,
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                "#},
            )
            .try_run()
            .unwrap()
            .into_irritations()
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>();
        assert_eq!(irritations.len(), 18);

        let assert_contains = |irritation: &str, strings: &[&str]| {
            strings.iter().for_each(|string| {
                assert!(
                    irritation.contains(string),
                    "could not find {string} in {irritation}"
                )
            })
        };
        assert_contains(&irritations[0], &[VEX_NAME, "test-00"]);
        assert_contains(&irritations[1], &[VEX_NAME, "test-01"]);
        assert_contains(&irritations[2], &[VEX_NAME, "test-02", INFO]);
        assert_contains(&irritations[3], &[VEX_NAME, "test-03", INFO]);
        assert_contains(&irritations[4], &[VEX_NAME, "test-04", FILE_NAME]);
        assert_contains(
            &irritations[5],
            &[VEX_NAME, "test-05", FILE_NAME, AT_PATH_LABEL],
        );
        assert_contains(&irritations[6], &[VEX_NAME, "test-06"]);
        assert_contains(&irritations[7], &[VEX_NAME, "test-07"]);
        assert_contains(&irritations[8], &[VEX_NAME, "test-08", AT_NODE_LABEL]);
        assert_contains(&irritations[9], &[VEX_NAME, "test-09", AT_NODE_LABEL]);
        assert_contains(
            &irritations[10],
            &[VEX_NAME, "test-10", SHOW_ALSO_L, SHOW_ALSO_R],
        );
        assert_contains(
            &irritations[11],
            &[VEX_NAME, "test-11", SHOW_ALSO_L, SHOW_ALSO_R],
        );
        assert_contains(
            &irritations[12],
            &[VEX_NAME, "test-12", AT_NODE_LABEL, SHOW_ALSO_L, SHOW_ALSO_R],
        );
        assert_contains(
            &irritations[13],
            &[VEX_NAME, "test-13", AT_NODE_LABEL, SHOW_ALSO_L, SHOW_ALSO_R],
        );
        assert_contains(
            &irritations[14],
            &[VEX_NAME, "test-14", SHOW_ALSO_L, SHOW_ALSO_R, INFO],
        );
        assert_contains(
            &irritations[15],
            &[VEX_NAME, "test-15", SHOW_ALSO_L, SHOW_ALSO_R, INFO],
        );
        assert_contains(
            &irritations[16],
            &[
                VEX_NAME,
                "test-16",
                AT_NODE_LABEL,
                SHOW_ALSO_L,
                SHOW_ALSO_R,
                INFO,
            ],
        );
        assert_contains(
            &irritations[17],
            &[
                VEX_NAME,
                "test-17",
                AT_NODE_LABEL,
                SHOW_ALSO_L,
                SHOW_ALSO_R,
                INFO,
            ],
        );

        assert_yaml_snapshot!(irritations);
    }

    #[test]
    fn warn_invalid() {
        const VEX_NAME: &str = "name-of-vex";
        const VEX_FILE_NAME: &str = "name_of_vex";
        const SHOW_ALSO_L: &str = "node l found here";
        const SHOW_ALSO_R: &str = "node r found here";
        VexTest::new("show-also-without-at")
            .with_scriptlet(
                format!("vexes/{VEX_NAME}.star"),
                formatdoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                            on_match,
                        )

                    def on_match(event):
                        l = event.captures['l']
                        r = event.captures['r']

                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]

                        vex.warn('{VEX_NAME}', 'test-2', show_also=show_also)

                    def on_match(event):
                        l = event.captures['l']
                        r = event.captures['r']

                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]

                        vex.warn('{VEX_NAME}', 'test-2', at=event.path, show_also=show_also)
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                "#},
            )
            .returns_error("cannot display `show_also` without an `at` argument");
        VexTest::new("show-also-with-path-at")
            .with_scriptlet(
                format!("vexes/{VEX_FILE_NAME}.star"),
                formatdoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                            on_match,
                        )

                    def on_match(event):
                        l = event.captures['l']
                        r = event.captures['r']

                        show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]

                        vex.warn('{VEX_NAME}', 'test-2', at=event.path, show_also=show_also)
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2;
                        println!("{x}");
                    }
                "#},
            )
            .returns_error("cannot display `show_also` without an `at` argument");
    }

    #[test]
    fn warn_sorting() {
        const VEX_1_NAME: &str = "vex-1";
        const VEX_1_FILE_NAME: &str = "vex_1";
        const VEX_2_NAME: &str = "vex-2";
        const VEX_2_FILE_NAME: &str = "vex_2";
        const AT: &str = "node bin_expr found here";
        const SHOW_ALSO_L: &str = "node l found here";
        const SHOW_ALSO_R: &str = "node r found here";
        const INFO: &str = "some hopefully useful extra info";

        let vex_source = |name| {
            formatdoc! {r#"
            def init():
                vex.observe('open_project', on_open_project)

            def on_open_project(event):
                vex.search(
                    'rust',
                    '(binary_expression left: (integer_literal) @l right: (integer_literal) @r) @bin_expr',
                    on_match,
                )

            def on_match(event):
                bin_expr = event.captures['bin_expr']
                l = event.captures['l']
                r = event.captures['r']

                at = (bin_expr, '{AT}')
                show_also = [(l, '{SHOW_ALSO_L}'), (r, '{SHOW_ALSO_R}')]
                info = '{INFO}'

                # Emit warnings in opposite order to expected.
                vex.warn('{name}', 'message-six', at=(bin_expr, 'bin_expr'), show_also=[(r, 'r')])
                vex.warn('{name}', 'message-four', at=(bin_expr, 'bin_expr'), show_also=[(l, 'l')])
                vex.warn('{name}', 'message-seven', at=(bin_expr, 'bin_expr'), show_also=[(r, 'r')], info=info)
                vex.warn('{name}', 'message-five', at=(bin_expr, 'bin_expr'), show_also=[(l, 'l')], info=info)
                vex.warn('{name}', 'message-eight', at=(r, 'r'))
                vex.warn('{name}', 'message-three', at=(l, 'l'))
                vex.warn('{name}', 'message-one')
                vex.warn('{name}', 'message-two')
        "#}
        };
        let irritations = VexTest::new("many-origins")
            .with_scriptlet(
                format!("vexes/{}.star", VEX_2_FILE_NAME),
                &vex_source(VEX_2_NAME),
            )
            .with_scriptlet(
                format!("vexes/{}.star", VEX_1_FILE_NAME),
                &vex_source(VEX_1_NAME),
            )
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

    #[test]
    fn lenient() {
        let test_leniency = |lenient| {
            let irritations = VexTest::new("default")
                .with_lenient(lenient)
                .with_scriptlet(
                    "vexes/test.star",
                    indoc! {r#"
                        def init():
                            lenient = vex.lenient
                            def on_open_project(event):
                                vex.warn("test", "vex.lenient=%s" % lenient)
                                vex.warn("test", "vex.lenient=%s" % vex.lenient)
                            vex.observe('open_project', on_open_project)

                            vex.observe('open_project', on_open_project_standard)
                            vex.observe('open_file', on_open_file)

                        def on_open_project_standard(event):
                            vex.warn("test", "vex.lenient=%s" % vex.lenient)
                            vex.search(
                                'rust',
                                '(source_file)',
                                on_match,
                            )

                        def on_match(event):
                            vex.warn("test", "vex.lenient=%s" % vex.lenient)

                        def on_open_file(event):
                            vex.warn("test", "vex.lenient=%s" % vex.lenient)
                    "#},
                )
                .with_source_file(
                    "main.rs",
                    indoc! {r#"
                        fn main() {}
                    "#},
                )
                .try_run()
                .unwrap()
                .into_irritations()
                .into_iter()
                .map(|irr| irr.to_string())
                .collect::<Vec<_>>();
            assert_eq!(5, irritations.len());
            let must_contain = format!("vex.lenient={}", if lenient { "True" } else { "False" });
            assert!(irritations
                .iter()
                .all(|irr| irr.contains(&must_contain)), "expected {} to be lenient: got {irritations:?} but all must contain {must_contain}", if lenient { "all" } else { "none" });
        };
        test_leniency(true);
        test_leniency(false);
    }
}
