use std::{process::ExitCode, sync::Mutex};

use annotate_snippets::{AnnotationType, Renderer, Snippet};
use log::{kv::Key, Level, Log, Metadata, Record};

use crate::verbosity::Verbosity;

pub fn init(level: Verbosity) -> anyhow::Result<()> {
    let level = level.into();
    log::set_boxed_logger(Box::new(Logger {
        level,
        num_errs: Mutex::new(0),
        num_warnings: Mutex::new(0),
    }))?;
    log::set_max_level(level.to_level_filter());
    Ok(())
}

pub fn report() -> ExitCode {
    todo!()
}

struct Logger {
    level: Level,
    num_errs: Mutex<u64>,
    num_warnings: Mutex<u64>,
}

impl Logger {
    #[allow(unused)]
    fn report(&self) -> ExitCode {
        if *self.num_errs.lock().expect("failed to lock num_errs") > 0 {
            ExitCode::from(u8::MAX)
        } else if *self
            .num_warnings
            .lock()
            .expect("failed to lock num_warnings")
            > 0
        {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    }
}

impl Log for Logger {
    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    #[inline]
    fn log(&self, record: &Record<'_>) {
        let metadata = record.metadata();
        if !self.enabled(metadata) {
            return;
        }

        let level = metadata.level();

        let kvs = record.key_values();
        if level >= Level::Trace {
            eprintln!("trace: {}", record.args());
        } else if kvs.get(Key::from_str("custom")).is_some() {
            eprintln!("{}", record.args())
        } else {
            let id = kvs.get(Key::from_str("id")).map(|v| v.to_string());
            let label = record.args().to_string();
            let snippet = Snippet {
                title: Some(annotate_snippets::Annotation {
                    id: id.as_deref(),
                    label: Some(&label),
                    annotation_type: annotation_type_of(level),
                }),
                footer: Vec::with_capacity(0),
                slices: Vec::with_capacity(0),
            };
            eprintln!("{}", render_snippet(snippet));
        };

        match level {
            Level::Error => *self.num_errs.lock().expect("failed to lock num_errs") += 1,
            Level::Warn => {
                *self
                    .num_warnings
                    .lock()
                    .expect("failed to lock num_warnings") += 1
            }
            _ => {}
        }
    }

    fn flush(&self) {}
}

fn annotation_type_of(level: Level) -> AnnotationType {
    match level {
        Level::Trace => AnnotationType::Note,
        Level::Error => AnnotationType::Error,
        Level::Info => AnnotationType::Info,
        Level::Warn => AnnotationType::Warning,
        Level::Debug => AnnotationType::Help,
    }
}

pub fn render_snippet(snippet: Snippet) -> String {
    Renderer::styled().render(snippet).to_string()
}
