use std::fmt::Display;

use clap::{
    builder::{StringValueParser, TypedValueParser},
    ArgAction, Parser, Subcommand,
};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about,
    disable_help_flag = true,
    disable_version_flag = true
)]
// #[warn(missing_docs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(short, value_name="level", action=ArgAction::Count, value_name="level", global=true)]
    pub verbosity_level: u8,

    /// Print help information, use `--help` for more detail
    #[arg(short, long, action=ArgAction::Help, global=true)]
    pub help: Option<bool>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(name = "languages")]
    ListLanguages,

    #[command(name = "lints")]
    ListLints,

    Check(CheckCmd),

    Ignore(IgnoreCmd),

    Init,
}

impl Default for Command {
    fn default() -> Self {
        Self::Check(CheckCmd::default())
    }
}

#[derive(Debug, Default, Parser)]
pub struct CheckCmd {
    #[arg(long, default_value_t = MaxConcurrentFileLimit::default(), value_parser = MaxConcurrentFileLimit::parser())]
    pub max_concurrent_files: MaxConcurrentFileLimit,

    #[arg(long, default_value_t = MaxProblems::default(), value_parser = MaxProblems::parser())]
    pub max_problems: MaxProblems,
}

#[derive(Clone, Debug)]
pub struct MaxConcurrentFileLimit(pub u32);

impl MaxConcurrentFileLimit {
    fn parser() -> impl TypedValueParser {
        StringValueParser::new().try_map(|s| Ok::<_, anyhow::Error>(Self(s.parse()?)))
    }
}

impl Default for MaxConcurrentFileLimit {
    fn default() -> Self {
        Self(16)
    }
}

impl Display for MaxConcurrentFileLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug)]
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
            Ok::<_, anyhow::Error>(Self::Limited(max))
        })
    }

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

#[derive(Clone, Debug, clap::Args)]
pub struct IgnoreCmd {
    pub kind: IgnoreKind,

    #[arg(required = true)]
    pub to_ignore: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, clap::ValueEnum)]
pub enum IgnoreKind {
    Extension,
    Language,
    Dir,
}
