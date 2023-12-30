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
