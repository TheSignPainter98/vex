pub struct PrintHandler;

impl starlark::PrintHandler for PrintHandler {
    fn println(&self, text: &str) -> anyhow::Result<()> {
        println!("{text}");
        Ok(())
    }
}
