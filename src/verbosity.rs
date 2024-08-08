use clap::{error::ErrorKind as ClapErrorKind, CommandFactory};
use log::Level;
use strum::EnumIs;

use crate::cli::Args;

#[derive(Copy, Clone, Debug, Default, EnumIs)]
pub enum Verbosity {
    Quiet,

    #[default]
    Terse,

    Verbose,
    Trace,
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

impl From<Verbosity> for Level {
    fn from(verbosity: Verbosity) -> Self {
        match verbosity {
            Verbosity::Quiet => Self::Error,
            Verbosity::Terse => Self::Warn,
            Verbosity::Verbose => Self::Info,
            Verbosity::Trace => Self::Trace,
        }
    }
}
