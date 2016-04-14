//! A basic static-files Web server.
//!
//! Call it like this:
//!
//!     http-server -f config.toml
//!
//! The config file is in the [TOML format][toml] because it’s commonly used in
//! the Rust ecosystem. Here is an example:
//!
//! ```toml
//! [listen]
//! port = 8000
//!
//! [static]
//! webroot = "/etc/http-server/site"
//! public_prefix = "/html"
//!
//! [fastcgi]
//! host = "localhost"
//! port = 9000
//! ```
//!
//! This example also serves as the defaults if no config file is provided,
//! or any given key is not present. If a key is of the wrong type, the server
//! will bail, so don’t do that.
//!
//! `http-server` will listen for connections from any IP address, and
//! understands only GET requests. It speaks only the bare minimum of HTTP to
//! perform that task, and doesn’t care about things like Accept headers.
//!
//! [toml]: https://github.com/toml-lang/toml

extern crate byteorder;
extern crate clap;
extern crate env_logger;
extern crate httparse;
#[macro_use] extern crate log;
#[macro_use] extern crate mime;
extern crate mime_guess;
#[macro_use] extern crate nom;
extern crate toml;

mod cgi;
mod config;
mod errors;
mod fastcgi;
mod filesystem;
mod log_util;
mod server;

use config::parser::{self, parse_file};
use server::serve;

use clap::{Arg, App};

use std::env;
use std::ffi::OsStr;
use std::io::{stderr, Write};
use std::os::unix::ffi::OsStrExt;
use std::process::exit;

fn main() {
    let mut log_builder = env_logger::LogBuilder::new();
    log_builder.filter(None, log::LogLevelFilter::Info);

    if let Ok(var) = env::var("SERVER_LOG") {
        log_builder.parse(&var);
    }

    match log_builder.init() {
        Ok(()) => (),
        Err(e) => {
            writeln!(stderr(),
                     "http-server: Error when initializing logging: {}",
                     e).unwrap();
            exit(1);
        }
    };

    let matches = App::new("http-server")
        .version("0.2")
        .author("Alex Hill <alexander.d.hill.89@gmail.com>")
        .arg(Arg::with_name("config_file")
             .short("f")
             .value_name("FILE")
             .help("The TOML file with server configuration")
             .takes_value(true))
        .get_matches();

    let config_file = matches.value_of_os("config_file")
        .unwrap_or(OsStr::from_bytes(b"/etc/http-server/config.toml"));

    let config = match parse_file(config_file) {
        Ok(c) => c,
        Err(parser::Error::Io(e)) => {
            log!(log::LogLevel::Error, "Error opening config file {:?}: {}",
                 config_file, e);
            exit(1);
        },
        Err(parser::Error::Parse(e)) => {
            log!(log::LogLevel::Error,
                 "Errors parsing config file {:?}", config_file);
            for error in e {
                log!(
                    log::LogLevel::Error,
                    "Config file error at line {} column {}: {}",
                    error.line, error.column, error.desc);
            }
            exit(1);
        },
        Err(parser::Error::Validation(message)) => {
            log!(log::LogLevel::Error,
                 "Error in config file: {}", message);
            exit(1);
        }
    };

    log!(log::LogLevel::Info, "Starting server on port {}", config.port);
    if let Err(_) = serve(config) {
        exit(1);
    }
}
