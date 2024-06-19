use std::{env, fmt::Write};

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    associations::Associations,
    cli::ParseCmd,
    context::Context,
    error::{Error, IOAction},
    result::Result,
    scriptlets::{Location, Node},
    source_file::{ParsedSourceFile, SourceFile},
    source_path::{PrettyPath, SourcePath},
};

pub fn parse(cmd: ParseCmd) -> Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir().map_err(|e| Error::IO {
        path: PrettyPath::new(Utf8Path::new(&cmd.path)),
        action: IOAction::Read,
        cause: e,
    })?)?;
    let src_path = SourcePath::new_in(&cmd.path, &cwd);
    let language = match cmd.language {
        Some(l) => Some(l),
        None => Context::acquire()
            .ok()
            .map(|ctx| ctx.associations())
            .transpose()?
            .unwrap_or_else(Associations::base)
            .get_language(&src_path)?,
    };
    let src_file = SourceFile::new(src_path, language)?.parse()?;

    let capacity_estimate = 20 * src_file.tree.root_node().descendant_count();
    let mut buf = String::with_capacity(capacity_estimate);
    PrettyFormatter::new(cmd.compact).write(&mut buf, &src_file)?;
    println!("{buf}");

    Ok(())
}

struct PrettyFormatter {
    compact: bool,
    curr_indent: u32,
}

impl PrettyFormatter {
    fn new(compact: bool) -> Self {
        let curr_indent = 0;
        Self {
            compact,
            curr_indent,
        }
    }

    fn write(&mut self, w: &mut impl Write, src_file: &ParsedSourceFile) -> Result<()> {
        let root = Node::new(src_file.tree.root_node(), src_file);
        self.write_node(w, root, None)
    }

    fn write_node(
        &mut self,
        w: &mut impl Write,
        node: Node<'_>,
        field_name: Option<&'static str>,
    ) -> Result<()> {
        let expandable_separator = if self.compact { ' ' } else { '\n' };

        self.write_indent(w)?;
        if let Some(field_name) = field_name {
            write!(w, "{field_name}: ").unwrap();
        }
        write!(w, "(").unwrap();
        if node.is_named() {
            write!(w, "{}", node.grammar_name()).unwrap();
        } else {
            write!(w, r#""{}""#, node.grammar_name()).unwrap();
        }

        self.curr_indent += 1;
        node.children(&mut node.walk())
            .enumerate()
            .try_for_each(|(i, child)| {
                write!(w, "{expandable_separator}").unwrap();
                let field_name = node.field_name_for_child(i as u32);
                self.write_node(w, Node::new(child, node.source_file), field_name)
            })?;
        self.curr_indent -= 1;

        if !self.compact && node.child_count() != 0 {
            write!(w, "{expandable_separator}").unwrap();
            self.write_indent(w)?;
        }
        write!(w, ")").unwrap();
        self.write_location(w, &Location::of(&node))?;
        Ok(())
    }

    fn write_indent(&self, w: &mut impl Write) -> Result<()> {
        if self.compact {
            return Ok(());
        }

        (0..self.curr_indent)
            .try_for_each(|_| write!(w, "  "))
            .unwrap();
        Ok(())
    }

    fn write_location(&self, w: &mut impl Write, loc: &Location) -> Result<()> {
        if self.compact {
            return Ok(());
        }

        write!(w, " ; {loc}").unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs::{self, File},
        io::Write,
        path,
    };

    use clap::Parser;
    use indoc::indoc;
    use tempfile::TempDir;

    use crate::{cli::Args, supported_language::SupportedLanguage};

    use super::*;

    struct TestFile {
        _dir: TempDir,
        path: Utf8PathBuf,
    }

    impl TestFile {
        fn new(path: impl AsRef<str>, content: impl AsRef<[u8]>) -> TestFile {
            let dir = tempfile::tempdir().unwrap();
            let file_path = Utf8PathBuf::try_from(dir.path().to_path_buf())
                .unwrap()
                .join(path.as_ref());

            fs::create_dir_all(file_path.parent().unwrap()).unwrap();
            File::create(&file_path)
                .unwrap()
                .write_all(content.as_ref())
                .unwrap();

            TestFile {
                _dir: dir,
                path: file_path,
            }
        }
    }

    #[test]
    fn parse_valid_file() {
        let test_file = TestFile::new(
            "path/to/file.rs",
            indoc! {r#"
                fn add(a: i32, b: i32) -> i32 {
                    a + b
                }
            "#},
        );

        let args = Args::try_parse_from(["vex", "parse", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_parse_cmd().unwrap();
        parse(cmd).unwrap();
    }

    #[test]
    fn parse_nonexistent_file() {
        let file_path = "/i/do/not/exist.rs";
        let args = Args::try_parse_from(["vex", "parse", file_path]).unwrap();
        let cmd = args.command.into_parse_cmd().unwrap();
        let err = parse(cmd).unwrap_err();
        if cfg!(target_os = "windows") {
            assert_eq!(
                err.to_string(),
                "cannot read /i/do/not/exist.rs: The system cannot find the path specified. (os error 3)"
            );
        } else {
            assert_eq!(
                err.to_string(),
                "cannot read /i/do/not/exist.rs: No such file or directory (os error 2)"
            );
        }
    }

    #[test]
    fn parse_invalid_file() {
        let test_file = TestFile::new(
            "src/file.rs",
            indoc! {r#"
                i am not valid a valid rust file!
            "#},
        );
        let args = Args::try_parse_from(["vex", "parse", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_parse_cmd().unwrap();
        let err = parse(cmd).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "cannot parse {} as rust",
                test_file.path.as_str().replace(path::MAIN_SEPARATOR, "/")
            )
        );
    }

    #[test]
    fn no_extension() {
        let test_file = TestFile::new("no-extension", "");
        let args = Args::try_parse_from(["vex", "parse", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_parse_cmd().unwrap();
        let err = parse(cmd).unwrap_err();

        // Assertion relaxed due to strange Github Actions Windows and Macos runner path handling.
        let expected = format!(
            "cannot discern language of {}",
            PrettyPath::new(&test_file.path)
        );
        assert!(
            err.to_string().ends_with(&expected),
            "unexpected error: expected {expected} but got {err}"
        );
    }

    #[test]
    fn unknown_extension() {
        let test_file = TestFile::new("file.unknown-extension", "");
        let args = Args::try_parse_from(["vex", "parse", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_parse_cmd().unwrap();
        let err = parse(cmd).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "cannot discern language of {}",
                PrettyPath::new(&test_file.path)
            )
        );
    }

    #[test]
    fn format() {
        let test_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("test.rs".into(), "".into()),
            "const X: usize = 1 + 2;",
            SupportedLanguage::Rust,
        )
        .unwrap();

        let compact_fmt = {
            let mut compact_fmt = String::new();
            PrettyFormatter::new(true)
                .write(&mut compact_fmt, &test_file)
                .unwrap();
            compact_fmt
        };
        let expanded_fmt = {
            let mut expanded_fmt = String::new();
            PrettyFormatter::new(false)
                .write(&mut expanded_fmt, &test_file)
                .unwrap();
            expanded_fmt
        };
        assert!(compact_fmt.len() < expanded_fmt.len());
        assert!(!compact_fmt.contains('\n'));
        assert!(expanded_fmt.contains('\n'));
    }
}
