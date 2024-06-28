use starlark::values::FrozenHeap;

use crate::{
    context::Context,
    result::Result,
    scriptlets::{
        action::Action, event::TestEvent, handler_module::HandlerModule, query_cache::QueryCache,
        Observable, ObserveOptions, PreinitOptions, PreinitingStore, VexingStore,
    },
    RunData,
};

pub fn test() -> Result<()> {
    let ctx = Context::acquire()?;
    let store = {
        let preinit_opts = PreinitOptions::default();
        PreinitingStore::new(&ctx)?.preinit(preinit_opts)?.init()?
    };
    run_tests(&ctx, &store)
}

pub fn run_tests(ctx: &Context, store: &VexingStore) -> Result<RunData> {
    let event = TestEvent;
    let handler_module = HandlerModule::new();
    let observe_opts = ObserveOptions {
        action: Action::Vexing(event.kind()),
        query_cache: &QueryCache::new(),
        ignore_markers: None,
    };
    store.observers_for(TestEvent.kind()).observe(
        &handler_module,
        handler_module.heap().alloc(event),
        observe_opts,
    )?;
    let frozen_heap = FrozenHeap::new();
    let mut intents = vec![];
    handler_module
        .into_intents_on(&frozen_heap)
        .into_iter()
        .for_each(|intent| match intent {
            _ => todo!(),
        });

    Ok(RunData {
        irritations,
        num_files_scanned: store.len(),
    })
}
