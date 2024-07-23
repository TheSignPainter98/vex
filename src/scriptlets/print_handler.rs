pub struct PrintHandler<'prefix> {
    prefix: &'prefix str,
}

impl<'prefix> PrintHandler<'prefix> {
    pub fn new(prefix: &'prefix str) -> Self {
        Self { prefix }
    }
}

impl starlark::PrintHandler for PrintHandler<'_> {
    fn println(&self, text: &str) -> anyhow::Result<()> {
        println!("[{}]: {text}", self.prefix);
        Ok(())
    }
}
