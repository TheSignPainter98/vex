use crate::verbosity::Verbosity;

#[derive(Debug)]
pub struct PrintHandler<'prefix> {
    verbosity: Verbosity,
    prefix: &'prefix str,
}

impl<'prefix> PrintHandler<'prefix> {
    pub fn new(verbosity: Verbosity, prefix: &'prefix str) -> Self {
        Self { verbosity, prefix }
    }
}

impl starlark::PrintHandler for PrintHandler<'_> {
    fn println(&self, text: &str) -> anyhow::Result<()> {
        let Self { verbosity, prefix } = self;

        if verbosity.is_quiet() {
            return Ok(());
        }

        println!("[{prefix}]: {text}");
        Ok(())
    }
}
