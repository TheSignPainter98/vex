use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    iter,
};

use camino::{Utf8Component, Utf8PathBuf};
use dupe::Dupe;
use log::{error, log_enabled};
use starlark::values::FrozenHeap;

use crate::{
    cli::MaxProblems,
    context::Context,
    error::{Error, IOAction},
    result::Result,
    scriptlets::{
        action::Action,
        event::{PostTestRunEvent, PreTestRunEvent},
        handler_module::HandlerModule,
        query_cache::QueryCache,
        Intent, Observable, ObserveOptions, PreinitOptions, PreinitingStore, VexingStore,
    },
    source_path::PrettyPath,
};

pub fn test() -> Result<()> {
    let ctx = Context::acquire()?;
    let store = {
        let preinit_opts = PreinitOptions::default();
        PreinitingStore::new(&ctx)?.preinit(preinit_opts)?.init()?
    };

    run_tests(&ctx, &store)?;
    Ok(())
}

pub(crate) fn run_tests(ctx: &Context, store: &VexingStore) -> Result<()> {
    let files_to_scan = {
        let frozen_heap = FrozenHeap::new();
        let event = PreTestRunEvent;
        let handler_module = HandlerModule::new();
        let observe_opts = ObserveOptions {
            action: Action::Vexing(event.kind()),
            query_cache: &QueryCache::new(),
            ignore_markers: None,
        };
        store.observers_for(event.kind()).observe(
            &handler_module,
            handler_module.heap().alloc(event),
            observe_opts,
        )?;

        let mut files_to_scan = Vec::with_capacity(handler_module.intent_count());
        let mut seen_file_names = BTreeMap::new();
        handler_module
            .into_intents_on(&frozen_heap)?
            .into_iter()
            .for_each(|intent| match intent {
                Intent::ScanFile {
                    file_name,
                    language,
                    content,
                } => {
                    seen_file_names
                        .entry(file_name.dupe())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                    files_to_scan.push((file_name, language, content));
                }
                _ => panic!("internal error: unexpected intent: {intent:?}"),
            });
        let mut test_run_invalid = false;
        seen_file_names
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .for_each(|(file_name, count)| {
                test_run_invalid = true;
                if log_enabled!(log::Level::Error) {
                    error!("test file '{file_name}' declared {count} times");
                }
            });
        if test_run_invalid {
            return Err(Error::TestRunInvalid);
        }
        files_to_scan
    };

    let temp_dir = tempfile::tempdir().map_err(|cause| Error::IO {
        path: "(temp file)".into(),
        action: IOAction::Create,
        cause,
    })?;
    let temp_dir_path = Utf8PathBuf::try_from(temp_dir.path().to_path_buf()).unwrap();
    for (file_name, _language, content) in files_to_scan {
        // TODO(kcza): make use of declared language
        if file_name
            .components()
            .any(|component| !matches!(component, Utf8Component::Normal(_)))
        {
            return Err(Error::InvalidTest(format!(
                "cannot use path operators in test path: got {file_name}"
            )));
        }
        let abs_path = temp_dir_path.join(&file_name);

        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent).map_err(|cause| Error::IO {
                path: PrettyPath::new(parent),
                action: IOAction::Create,
                cause,
            })?;
        }
        File::create(&abs_path)
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&abs_path),
                action: IOAction::Create,
                cause,
            })?
            .write_all(content.as_bytes())
            .map_err(|cause| Error::IO {
                path: PrettyPath::new(&abs_path),
                action: IOAction::Write,
                cause,
            })?;
    }

    let collect_run_data = |lenient| {
        let sub_store = {
            let preinit_opts = PreinitOptions { lenient };
            // Create new store using the current context to inherit the existing scripts.
            PreinitingStore::new(ctx)?.preinit(preinit_opts)?.init()?
        };
        let sub_ctx = ctx.child_context(PrettyPath::new(&temp_dir_path));
        crate::vex(&sub_ctx, &sub_store, MaxProblems::Unlimited)
    };
    let nonlenient_data = collect_run_data(false)?;
    let lenient_data = collect_run_data(true)?;

    {
        let handler_module = HandlerModule::new();
        let irritations = nonlenient_data
            .irritations
            .into_iter()
            .zip(iter::repeat(false))
            .chain(lenient_data.irritations.into_iter().zip(iter::repeat(true)));
        let event = PostTestRunEvent::new(irritations, handler_module.heap());
        let observer_opts = ObserveOptions {
            action: Action::Vexing(event.kind()),
            query_cache: &QueryCache::new(),
            ignore_markers: None,
        };
        store.observers_for(event.kind()).observe(
            &handler_module,
            handler_module.heap().alloc(event),
            observer_opts,
        )?;
    }

    Ok(())
}

