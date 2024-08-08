use std::{cmp, env, fmt::Display, iter, process};

use camino::Utf8PathBuf;
use clap::{
    builder::{
        styling::{AnsiColor, Color, Style},
        StringValueParser, Styles, TypedValueParser,
    },
    ArgAction, Parser, Subcommand, ValueEnum,
};

use crate::{error::Error, supported_language::SupportedLanguage};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about,
    disable_help_flag = true,
    disable_version_flag = true,
    styles=Self::styles(),
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Use quiet output
    #[arg(short, global = true, conflicts_with = "verbosity_level")]
    pub quiet: bool,

    /// Use verbose output (-vv very verbose)
    #[arg(short, action=ArgAction::Count, value_name="level", global=true)]
    pub verbosity_level: u8,

    /// Print help information, use `--help` for more detail
    #[arg(short, long, action=ArgAction::Help, global=true)]
    help: Option<bool>,

    /// Print version
    #[arg(long, action=ArgAction::Version)]
    version: Option<bool>,
}

impl Args {
    pub fn parse() -> Self {
        parse_overrides();
        <Self as Parser>::parse()
    }

    fn styles() -> Styles {
        let header_style = Style::new()
            .bold()
            .fg_color(Color::Ansi(AnsiColor::Green).into());
        let literal_style = Style::new().fg_color(Color::Ansi(AnsiColor::Cyan).into());
        let command_style = literal_style.bold();
        Styles::styled()
            .header(header_style)
            .error(
                Style::new()
                    .bold()
                    .fg_color(Color::Ansi(AnsiColor::Red).into()),
            )
            .usage(header_style)
            .literal(command_style)
            .placeholder(literal_style)
            .valid(literal_style)
            .invalid(
                Style::new()
                    .bold()
                    .fg_color(Color::Ansi(AnsiColor::Yellow).into()),
            )
    }
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

    /// Print the syntax tree of the given file
    Dump(DumpCmd),

    /// Create new vex project with this directory as the root
    Init(InitCmd),

    /// Print lists of things vex knows about
    List(ListCmd),

    /// Test available lints
    Test,
}

#[cfg(test)]
impl Command {
    pub fn into_check_cmd(self) -> Option<CheckCmd> {
        match self {
            Self::Check(c) => Some(c),
            _ => None,
        }
    }

