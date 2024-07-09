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
                                    (binary_expression) @bin_expr
                                ''',
                                on_match,
                            )

                        def on_match(event):
                            bin_expr = event.captures['bin_expr']
                            vex.warn('oh no!', at=(bin_expr, 'label'))

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
                    "#,
                    check_path = VexTest::CHECK_STARLARK_PATH,
                },
            )
            .assert_irritation_free()
    }
}
