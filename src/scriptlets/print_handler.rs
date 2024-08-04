use crate::verbosity::Verbosity;

pub struct PrintHandler<'prefix> {
    quiet: bool,
    prefix: &'prefix str,
}

impl<'prefix> PrintHandler<'prefix> {
    pub fn new(verbosity: Verbosity, prefix: &'prefix str) -> Self {
        let quiet = verbosity.is_quiet();
        Self { quiet, prefix }
    }
}

impl starlark::PrintHandler for PrintHandler<'_> {
    fn println(&self, text: &str) -> anyhow::Result<()> {
        if !self.quiet {
            println!("[{}]: {text}", self.prefix);
        }
        Ok(())
    }
}
