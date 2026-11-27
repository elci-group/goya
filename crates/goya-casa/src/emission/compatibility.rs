//! compatibility - placeholder implementation

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Not implemented")]
    NotImplemented,
}

pub fn placeholder() -> Result<(), Error> {
    Err(Error::NotImplemented)
}
