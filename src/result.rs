use crate::error::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
#[must_use]
pub enum RecoverableResult<T> {
    Ok(T),
    Recovered(T, Vec<Error>),
    Err(Error),
}