#[allow(clippy::module_inception)]
#[cfg(test)]
mod test {
    use indoc::formatdoc;

    use crate::vextest::VexTest;

    #[test]
    fn standard_flow() {
        VexTest::new("standard")
            .with_test_events(true)
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)
                            vex.observe('pre_test_run', on_pre_test_run)
                            vex.observe('post_test_run', on_post_test_run)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        right: (_) @right
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']
                            right = event.captures['right']

                            vex.warn(
                                'warning-a',
                                'warning-a-message',
                                at=bin_expr,
                            )
                            if 'b' in str(right):
                                vex.warn(
                                    'warning-b',
                                    'warning-b-message',
                                    at=bin_expr,
                                )
                            if 'c' in str(right):
                                vex.warn(
                                    'warning-c',
                                    'warning-c-message',
                                    at=bin_expr,
                                )
                            vex.warn(
                                'warning-d',
                                'warning-d-message',
                            )

                        def on_pre_test_run(event):
                            vex.scan(
                                'a_and_c.rs',
                                'rust',
                                '''


                                    // The newlines above are part of the test :D

                                    mod other;

                                    fn main() {{
                                        let _ = 1 + warning_a_and_c;
                                    }}
                                ''',
                            )
                            vex.scan(
                                'b_and_c.rs',
                                'rust',
                                '''
                                    fn other() {{
                                        let _ = 1 + warning_b_and_c;
                                    }}
                                ''',
                            )

                        def on_post_test_run(event):
                            expected_warnings = {{
                                'a_and_c.rs': {{
                                    'warning-a': [{{
                                        'id': 'warning-a',
                                        'message': 'warning-a-message',
                                        'at': {{
                                            'location': {{
                                                'start_row': 8,
                                            }},
                                        }},
                                    }}],
                                    'warning-c': [{{
                                        'id': 'warning-c',
                                        'message': 'warning-c-message',
                                        'at': {{
                                            'location': {{
                                                'start_row': 8,
                                            }},
                                        }},
                                    }}],
                                }},
                                'b_and_c.rs': {{
                                    'warning-b': [{{
                                        'id': 'warning-b',
                                        'message': 'warning-b-message',
                                        'at': {{
                                            'location': {{
                                                'start_row': 2,
                                            }},
                                        }},
                                    }}],
                                    'warning-c': [{{
                                        'id': 'warning-c',
                                        'message': 'warning-c-message',
                                        'at': {{
                                            'location': {{
                                                'start_row': 2,
                                            }},
                                        }},
                                    }}],
                                }},
                                'no-file': {{
                                    'warning-d': [{{
                                        'id': 'warning-d',
                                        'message': 'warning-d-message',
                                        'at': None,
                                    }}]
                                }}
                            }}

                            check['type'](event.warnings, 'WarningsByFile')
                            for file, expected_warnings_by_id in expected_warnings.items():
                                check['in'](file, event.warnings)
                                actual_warnings_by_id = event.warnings[file]
                                check['type'](actual_warnings_by_id, 'WarningsById')

                                for (id, expected_warnings) in expected_warnings_by_id.items():
                                    check['in'](id, actual_warnings_by_id)
                                    actual_warnings = actual_warnings_by_id[id]
                                    check['type'](actual_warnings, 'Warnings')

                                    for (actual_warning, expected_warning) in zip(actual_warnings, expected_warnings):
                                        check['eq'](actual_warning.id, id)
                                        check['eq'](actual_warning.id, expected_warning['id'])
                                        check['eq'](actual_warning.message, expected_warning['message'])

                                        expected_at = expected_warning['at']
                                        actual_at = actual_warning.at
                                        if expected_at == None:
                                            check['eq'](actual_at, None)
                                        else:
                                            check['type'](actual_at, 'tuple')
                                            (actual_src, actual_label) = actual_warning.at
                                            check['type'](actual_src, 'IrritationSource')
                                            check['eq'](actual_label, None)

                                            expected_src = expected_at
                                            check_src(actual_src, expected_src)