    pub fn into_dump_cmd(self) -> Option<DumpCmd> {
        match self {
            Self::Dump(p) => Some(p),
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

const OVERRIDES: [(&[u8], &[u8]); 2] = [
    (
        &[0x96, 0x8c],
        &[
            0xee, 0xff, 0x9e, 0xdf, 0x8f, 0x9a, 0x9b, 0x9e, 0x91, 0x8b, 0xd8, 0x8c, 0xdf, 0x9b,
            0x8d, 0x9a, 0x9e, 0x92, 0xf5,
        ],
    ),
    (
        &[0x88, 0x96, 0x91, 0x91, 0x96, 0x9a],
        &[
            0x9f, 0xf7, 0xf5, 0xdf, 0x0e, 0xc5, 0xd2, 0x02, 0xc5, 0xd1, 0xdf, 0x1d, 0xd1, 0x01,
            0xf5, 0xdf, 0x0b, 0xd2, 0xdc, 0xda, 0xd5, 0xd4, 0xc2, 0x01, 0xd4, 0xdc, 0xda, 0xd5,
            0xd1, 0xdf, 0x16, 0xd2, 0xd5, 0xda, 0xdc, 0xd5, 0x01, 0xdc, 0x01, 0xd5, 0xd1, 0xf5,
            0xdf, 0x0a, 0xd5, 0xda, 0xc2, 0xc5, 0x07, 0xd4, 0xbf, 0xc5, 0xdf, 0x04, 0xd1, 0xc5,
            0xd2, 0xc2, 0x04, 0xd2, 0x01, 0xc5, 0x01, 0xdf, 0x03, 0xd5, 0xda, 0xc2, 0xc5, 0x05,
            0xd4, 0xbf, 0xc5, 0xf5, 0xdf, 0x0a, 0xbf, 0xc2, 0xc5, 0x09, 0xd5, 0xda, 0xc2, 0xd4,
            0xdc, 0x02, 0xd5, 0xd4, 0xc2, 0x01, 0xd2, 0x03, 0xc2, 0x01, 0xd4, 0xd5, 0xdc, 0x02,
            0xd4, 0xbf, 0xd2, 0xc5, 0x07, 0xd5, 0xdc, 0xf5, 0xdf, 0x0a, 0xbf, 0xc2, 0xc5, 0x0a,
            0xd4, 0x01, 0xd2, 0xc5, 0x10, 0xd2, 0xc2, 0xdc, 0xda, 0xd5, 0xc5, 0x06, 0xda, 0xd4,
            0xf5, 0xdf, 0x0a, 0xd4, 0xda, 0xc5, 0x29, 0xd4, 0xbf, 0xd1, 0xf5, 0xdf, 0x0b, 0xd5,
            0xda, 0xd2, 0xc5, 0x26, 0xc2, 0xdc, 0xbf, 0xd1, 0xf5, 0xdf, 0x0b, 0xd5, 0xbf, 0xd2,
            0xc5, 0x28, 0xbf, 0xd2, 0xf5, 0xdf, 0x0a, 0xd5, 0xda, 0xc5, 0x0c, 0xd2, 0xc2, 0xd4,
            0x04, 0xd5, 0xd4, 0x02, 0xd5, 0xda, 0xc2, 0xc5, 0x0f, 0xda, 0xd4, 0xd5, 0xf5, 0xdf,
            0x0a, 0xbf, 0xc2, 0xc5, 0x0d, 0xd2, 0x01, 0xd5, 0x01, 0xd4, 0xc2, 0xd2, 0x01, 0xc2,
            0xd4, 0xd5, 0x01, 0xc2, 0xc5, 0x04, 0xd2, 0xd4, 0x01, 0xd5, 0xbf, 0x01, 0xda, 0xd5,
            0xd4, 0x01, 0xdc, 0xbf, 0xd5, 0xd2, 0xf5, 0xdf, 0x09, 0xd2, 0xbf, 0xc5, 0x0f, 0xbf,
            0xc5, 0xdf, 0x01, 0xc2, 0xd5, 0x01, 0xd2, 0xdf, 0x02, 0xd4, 0xdc, 0xc5, 0x02, 0xd4,
            0x01, 0xc5, 0xd2, 0xda, 0xd2, 0xdf, 0x01, 0xd2, 0xd4, 0xc2, 0xdf, 0x01, 0xc2, 0xdc,
            0xc5, 0xf5, 0xdf, 0x09, 0xc2, 0xdc, 0xc5, 0x0f, 0xbf, 0xdf, 0x02, 0xda, 0xbf, 0x01,
            0xdc, 0xdf, 0x03, 0xd5, 0xc2, 0xc5, 0xd4, 0xbf, 0xd2, 0xc5, 0xd5, 0xc5, 0xdf, 0x01,
            0xc2, 0xbf, 0x02, 0xd2, 0xdf, 0x02, 0xbf, 0xdc, 0x01, 0xc2, 0xc5, 0xf5, 0xdf, 0x09,
            0xdc, 0x01, 0xc5, 0x0f, 0xd4, 0xdc, 0xd4, 0xd2, 0xc5, 0xd1, 0xc5, 0xdf, 0xd1, 0xc5,
            0xc2, 0xd5, 0xdc, 0xd2, 0xc5, 0x01, 0xdc, 0x01, 0xd4, 0xc2, 0xdc, 0xd4, 0xc5, 0xd1,
            0xc5, 0x01, 0xd1, 0xdf, 0x02, 0xc2, 0xdc, 0xc5, 0xd2, 0xc2, 0xd5, 0xbf, 0xd5, 0xd1,
            0xf5, 0xdf, 0x08, 0xd4, 0xbf, 0xd2, 0xc5, 0x12, 0xd2, 0xc2, 0xd4, 0x03, 0xc2, 0xd2,
            0xc5, 0x05, 0xc2, 0xdc, 0x01, 0xda, 0xdc, 0x02, 0xd5, 0xd4, 0xc2, 0x01, 0xd4, 0xd5,
            0xc2, 0xc5, 0x05, 0xd5, 0xbf, 0xd2, 0xf5, 0xdf, 0x07, 0xc5, 0xbf, 0xd2, 0xc5, 0x25,
            0xd2, 0xd5, 0xda, 0xbf, 0x01, 0xda, 0xd5, 0xd2, 0xc5, 0x08, 0xd4, 0xbf, 0xd1, 0xf5,
            0xdf, 0x07, 0xd4, 0xdc, 0xc5, 0x26, 0xda, 0xbf, 0x06, 0xd5, 0xc5, 0x08, 0xda, 0xd4,
            0xf5, 0xdf, 0x07, 0xd4, 0xdc, 0xc5, 0x26, 0xd4, 0xda, 0xbf, 0x03, 0xda, 0xd4, 0xbf,
            0xd2, 0xc5, 0x07, 0xd4, 0xda, 0xf5, 0xdf, 0x03, 0xd1, 0xc5, 0x02, 0xd4, 0xbf, 0xd2,
            0xc5, 0x27, 0xd2, 0x03, 0xc5, 0x01, 0xbf, 0xc2, 0xc5, 0x07, 0xd2, 0xbf, 0xf5, 0xdf,
            0x02, 0xc2, 0xbf, 0x01, 0xda, 0x02, 0xbf, 0xdc, 0xc5, 0x2c, 0xd5, 0xbf, 0xc5, 0x08,
            0xc2, 0xbf, 0xf5, 0xdf, 0x02, 0xdc, 0xda, 0xd5, 0x02, 0xbf, 0x02, 0xd4, 0xc5, 0x2a,
            0xc2, 0xbf, 0xc2, 0xc5, 0x08, 0xd5, 0xdc, 0xf5, 0xdf, 0x02, 0xdc, 0xda, 0xd5, 0x03,
            0xdc, 0xda, 0xbf, 0xdc, 0xd2, 0xc5, 0x27, 0xc2, 0xbf, 0xd5, 0xc5, 0x08, 0xd2, 0xbf,
            0xd2, 0xf5, 0xdf, 0x02, 0xd2, 0xbf, 0xd5, 0x06, 0xdc, 0xbf, 0xda, 0xd4, 0xd2, 0xc5,
            0x12, 0xd2, 0xc2, 0xd4, 0xd5, 0x01, 0xdc, 0x02, 0xd5, 0x01, 0xd4, 0x02, 0xc2, 0x02,
            0xd5, 0xdc, 0xbf, 0xd4, 0xc5, 0x08, 0xd2, 0xda, 0xd5, 0xf5, 0xdf, 0x02, 0xd4, 0xbf,
            0xd5, 0x09, 0xda, 0xbf, 0xda, 0xd5, 0xc2, 0xc5, 0x0e, 0xd2, 0xbf, 0x02, 0xda, 0xd5,
            0xd4, 0xd5, 0xda, 0xbf, 0x01, 0xda, 0xd4, 0xc2, 0x04, 0xd2, 0xc5, 0x09, 0xc2, 0xbf,
            0xd4, 0xf5, 0xdf, 0x01, 0xc5, 0xbf, 0xdc, 0xd5, 0x0c, 0xdc, 0xda, 0xbf, 0xda, 0xd5,
            0xd4, 0xc5, 0x0b, 0xc2, 0xd5, 0xdc, 0x01, 0xd5, 0x01, 0xdc, 0xd5, 0xda, 0x03, 0xd5,
            0x01, 0xd2, 0xc5, 0x0b, 0xd5, 0xbf, 0xd2, 0xf5, 0xdf, 0xd1, 0xd5, 0xbf, 0xda, 0xd5,
            0x10, 0xdc, 0xda, 0xbf, 0xd5, 0xc2, 0xc5, 0x13, 0xc2, 0xda, 0xdc, 0xd2, 0xc5, 0x09,
            0xd4, 0xbf, 0xd5, 0xd1, 0xf5, 0xdc, 0xbf, 0xdc, 0xd5, 0x15, 0xdc, 0xda, 0xbf, 0xda,
            0xd5, 0xc2, 0xc5, 0x1a, 0xd2, 0xd5, 0xbf, 0xdc, 0xc5, 0xf5, 0xdc, 0xd5, 0x1b, 0xdc,
            0xda, 0x03, 0xdc, 0xd5, 0xd4, 0xd2, 0xc5, 0x0e, 0xd2, 0xd4, 0xd5, 0xdc, 0xda, 0x01,
            0xdc, 0xbf, 0xc2, 0xf5, 0xd5, 0x23, 0xdc, 0xda, 0x02, 0xd5, 0xd2, 0xc5, 0x03, 0xd2,
            0x01, 0xc2, 0xd4, 0xdc, 0xda, 0xbf, 0x02, 0xda, 0xd5, 0x03, 0xbf, 0x01, 0xdc, 0xc5,
            0xf5, 0xd5, 0x27, 0xdc, 0xda, 0xbf, 0x01, 0xda, 0xdc, 0xd5, 0xd4, 0x01, 0xc2, 0x01,
            0xbf, 0x02, 0xdc, 0xd5, 0x04, 0xbf, 0xdc, 0x01, 0xbf, 0xdc, 0xc5, 0xf5, 0xd5, 0x2a,
            0xdc, 0xda, 0xbf, 0xd5, 0xc2, 0xd2, 0x01, 0xd4, 0xbf, 0xda, 0xd5, 0x06, 0xda, 0xbf,
            0xd5, 0x01, 0xdc, 0xbf, 0xdc, 0xc5, 0xf5, 0xd5, 0x15, 0xdc, 0xda, 0xbf, 0xd5, 0x14,
            0xdc, 0x02, 0xda, 0x01, 0xdc, 0xd5, 0x08, 0xbf, 0x01, 0xd5, 0x02, 0xdc, 0xda, 0x01,
            0xc2, 0xf5, 0xd5, 0x13, 0xdc, 0xbf, 0x01, 0xdc, 0xd5, 0x24, 0xdc, 0xbf, 0xdc, 0xd5,
            0x04, 0xda, 0x01, 0xd1, 0xf5, 0x01, 0xbe, 0x8c, 0x94, 0xdf, 0x91, 0x90, 0x8b, 0xdf,
            0x88, 0x97, 0x9e, 0x8b, 0xdf, 0x98, 0x90, 0x01, 0x9b, 0xdf, 0x9c, 0x90, 0x9b, 0x9a,
            0xdf, 0x9c, 0x9e, 0x91, 0xdf, 0x9b, 0x90, 0xdf, 0x99, 0x90, 0x8d, 0xdf, 0x86, 0x90,
            0x8a, 0xc4, 0xdf, 0x9e, 0x8c, 0x94, 0xdf, 0x99, 0x90, 0x8d, 0xdf, 0x99, 0x90, 0x8d,
            0x98, 0x96, 0x89, 0x9a, 0x91, 0x9a, 0x8c, 0x01, 0xd1, 0xf5, 0xff,
        ],
    ),
];

#[derive(Debug, Default, PartialEq, Eq, Parser)]
pub struct DumpCmd {
    /// File to parse
    #[arg(value_name = "file")]
    pub path: Utf8PathBuf,

    /// Remove location info, line-breaks and indentation
    #[arg(long)]
    pub compact: bool,

    /// Override language detection
    #[arg(long = "as", value_name = "language")]
    pub language: Option<SupportedLanguage>,
}

#[derive(Debug, Default, PartialEq, Eq, Parser)]
pub struct InitCmd {
    /// Force init
    #[arg(long)]
    pub force: bool,
}

fn parse_overrides() {
    if env::args().count() > 2 {
        return;
    }
    let Some(cmd) = env::args().nth(1) else {
        return;
    };
    const MAX_OVERRIDE_NAME_LEN: usize = 6;
    if cmd.len() > MAX_OVERRIDE_NAME_LEN {
        return;
    }
    let cmd_buf = {
        let mut buf = [0u8; MAX_OVERRIDE_NAME_LEN];
        buf[..cmd.len()].copy_from_slice(cmd.as_bytes());
        buf[..cmd.len()].iter_mut().for_each(|b| *b = !*b);
        buf
    };
    let Some((_, r#override)) = OVERRIDES
        .iter()
        .find(|(name, _)| &cmd_buf[..cmd.len()] == *name)
    else {
        return;
    };
    let cap = !((r#override[1] as u16) << 8 | r#override[0] as u16) as usize;
    println!(
        "{}",
        String::from_utf8(
            r#override
                .windows(2)
                .skip(2)
                .map(|w| <[u8; 2]>::try_from(w).unwrap())
                .map(|[a, b]| [!a, !b])
                .filter(|[a, _]| *a as i8 >= 0)
                .fold(Vec::with_capacity(cap), |mut acc, [a, b]| {
                    acc.extend(iter::repeat(a).take(cmp::max(1, -(b as i8)) as usize));
                    acc
                }),
        )
        .unwrap()
    );

    process::exit(0);
}

#[cfg(test)]
mod test {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn consistency() {
        Args::command().debug_assert()
    }

    #[test]
    fn no_default() {
        Args::try_parse_from(["vex"]).unwrap_err();
    }

    #[test]
    fn quiet() {
        const CMD: &str = "check";
        assert!(!Args::try_parse_from(["vex", CMD]).unwrap().quiet);
        assert!(Args::try_parse_from(["vex", CMD, "-q"]).unwrap().quiet);
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

    #[test]
    fn verbosity_conflict() {
        const CMD: &str = "check";
        assert!(Args::try_parse_from(["vex", "-v", "-q", CMD]).is_err());
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

    mod dump {
        use super::*;

        #[test]
        fn requires_path() {
            Args::try_parse_from(["vex", "dump"]).unwrap_err();
        }

        #[test]
        fn relative_path() {
            const PATH: &str = "./src/main.rs";
            let args = Args::try_parse_from(["vex", "dump", PATH]).unwrap();
            let dump_cmd = args.into_command().into_dump_cmd().unwrap();
            assert_eq!(dump_cmd.path, PATH);
        }

        #[test]
        fn absolute_path() {
            const PATH: &str = "/src/main.rs";
            let args = Args::try_parse_from(["vex", "dump", PATH]).unwrap();
            let dump_cmd = args.into_command().into_dump_cmd().unwrap();
            assert_eq!(dump_cmd.path, PATH);
        }

        #[test]
        fn language() {
            let args = Args::try_parse_from(["vex", "dump", "asdf.foo", "--as", "rust"]).unwrap();
            let dump_cmd = args.into_command().into_dump_cmd().unwrap();
            assert_eq!(SupportedLanguage::Rust, dump_cmd.language.unwrap());
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

    #[test]
    fn test() {
        assert_eq!(
            Args::try_parse_from(["vex", "test"])
                .unwrap()
                .into_command(),
            Command::Test,
        )
    }
}
