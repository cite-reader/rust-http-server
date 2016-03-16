//! A basic static-files Web server.
//!
//! Call it like this:
//!
//!     http-server -p 8080 path/to/mount
//!
//! If the port isn’t provided, it defaults to `8080`. The path is mandatory.
//!
//! `http-server` will listen for connections from any IP address, and
//! understands only GET requests. It speaks only the bare minimum of HTTP to
//! perform that task, and doesn’t care about things like Accept headers.

extern crate clap;
extern crate httparse;

mod server;
mod errors;

use server::serve;

use clap::{Arg, App};

fn main() {
    let matches = App::new("http-server")
        .version("0.1")
        .author("Alex Hill <alexander.d.hill.89@gmail.com>")
        .arg(Arg::with_name("port")
             .short("p")
             .value_name("PORT")
             .help("The port to listen on")
             .takes_value(true))
        .arg(Arg::with_name("files")
             .help("The directory to serve")
             .required(true))
        .get_matches();

    let port = matches.value_of("port")
        .and_then(|p| p.parse().ok()) // Discard parse errors for 0.1
        .unwrap_or(8080u16);

    // clap guarantees that if we get here, this arg will exist.
    let files = matches.value_of_os("files").unwrap();

    serve(port, files).unwrap();
}
