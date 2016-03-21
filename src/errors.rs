//! Error handling for the http server

use httparse;

use std::io;

/// A Result for internal operations.
pub type Result<T> = ::std::result::Result<T, Error>;

/// All errors which might arise within the application
#[derive(Debug)]
pub enum Error {
    Parse(httparse::Error),
    Io(io::Error),
    RequestLineTooLong,
    PathNotInOriginForm,
    IllegalPercentEncoding,
    MethodNotAllowed,
    PermissionDenied,
    RequestIncomplete
}

impl From<httparse::Error> for Error {
    fn from(e: httparse::Error) -> Error {
        Error::Parse(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}