                        def check_src(actual_src, expected_src):
                            expected_location = expected_src['location']
                            actual_location = actual_src.location
                            check['type'](actual_location, 'Location')
                            if 'start_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['start_row'])
                            if 'start_column' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['start_column'])
                            if 'end_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['end_row'])
                            if 'end_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['end_row'])
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free()
    }

    #[test]
    fn attrs() {
        VexTest::new("standard")
            .with_test_events(true)
            .with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)
                            vex.observe('pre_test_run', on_pre_test_run)
                            vex.observe('post_test_run', on_post_test_run)

                        def on_open_project(event):
                            if vex.lenient:
                                return

                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (_) @left
                                        right: (_) @right
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']
                            left = event.captures['left']
                            right = event.captures['right']

                            vex.warn(
                                'test',
                                'just-message',
                            )
                            vex.warn(
                                'test',
                                'with-at-without-label',
                                at=bin_expr,
                            )
                            vex.warn(
                                'test',
                                'with-at-and-label',
                                at=(bin_expr, 'bin_expr label'),
                            )
                            vex.warn(
                                'test',
                                'with-at-and-show_also',
                                at=bin_expr,
                                show_also=[
                                    (left, 'left label'),
                                    (right, 'right label'),
                                ]
                            )
                            vex.warn(
                                'test',
                                'with-info',
                                info='info text',
                            )

                        def on_pre_test_run(event):
                            vex.scan(
                                'test_file.rs',
                                'rust',
                                '''
                                    fn main() {{
                                        let x = 1 + 1;
                                    }}
                                ''',
                            )

                        def on_post_test_run(event):
                            expected_warnings = [{{
                                'id': 'test',
                                'message': 'just-message',
                            }}, {{
                                'id': 'test',
                                'message': 'with-info',
                                'info': 'info text',
                            }}, {{
                                'id': 'test',
                                'message': 'with-at-without-label',
                                'at': {{
                                    'src': {{
                                        'location': {{
                                            'start_row': 2,
                                        }},
                                    }},
                                }},
                            }}, {{
                                'id': 'test',
                                'message': 'with-at-and-label',
                                'at': {{
                                    'src': {{
                                        'location': {{
                                            'start_row': 2
                                        }},
                                    }},
                                    'label': 'bin_expr label',
                                }},
                            }}, {{
                                'id': 'test',
                                'message': 'with-at-and-show_also',
                                'at': {{
                                    'src': {{
                                        'location': {{
                                            'start_row': 2,
                                        }},
                                    }},
                                }},
                                'show_also': [{{
                                    'src': {{
                                        'location': {{
                                            'start_row': 2,
                                        }},
                                    }},
                                    'label': 'left label',
                                }}, {{
                                    'src': {{
                                        'location': {{
                                            'start_row': 2,
                                        }},
                                    }},
                                    'label': 'right label',
                                }}],
                            }}]

                            no_file_warnings = event.warnings['no-file']['test']
                            test_file_warnings = event.warnings['test_file.rs']['test']
                            actual_warnings = []
                            for warning in no_file_warnings:
                                actual_warnings.append(warning)
                            for warning in test_file_warnings:
                                actual_warnings.append(warning)

                            check['eq'](len(actual_warnings), len(expected_warnings))
                            for (actual_warning, expected_warning) in zip(actual_warnings, expected_warnings):
                                check['eq'](actual_warning.id, expected_warning['id'])
                                check['eq'](actual_warning.message, expected_warning['message'])

                                actual_at = actual_warning.at
                                if 'at' in expected_warning:
                                    expected_at = expected_warning['at']
                                    check['type'](actual_at, 'tuple')
                                    (actual_src, actual_label) = actual_at

                                    expected_src = expected_at['src']
                                    check_src(actual_src, expected_src)

                                    if 'label' in expected_at:
                                        check['eq'](actual_label, expected_at['label'])
                                    else:
                                        check['eq'](actual_label, None)
                                else:
                                    check['eq'](actual_warning.at, None)

                                if 'show_also' in expected_warning:
                                    expected_show_also = expected_warning['show_also']
                                    actual_show_also = actual_warning.show_also
                                    for (expected_show_also_entry, actual_show_also_entry) in zip(expected_show_also, actual_show_also):
                                        check['type'](actual_show_also_entry, 'tuple')

                                        (actual_src, actual_label) = actual_show_also_entry

                                        expected_src = expected_show_also_entry['src']
                                        check_src(actual_src, expected_src)

                                        actual_label = actual_show_also_entry[1]
                                        if 'label' in expected_show_also_entry:
                                            check['eq'](actual_label, expected_show_also_entry['label'])
                                        else:
                                            check['eq'](actual_label, None)

                        def check_src(actual_src, expected_src):
                            expected_location = expected_src['location']
                            actual_location = actual_src.location
                            check['type'](actual_location, 'Location')
                            if 'start_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['start_row'])
                            if 'start_column' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['start_column'])
                            if 'end_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['end_row'])
                            if 'end_row' in expected_location:
                                check['eq'](actual_location.start_row, expected_location['end_row'])
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free()
    }
}
