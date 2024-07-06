use std::{env, hint};

use indicatif::{ProgressIterator, ProgressStyle};

use crate::result::Result;

pub fn profile(name: &str, f: impl Fn() -> Result<()>) -> Result<()> {
    const REPS_ENV_VAR: &str = "VEX_PROFILE_REPS";
    let reps: u32 = env::var(REPS_ENV_VAR)
        .map(|v| v.parse().expect("internal error: failed to parse integer"))
        .unwrap_or(1000);
    println!("repeating '{name}' {reps} times...\nadjust how many by setting `${REPS_ENV_VAR}`");

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
        .try_for_each(|_| hint::black_box(f()))?;

    if let Ok(report) = guard.report().build() {
        let output_file_name = format!("flamegraph-{name}.svg");
        report
            .flamegraph(
                std::fs::File::create(&output_file_name)
                    .expect("internal error: failed to create file"),
            )
            .expect("internal error: failed to write flamegraph");
        println!("flamegraph written to {output_file_name}");
    }

    Ok(())
}
