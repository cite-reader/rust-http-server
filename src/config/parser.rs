use super::*;
use toml::{Parser, ParserError, Table, Value};

use std::fs::File;
use std::io::{self, Read};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::u16;

pub fn parse_file<P: AsRef<Path>>(conf: P)
                                  -> Result<Config, Error>
{
    let mut toml = String::new();
    {
        let mut f = try!(File::open(conf));
        try!(f.read_to_string(&mut toml));
    }

    let mut parser = Parser::new(&toml);

    match parser.parse() {
        Some(table) => config_from_table(table),
        None => {
            let errors = parser.errors.iter().
                map(|e| message_from_error(&parser, e)).
                collect();
            Err(Error::Parse(errors))
        }
    }
}

fn config_from_table(table: Table) -> Result<Config, Error> {
    let table = Value::Table(table);
    let mut config: Config = Default::default();

    match table.lookup("listen.port") {
        Some(&Value::Integer(p))
            if p <= u16::MAX as i64 &&
            p > 0 => config.port = p as u16,
        Some(&Value::Integer(p)) => return Err(Error::Validation(
            format!("The given port {} is out of range", p)
        )),
        Some(val) => return Err(Error::Validation(
            format!("Expected the port to be an integer, got a {}",
                    val.type_str())
        )),
        None => ()
    }

    match table.lookup("static.webroot") {
        Some(&Value::String(ref path)) =>
            config.stat.webroot = PathBuf::from(path),
        Some(val) => return Err(Error::Validation(
            format!("Expected the webroot to be a string, got a {}",
                    val.type_str())
        )),
        None => ()
    }

    match table.lookup("static.public_prefix") {
        Some(&Value::String(ref path)) =>
            config.stat.public_prefix = PathBuf::from(path),
        Some(val) => return Err(Error::Validation(
            format!("Expected the webroot to be a string, got a {}",
                    val.type_str())
        )),
        None => ()
    }

    let fcgi_host = match table.lookup("fastcgi.host") {
        Some(&Value::String(ref host)) => &host[..],
        Some(val) => return Err(Error::Validation(
            format!("Expected the FastCGI host to be a string, got a {}",
                    val.type_str())
        )),
        None => "localhost"
    };

    let fcgi_port = match table.lookup("fastcgi.port") {
        Some(&Value::Integer(p)) if
            p <= u16::MAX as i64 &&
            p > 0 => p as u16,
        Some(&Value::Integer(p)) => return Err(Error::Validation(
            format!("The FastCGI port {} is out of range", p)
        )),
        Some(val) => return Err(Error::Validation(
            format!("Expected the FastCGI port to be an integer, got a {}",
                    val.type_str())
        )),
        None => 9000
    };

    config.fcgi.address =
        ToSocketAddrs::to_socket_addrs(&(fcgi_host, fcgi_port)).unwrap()
        .next().unwrap();

    Ok(config)
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(Vec<ErrorMessage>),
    Validation(String)
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

/// An owned, rendered version of a `ParserError`
#[derive(Debug, Clone)]
pub struct ErrorMessage {
    pub desc: String,
    pub line: usize,
    pub column: usize
}

/// Convert a `ParserError` into an `ErrorMessage`
fn message_from_error(parser: &Parser, error: &ParserError)
                      -> ErrorMessage
{
    let (line, column) = parser.to_linecol(error.lo);

    ErrorMessage {
        desc: error.desc.clone(),
        line: line,
        column: column
    }
}
