use std::{
    env,
    io::{self, Write},
};

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    associations::Associations,
    cli::ParseCmd,
    context::Context,
    error::{Error, IOAction},
    result::Result,
    scriptlets::Node,
    source_file::SourceFile,
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

    let root = Node::new(src_file.tree.root_node(), &src_file);
    PrettyFormatter::new(cmd.compact).write(&io::stdout().lock(), root)?;

    Ok(())
}

struct PrettyFormatter {
    curr_indent: Option<u32>,
}

impl PrettyFormatter {
    fn new(compact: bool) -> Self {
        let curr_indent = if compact { None } else { Some(0) };
        Self { curr_indent }
    }

    fn write(&mut self, out: &impl Write, node: Node<'_>) -> Result<()> {
        unimplemented!()
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

    use crate::cli::Args;

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
}
