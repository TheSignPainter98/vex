use clap::{error::ErrorKind as ClapErrorKind, CommandFactory};
use stderrlog::LogLevelNum;

use crate::cli::Args;

#[derive(Copy, Clone, Debug, Default)]
pub enum Verbosity {
    #[default]
    Terse,
    Verbose,
    Trace,
}

impl Verbosity {
    pub fn log_level_num(self) -> LogLevelNum {
        match self {
            Self::Terse => LogLevelNum::Warn,
            Self::Verbose => LogLevelNum::Info,
            Self::Trace => LogLevelNum::Trace,
        }
    }
}

impl TryFrom<u8> for Verbosity {
    type Error = clap::Error;

    fn try_from(ctr: u8) -> Result<Self, Self::Error> {
        match ctr {
            0 => Ok(Verbosity::Terse),
            1 => Ok(Verbosity::Verbose),
            2 => Ok(Verbosity::Trace),
            _ => Err(Args::command().error(ClapErrorKind::TooManyValues, "too verbose")),
        }
    }
}
