use crate::{
    context::Context,
    result::Result,
    scriptlets::{
        action::Action, event::TestEvent, handler_module::HandlerModule, query_cache::QueryCache,
        Observable, ObserveOptions, PreinitOptions, PreinitingStore, VexingStore,
    },
    vex::id::PrettyVexId,
};

pub fn test() -> Result<()> {
    let ctx = Context::acquire()?;
    let store = {
        let preinit_opts = PreinitOptions::default();
        PreinitingStore::new(&ctx)?.preinit(preinit_opts)?.init()?
    };

    run_tests(&ctx, &store, None)?;
    Ok(())
}

pub fn run_tests(ctx: &Context, store: &VexingStore, _filter: Option<PrettyVexId>) -> Result<()> {
    let event = TestEvent;
    let handler_module = HandlerModule::new();
    let observe_opts = ObserveOptions {
        ctx: Some(ctx),
        store: Some(store),
        action: Action::Vexing(event.kind()),
        query_cache: &QueryCache::new(),
        ignore_markers: None,
    };
    store.observers_for(event.kind()).observe(
        &handler_module,
        handler_module.heap().alloc(event),
        observe_opts,
    )?;

    Ok(())
}

#[allow(clippy::module_inception)]
#[cfg(test)]
mod test {
    use indoc::formatdoc;

    use crate::vextest::VexTest;

    #[test]
    fn run() {
        VexTest::new("run")
            .with_test_event(true)
            .with_scriptlet(
                "vexes/asdf.star",
                formatdoc! {
                    r#"
                        load('{check_path}', 'check')

                        def init():
                            vex.observe('open_project', on_open_project)
                            vex.observe('test', on_test)

                        def on_open_project(event):
                            vex.search(
                                'rust',
                                '''
                                    (binary_expression
                                        left: (_) @left
                                    ) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            check['true'](vex.lenient)

                            bin_expr = event.captures['bin_expr']
                            if event.path.matches('src/main.rs'):
                                vex.warn('oh no!', at=bin_expr)
                            else:
                                left = event.captures['left']
                                vex.warn('oh no!',
                                    at=(bin_expr, 'label'),
                                    show_also=[(left, 'l')],
                                    info='waddup',
                                )

                        def on_test(event):
                            data = vex.run(
                                # event.vex_id,
                                'helo',
                                lenient=True,
                                files={{
                                    'src/main.rs': '''
                                        mod other;

                                        fn main() {{
                                            let _ = 1 + (2 + (3 + 3));
                                        }}
                                    ''',
                                    'src/other.rs': '''
                                        fn other() {{
                                            let _ = 4 + 4;
                                        }}
                                    ''',
                                }}
                            )
                            check['eq'](data.num_files_scanned, 2)
                            check['eq'](len(data.irritations), 4)

                            simple_irritation = None
                            complex_irritation = None
                            for irritation in data.irritations:
                                check['type'](irritation, 'Irritation')
                                check['attrs'](irritation, ['at', 'info', 'message', 'show_also', 'vex_id'])

                                (src, _) = irritation.at
                                if str(src.path) == 'src/main.rs':
                                    if simple_irritation == None:
                                        simple_irritation = irritation
                                elif complex_irritation == None:
                                    complex_irritation = irritation
                            check['neq'](simple_irritation, None)
                            check['neq'](complex_irritation, None)

                            check['eq'](simple_irritation.vex_id, 'asdf')
                            (src, label) = simple_irritation.at
                            check['type'](src, 'IrritationSource')
                            check['eq'](str(src), 'src/main.rs:5:12-29')
                            check['eq'](str(src.path), 'src/main.rs')
                            loc = src.location
                            check['type'](loc, 'Location')
                            check['eq'](loc.start_row, 5)
                            check['eq'](loc.start_column, 12)
                            check['eq'](loc.end_row, 5)
                            check['eq'](loc.end_column, 29)
                            check['eq'](label, None)
                            check['eq'](simple_irritation.info, None)
                            check['eq'](simple_irritation.show_also, [])

                            check['eq'](complex_irritation.vex_id, 'asdf')
                            (src, label) = complex_irritation.at
                            check['type'](src, 'IrritationSource')
                            check['eq'](str(src), 'src/other.rs:3:12-17')
                            check['eq'](str(src.path), 'src/other.rs')
                            loc = src.location
                            check['type'](loc, 'Location')
                            check['eq'](loc.start_row, 3)
                            check['eq'](loc.start_column, 12)
                            check['eq'](loc.end_row, 3)
                            check['eq'](loc.end_column, 17)
                            check['type'](label, 'string')
                            check['eq'](label, 'label')
                            show_also = complex_irritation.show_also
                            check['eq'](len(show_also), 1)
                            [(show_also_src, show_also_label)] = show_also
                            check['type'](show_also_src, 'IrritationSource')
                            check['eq'](str(show_also_src), 'src/other.rs:3:12-13')
                            check['eq'](show_also_label, 'l')
                            check['eq'](complex_irritation.info, 'waddup')
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free()
    }
}
