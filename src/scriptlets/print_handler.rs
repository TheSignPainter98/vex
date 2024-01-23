use derive_new::new;

#[derive(new)]
pub struct PrintHandler<S> {
    tag: S,
}

impl<S> starlark::PrintHandler for PrintHandler<S>
where
    S: AsRef<str>,
{
    fn println(&self, text: &str) -> anyhow::Result<()> {
        println!("{}: {text}", self.tag.as_ref());
        Ok(())
    }
}
