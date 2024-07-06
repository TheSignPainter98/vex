use std::env;

use indicatif::{ProgressIterator, ProgressStyle};

pub fn profile<E>(f: impl Fn() -> Result<(), E>) -> Result<(), E> {
    const REPS_ENV_VAR: &str = "VEX_PROFILE_REPS";
    println!("repeating the operations many times, adjust how many by setting `${REPS_ENV_VAR}`");
    let reps: u32 = env::var(REPS_ENV_VAR)
        .as_deref()
        .unwrap_or("100")
        .parse()
        .unwrap();

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .build()
        .expect("internal error: failed to build profile guard");

    let progress_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {wide_bar:.green/red} {human_pos:>7}/{human_len:7} {msg}")
        .expect("internal error: failed to parse progress template")
        .progress_chars("=>-");
    (0..reps)
        .progress_with_style(progress_style)
        .try_for_each(|_| f())?;

    if let Ok(report) = guard.report().build() {
        const OUTPUT_FILE_NAME: &str = "flamegraph.svg";
        report
            .flamegraph(
                std::fs::File::create(OUTPUT_FILE_NAME)
                    .expect("internal error: failed to create file"),
            )
            .expect("internal error: failed to write flamegraph");
        println!("flamegraph written to {OUTPUT_FILE_NAME}");
    }

    Ok(())
}
