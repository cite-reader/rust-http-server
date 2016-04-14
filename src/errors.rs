//! Error handling for the http server

use httparse;

use std::io;
use std::num::ParseIntError;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

/// A Result for internal operations.
pub type Result<T> = ::std::result::Result<T, Error>;

/// All errors which might arise within the application
#[derive(Debug)]
pub enum Error {
    Parse(httparse::Error),
    Io(io::Error),
    Serialization(SerializationError),
    ParseInt(ParseIntError),
    FromUtf8(Utf8Error),
    FromUtf8Alt(FromUtf8Error),
    Poison,
    ApplicationServerDisappeared,
    FastCgiProtocolViolation,
    PathNotInOriginForm,
    IllegalPercentEncoding,
    PermissionDenied,
    RequestIncomplete
}

/// Things that can go wrong when serializing FastCGI messages
#[derive(Debug)]
pub enum SerializationError {
    /// The length of a name or value could not be represented in four bytes
    TooLong
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

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Error {
        Error::ParseInt(e)
    }
}

impl From<Utf8Error> for Error {
    fn from(e: Utf8Error) -> Error {
        Error::FromUtf8(e)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(e: FromUtf8Error) -> Error {
        Error::FromUtf8Alt(e)
    }
}
