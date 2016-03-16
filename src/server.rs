//! Server functionality

use errors::{Result, Error};

use httparse::{self, Request};

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

/// Binds the given port and begins serving the given directory.
///
/// This function has _no_ security. Wanna serve `/`? How about
/// `~/.ssh`? Sure! Put those bytes on the Web.
///
/// Fixing this is a project for post-`0.1`.
pub fn serve<P: AsRef<Path>>(port: u16, serve_path: P) -> Result<()> {
    let listener = try!(TcpListener::bind(("0.0.0.0", port)));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(stream, &serve_path),
            Err(_) => Ok(()) // Ignore failed connections fobr now.
        };
    }

    Ok(())
}

/// Serves up files in the requested path.
///
/// This is a blocking operation; post `0.1` we’ll grow a thread pool or a `mio`
/// reactor.
fn handle_client<P: AsRef<Path>>(mut stream: TcpStream, serve_path: P)
                                 -> Result<()> {
    // length is the suggested minimum from RFC 7230
    let mut buffer = [0u8; 8000];
    let request_path = match parse_path(&mut stream, &mut buffer[..]) {
        Ok(p) => p,
        Err(e) => {
            // TODO: 4xx FIRST LINE TOO LONG; CHILL OUT, CLIENT
            return Err(Error::from(e));
        }  
    };
    
    // Well-formed requests lead with a '/', which makes for an absolute path.
    // Slice that off to create a relative path.
    let requested_file = serve_path.as_ref().join(&request_path[1..]);

    // TODO: check that requested_file is a child of serve_path, serve 403
    // otherwise.

    let mut file = match File::open(requested_file) {
        Ok(f) => f,
        Err(e) => {
            // TODO: handle this. In particular, 404 Not Found
            return Err(Error::from(e));
        }
    };

    let meta = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            // TODO: 500 Internal Server Error
            return Err(Error::from(e));
        }
    };

    try!(write!(stream,
                "HTTP/1.1 200 OK\r\nContent-type: text/plain; charset=utf-8\r\nContent-length: {}\r\n\r\n",
                meta.len()));

    try!(io::copy(&mut file, &mut stream));

    Ok(())
}

/// Parses a path out of a reader
fn parse_path<R: Read>(mut stream: R, buffer: &mut [u8]) -> Result<String> {
    let mut read = 0;
    let mut headers = [httparse::EMPTY_HEADER; 16];
    loop {
        if read == buffer.len() {
            return Err(Error::RequestLineTooLong);
        }

        let mut request = Request::new(&mut headers);

        read += try!(stream.read(&mut buffer[read ..]));
        try!(request.parse(&buffer[.. read]));

        if let Some(path) = request.path {
            return Ok(String::from(path));
        }
    }
}
