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
    // use indoc::formatdoc;
    //
    // use crate::vextest::VexTest;

    // #[test]
    // fn run() {
    //     VexTest::new("run")
    //         .with_test_event(true)
    //         .with_scriptlet(
    //             "vexes/test.star",
    //             formatdoc! {
    //                 r#"
    //                     load('{check_path}', 'check')
    //
    //                     def init():
    //                         vex.observe('open_project', on_open_project)
    //                         vex.observe('test', on_test)
    //
    //                     def on_open_project(event):
    //                         vex.search(
    //                             'rust',
    //                             '''
    //                                 (binary_expression
    //                                     left: (_) @left
    //                                 ) @bin_expr
    //                             ''',
    //                             on_match,
    //                         )
    //
    //                     def on_match(event):
    //                         check['true'](vex.lenient)
    //
    //                         bin_expr = event.captures['bin_expr']
    //                         if event.path.matches('src/main.rs'):
    //                             vex.warn('test-id', 'oh no!', at=bin_expr)
    //                         else:
    //                             left = event.captures['left']
    //                             vex.warn('test-id', 'oh no!',
    //                                 at=(bin_expr, 'label'),
    //                                 show_also=[(left, 'l')],
    //                                 info='waddup',
    //                             )
    //
    //                     def on_test(event):
    //                         data = vex.run(
    //                             lenient=True,
    //                             files={{
    //                                 'src/main.rs': '''
    //                                     mod other;
    //
    //                                     fn main() {{
    //                                         let _ = 1 + (2 + (3 + 3));
    //                                     }}
    //                                 ''',
    //                                 'src/other.rs': '''
    //                                     fn other() {{
    //                                         let _ = 4 + 4;
    //                                     }}
    //                                 ''',
    //                             }}
    //                         )
    //                         check['eq'](data.num_files_scanned, 2)
    //                         check['eq'](len(data.irritations), 4)
    //
    //                         simple_irritation = None
    //                         complex_irritation = None
    //                         for irritation in data.irritations:
    //                             check['type'](irritation, 'Irritation')
    //                             check['attrs'](irritation, ['at', 'info', 'message', 'show_also', 'vex_id'])
    //
    //                             (src, _) = irritation.at
    //                             if str(src.path) == 'src/main.rs':
    //                                 if simple_irritation == None:
    //                                     simple_irritation = irritation
    //                             elif complex_irritation == None:
    //                                 complex_irritation = irritation
    //                         check['neq'](simple_irritation, None)
    //                         check['neq'](complex_irritation, None)
    //
    //                         check['eq'](simple_irritation.vex_id, 'test-id')
    //                         (src, label) = simple_irritation.at
    //                         check['type'](src, 'IrritationSource')
    //                         check['eq'](str(src), 'src/main.rs:5:12-29')
    //                         check['eq'](str(src.path), 'src/main.rs')
    //                         loc = src.location
    //                         check['type'](loc, 'Location')
    //                         check['eq'](loc.start_row, 5)
    //                         check['eq'](loc.start_column, 12)
    //                         check['eq'](loc.end_row, 5)
    //                         check['eq'](loc.end_column, 29)
    //                         check['eq'](label, None)
    //                         check['eq'](simple_irritation.info, None)
    //                         check['eq'](simple_irritation.show_also, [])
    //
    //                         check['eq'](complex_irritation.vex_id, 'test-id')
    //                         (src, label) = complex_irritation.at
    //                         check['type'](src, 'IrritationSource')
    //                         check['eq'](str(src), 'src/other.rs:3:12-17')
    //                         check['eq'](str(src.path), 'src/other.rs')
    //                         loc = src.location
    //                         check['type'](loc, 'Location')
    //                         check['eq'](loc.start_row, 3)
    //                         check['eq'](loc.start_column, 12)
    //                         check['eq'](loc.end_row, 3)
    //                         check['eq'](loc.end_column, 17)
    //                         check['type'](label, 'string')
    //                         check['eq'](label, 'label')
    //                         show_also = complex_irritation.show_also
    //                         check['eq'](len(show_also), 1)
    //                         [(show_also_src, show_also_label)] = show_also
    //                         check['type'](show_also_src, 'IrritationSource')
    //                         check['eq'](str(show_also_src), 'src/other.rs:3:12-13')
    //                         check['eq'](show_also_label, 'l')
    //                         check['eq'](complex_irritation.info, 'waddup')
    //                 "#,
    //                 check_path = VexTest::CHECK_STARLARK_PATH,
    //             },
    //         )
    //         .assert_irritation_free()
    // }
}
