use std::{
    ops::Deref,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use dupe::Dupe;
use log::{info, log_enabled};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use starlark::values::FrozenHeap;
use tree_sitter::QueryCursor;

use crate::{
    active_lints::ActiveLints,
    cli::{MaxConcurrentFileLimit, MaxProblems},
    context::Context,
    irritation::Irritation,
    query::Query,
    result::Result,
    scriptlets::{
        action::Action,
        event::{EventKind, MatchEvent, OpenFileEvent, OpenProjectEvent},
        handler_module::HandlerModule,
        intents::Intent,
        query_cache::QueryCache,
        query_captures::QueryCaptures,
        Observable, ObserveOptions, Observer, PrintHandler, VexingStore,
    },
    source_file::{self, SourceFile},
    supported_language::SupportedLanguage,
    verbosity::Verbosity,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProjectRunData {
    pub irritations: Vec<Irritation>,
    pub num_files_scanned: u64,
    pub num_bytes_scanned: u64,
}

pub fn scan_project(
    ctx: &Context,
    store: &VexingStore,
    active_lints: ActiveLints,
    max_problems: MaxProblems,
    max_concurrent_files: MaxConcurrentFileLimit,
    verbosity: Verbosity,
) -> Result<ProjectRunData> {
    let files = source_file::sources_in_dir(ctx, max_concurrent_files)?;

    let project_queries_hint = store.project_queries_hint();
    let file_queries_hint = store.file_queries_hint();

    let query_cache = QueryCache::with_capacity(project_queries_hint + file_queries_hint);

    let mut irritations = vec![];
    let frozen_heap = store.frozen_heap();
    let project_queries = {
        let mut project_queries = Vec::with_capacity(project_queries_hint);

        let event = OpenProjectEvent::new(ctx.project_root.dupe());
        let handler_module = HandlerModule::new();
        let observe_opts = ObserveOptions {
            action: Action::Vexing(event.kind()),
            query_cache: Some(&query_cache),
            active_lints: Some(&active_lints),
            ignore_markers: None,
            print_handler: &PrintHandler::new(verbosity, event.kind().name()),
        };
        store.observers_for(event.kind()).observe(
            &handler_module,
            handler_module.heap().alloc(event),
            observe_opts,
        )?;
        handler_module
            .into_intents_on(frozen_heap.deref())?
            .into_iter()
            .for_each(|intent| match intent {
                Intent::Find {
                    language,
                    query,
                    on_match,
                } => project_queries.push((language, query, on_match)),
                Intent::Observe { .. } => panic!("internal error: non-init observe"),
                Intent::Warn(irr) => irritations.push(irr),
                Intent::ScanFile { .. } => {
                    panic!("internal error: unexpected ScanFile intent declared")
                }
            });
        project_queries
    };

    let total_irritations = AtomicUsize::new(0);
    let runs: Vec<_> = files
        .par_iter()
        .filter_map(|file| match file.language() {
            Some(language) => Some((file, language)),
            None => {
                if log_enabled!(log::Level::Info) {
                    info!("skipping {}", file.path());
                }
                None
            }
        })
        .map(|(file, language)| {
            let opts = VexFileOptions {
                store,
                language,
                project_queries: &project_queries,
                query_cache: &query_cache,
                active_lints: &active_lints,
                verbosity,
            };
            scan_file(file, opts)
        })
        .take_any_while(|file_scan_result| {
            let run = match file_scan_result {
                Ok(run) => run,
                Err(_) => return true,
            };
            let new_irritations = run.irritations.len();
            let prev_total_irritations = if new_irritations > 0 {
                total_irritations.fetch_add(new_irritations, Ordering::Relaxed)
            } else {
                total_irritations.load(Ordering::Relaxed)
            };
            !max_problems.is_exceeded_by(prev_total_irritations)
        })
        .collect::<Result<_>>()?;

    let num_files_scanned = runs.len() as u64;
    let num_bytes_scanned = runs.iter().map(|run| run.num_bytes_scanned).sum();
    for run in runs {
        irritations.extend(run.irritations);
    }

    irritations.sort();
    if let MaxProblems::Limited(max) = max_problems {
        let max = max as usize;
        if max < irritations.len() {
            irritations.truncate(max);
        }
    }

    Ok(ProjectRunData {
        irritations,
        num_files_scanned,
        num_bytes_scanned,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileRunData {
    pub irritations: Vec<Irritation>,
    pub num_bytes_scanned: u64,
}

pub struct VexFileOptions<'a> {
    store: &'a VexingStore,
    language: SupportedLanguage,
    project_queries: &'a [(SupportedLanguage, Arc<Query>, Observer)],
    query_cache: &'a QueryCache,
    active_lints: &'a ActiveLints,
    verbosity: Verbosity,
}

fn scan_file(file: &SourceFile, opts: VexFileOptions<'_>) -> Result<FileRunData> {
    let VexFileOptions {
        store,
        language,
        project_queries,
        query_cache,
        active_lints,
        verbosity,
    } = opts;

    let mut irritations = Vec::new();

    let frozen_heap = FrozenHeap::new();
    let file_queries = {
        let mut file_queries = Vec::with_capacity(store.file_queries_hint());
        let path = file.path().pretty_path.dupe();

        let event = OpenFileEvent::new(path);
        let handler_module = HandlerModule::new();
        let observe_opts = ObserveOptions {
            action: Action::Vexing(event.kind()),
            query_cache: Some(query_cache),
            active_lints: Some(&active_lints),
            ignore_markers: None,
            print_handler: &PrintHandler::new(verbosity, event.kind().name()),
        };
        store.observers_for(event.kind()).observe(
            &handler_module,
            handler_module.heap().alloc(event),
            observe_opts,
        )?;
        handler_module
            .into_intents_on(&frozen_heap)?
            .into_iter()
            .for_each(|intent| match intent {
                Intent::Find {
                    language,
                    query,
                    on_match,
                } => file_queries.push((language, query, on_match)),
                Intent::Observe { .. } => panic!("internal error: non-init observe"),
                Intent::Warn(irr) => irritations.push(irr.clone()),
                Intent::ScanFile { .. } => {
                    panic!("internal error: unexpected ScanFile intent declared")
                }
            });
        file_queries
    };

    if project_queries
        .iter()
        .chain(file_queries.iter())
        .all(|(l, _, _)| *l != language)
    {
        // The user did not request a scan of this type of file.
        return Ok(FileRunData {
            irritations,
            num_bytes_scanned: 0,
        });
    }

    let parsed_file = file.parse()?;
    let ignore_markers = parsed_file.ignore_markers()?;
    project_queries
        .iter()
        .chain(file_queries.iter())
        .filter(|(l, _, _)| *l == language)
        .try_for_each(|(_, query, on_match)| {
            QueryCursor::new()
                .matches(
                    query,
                    parsed_file.tree.root_node(),
                    parsed_file.content.as_bytes(),
                )
                .try_for_each(|qmatch| {
                    let handler_module = HandlerModule::new();
                    let event = {
                        let path = parsed_file.path.pretty_path.dupe();
                        let captures =
                            QueryCaptures::new(query, qmatch, &parsed_file, handler_module.heap());
                        handler_module.heap().alloc(MatchEvent::new(path, captures))
                    };
                    let observe_opts = ObserveOptions {
                        action: Action::Vexing(EventKind::Match),
                        query_cache: Some(query_cache),
                        active_lints: Some(&active_lints),
                        ignore_markers: Some(&ignore_markers),
                        print_handler: &PrintHandler::new(verbosity, EventKind::Match.name()),
                    };
                    on_match.observe(&handler_module, event, observe_opts)?;
                    handler_module
                        .into_intents_on(&frozen_heap)?
                        .into_iter()
                        .for_each(|intent| match intent {
                            Intent::Find { .. } => {
                                panic!("internal error: find intended during find")
                            }
                            Intent::Observe { .. } => {
                                panic!("internal error: non-init observe")
                            }
                            Intent::Warn(irr) => irritations.push(irr),
                            Intent::ScanFile { .. } => {
                                panic!("internal error: unexpected ScanFile intent declared")
                            }
                        });

                    Result::Ok(())
                })
        })?;
    let num_bytes_scanned = parsed_file.content.len() as u64;
    Ok(FileRunData {
        irritations,
        num_bytes_scanned,
    })
}
