use std::fmt::Display;

use camino::Utf8PathBuf;
use clap::{
    builder::{StringValueParser, TypedValueParser},
    ArgAction, Parser, Subcommand, ValueEnum,
};

use crate::{error::Error, supported_language::SupportedLanguage};

#[derive(Debug, Parser)]
#[command(author, version, about, disable_help_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Use verbose output (-vv very verbose)
    #[arg(short, action=ArgAction::Count, value_name="level", global=true)]
    pub verbosity_level: u8,

    /// Print help information, use `--help` for more detail
    #[arg(short, long, action=ArgAction::Help, global=true)]
    help: Option<bool>,
}

#[cfg(test)]
impl Args {
    fn into_command(self) -> Command {
        self.command
    }
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Check this project for lint
    Check(CheckCmd),

    /// Print lists of things vex knows about
    List(ListCmd),

    /// Create new vex project with this directory as the root
    Init(InitCmd),

    /// Print the syntax tree of the given file
    Parse(ParseCmd),
}

#[cfg(test)]
impl Command {
    pub fn into_check_cmd(self) -> Option<CheckCmd> {
        match self {
            Self::Check(c) => Some(c),
            _ => None,
        }
    }

    pub fn into_parse_cmd(self) -> Option<ParseCmd> {
        match self {
            Self::Parse(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_init_cmd(self) -> Option<InitCmd> {
        match self {
            Self::Init(i) => Some(i),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Parser)]
pub struct ListCmd {
    /// What to print
    #[arg(value_name = "what")]
    pub what: ToList,
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ToList {
    Checks,
    Languages,
}

#[derive(Debug, Default, PartialEq, Eq, Parser)]
pub struct CheckCmd {
    // Set concurrency limit
    // #[arg(long, default_value_t = MaxConcurrentFileLimit::default(), value_parser = MaxConcurrentFileLimit::parser())]
    // pub max_concurrent_files: MaxConcurrentFileLimit,
    /// Reduce strictness
    #[arg(long)]
    pub lenient: bool,

    /// Exit early after this many problems (pass `unlimited` for no max)
    #[arg(long, default_value_t = MaxProblems::default(), value_parser = MaxProblems::parser(), value_name = "max")]
    pub max_problems: MaxProblems,
}

// #[derive(Clone, Debug)]
// pub struct MaxConcurrentFileLimit(pub u32);
//
// impl MaxConcurrentFileLimit {
//     fn parser() -> impl TypedValueParser {
//         StringValueParser::new().try_map(|s| Ok::<_, Error>(Self(s.parse()?)))
//     }
// }
//
// impl Default for MaxConcurrentFileLimit {
//     fn default() -> Self {
//         Self(16)
//     }
// }
//
// impl Display for MaxConcurrentFileLimit {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         self.0.fmt(f)
//     }
// }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaxProblems {
    Unlimited,
    Limited(u32),
}

impl MaxProblems {
    fn parser() -> impl TypedValueParser {
        StringValueParser::new().try_map(|s| {
            if s.to_lowercase() == "unlimited" {
                return Ok(Self::Unlimited);
            }

            let max: u32 = s.parse()?;
            Ok::<_, Error>(Self::Limited(max))
        })
    }

    #[allow(unused)]
    pub fn is_exceeded_by(&self, to_check: usize) -> bool {
        match self {
            Self::Unlimited => false,
            Self::Limited(lim) => to_check >= *lim as usize,
        }
    }
}

impl Default for MaxProblems {
    fn default() -> Self {
        Self::Limited(100)
    }
}

impl Display for MaxProblems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unlimited => write!(f, "unlimited"),
            Self::Limited(l) => l.fmt(f),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Parser)]
pub struct ParseCmd {
    /// File to parse
    #[arg(value_name = "file")]
    pub path: Utf8PathBuf,

    #[arg(long = "as", value_name = "language")]
    pub language: Option<SupportedLanguage>,
}

#[derive(Debug, Default, PartialEq, Eq, Parser)]
pub struct InitCmd {
    /// Force init
    #[arg(long)]
    pub force: bool,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn no_default() {
        Args::try_parse_from(["vex"]).unwrap_err();
    }

    #[test]
    fn verbosity_level() {
        const CMD: &str = "check";
        assert_eq!(
            Args::try_parse_from(["vex", CMD]).unwrap().verbosity_level,
            0
        );
        assert_eq!(
            Args::try_parse_from(["vex", "-v", CMD])
                .unwrap()
                .verbosity_level,
            1
        );
        assert_eq!(
            Args::try_parse_from(["vex", "-vv", CMD])
                .unwrap()
                .verbosity_level,
            2
        );
        assert_eq!(
            Args::try_parse_from(["vex", "-vv", CMD])
                .unwrap()
                .verbosity_level,
            2
        );
    }

    mod list {
        use super::*;

        #[test]
        fn languages() {
            assert_eq!(
                Args::try_parse_from(["vex", "list", "languages"])
                    .unwrap()
                    .into_command(),
                Command::List(ListCmd {
                    what: ToList::Languages
                }),
            );
        }

        #[test]
        fn vexes() {
            assert_eq!(
                Args::try_parse_from(["vex", "list", "checks"])
                    .unwrap()
                    .into_command(),
                Command::List(ListCmd {
                    what: ToList::Checks
                }),
            );
        }
    }

    mod check {
        use super::*;

        #[test]
        fn default() {
            let args = Args::try_parse_from(["vex", "check"]).unwrap();
            let cmd = args.into_command();
            assert!(matches!(cmd, Command::Check(_)));

            let check_cmd = cmd.into_check_cmd().unwrap();
            assert_eq!(check_cmd.max_problems, MaxProblems::Limited(100));
        }

        #[test]
        fn finite_max_problems() {
            let args = Args::try_parse_from(["vex", "check", "--max-problems", "1000"]).unwrap();
            let cmd = args.into_command();
            assert!(matches!(cmd, Command::Check(_)));

            let check_cmd = cmd.into_check_cmd().unwrap();
            assert_eq!(check_cmd.max_problems, MaxProblems::Limited(1000));
        }

        #[test]
        fn infinite_max_problems() {
            let args =
                Args::try_parse_from(["vex", "check", "--max-problems", "unlimited"]).unwrap();
            let cmd = args.into_command();
            assert!(matches!(cmd, Command::Check(_)));

            let check_cmd = cmd.into_check_cmd().unwrap();
            assert_eq!(check_cmd.max_problems, MaxProblems::Unlimited);
        }
    }

    mod parse {
        use super::*;

        #[test]
        fn requires_path() {
            Args::try_parse_from(["vex", "parse"]).unwrap_err();
        }

        #[test]
        fn relative_path() {
            const PATH: &str = "./src/main.rs";
            let args = Args::try_parse_from(["vex", "parse", PATH]).unwrap();
            let parse_cmd = args.into_command().into_parse_cmd().unwrap();
            assert_eq!(parse_cmd.path, PATH);
        }

        #[test]
        fn absolute_path() {
            const PATH: &str = "/src/main.rs";
            let args = Args::try_parse_from(["vex", "parse", PATH]).unwrap();
            let parse_cmd = args.into_command().into_parse_cmd().unwrap();
            assert_eq!(parse_cmd.path, PATH);
        }

        #[test]
        fn language() {
            let args = Args::try_parse_from(["vex", "parse", "asdf.foo", "--as", "rust"]).unwrap();
            let parse_cmd = args.into_command().into_parse_cmd().unwrap();
            assert_eq!(SupportedLanguage::Rust, parse_cmd.language.unwrap());
        }
    }

    #[test]
    fn init() {
        assert_eq!(
            Args::try_parse_from(["vex", "init"])
                .unwrap()
                .into_command()
                .into_init_cmd()
                .unwrap(),
            InitCmd { force: false },
        );
        assert_eq!(
            Args::try_parse_from(["vex", "init", "--force"])
                .unwrap()
                .into_command()
                .into_init_cmd()
                .unwrap(),
            InitCmd { force: true },
        );
    }
}
