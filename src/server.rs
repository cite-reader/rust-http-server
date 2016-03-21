//! Server functionality

use errors::{Result, Error};

use httparse::{self, Request};
use mime::Mime;
use mime_guess::guess_mime_type_opt;

#[cfg(test)]
use std::char;

use std::ffi::OsString;
use std::fs::{File, canonicalize};
use std::io::{self, Read, Write, ErrorKind};
use std::net::{TcpListener, TcpStream};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Binds the given port and begins serving the given directory.
///
/// This function has _no_ security. Wanna serve `/`? How about
/// `~/.ssh`? Sure! Put those bytes on the Web.
///
/// Fixing this is a project for post-`0.1`.
pub fn serve<P: AsRef<Path>>(port: u16, serve_path: P) -> Result<()> {
    let listener = try!(TcpListener::bind(("0.0.0.0", port)));
    let serve_path = try!(canonicalize(serve_path));

    for stream in listener.incoming() {
        let _ = match stream {
            Ok(stream) => {
                try!(stream.set_read_timeout(Some(Duration::new(1, 0))));
                try!(stream.set_write_timeout(Some(Duration::new(1, 0))));
                handle_client(stream, &serve_path)
            },
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
    let (request_path, request_method) =
        match parse_path(&mut stream, &mut buffer[..]) {
            Ok(p) => p,
            Err(e@Error::RequestLineTooLong) => {
                try!(stream.write(ERROR_414));
                return Err(e);
            },
            Err(e@Error::PathNotInOriginForm) => {
                // TODO: be more precise
                try!(stream.write(ERROR_400));
                return Err(e);
            },
            Err(e@Error::IllegalPercentEncoding) |
            Err(e@Error::RequestIncomplete) => {
                try!(stream.write(ERROR_400));
                return Err(e);
            },
            Err(Error::Parse(e)) => {
                try!(stream.write(ERROR_400));
                return Err(Error::Parse(e));
            },
            Err(Error::Io(e)) => {
                try!(stream.write(ERROR_500));
                return Err(Error::Io(e));
            },
            Err(Error::PermissionDenied) | Err(Error::MethodNotAllowed) =>
                unreachable!()
        };

    if request_method != b"GET" {
        try!(stream.write(ERROR_405));
        return Err(Error::MethodNotAllowed);
    }

    let normalized_path = PathBuf::from(OsString::from_vec(
        match normalize_path(&request_path) {
            Ok(p) => p,
            Err(e@Error::IllegalPercentEncoding) |
            Err(e@Error::PathNotInOriginForm) => {
                try!(stream.write(ERROR_400));
                return Err(e);
            },
            Err(_) => unreachable!()
        }));

    let requested_file =
        match canonicalize(serve_path.as_ref().join(normalized_path)) {
            Ok(f) => f,
            Err(e) => {
                match e.kind() {
                    ErrorKind::NotFound => try!(stream.write(ERROR_404)),
                    _ => try!(stream.write(ERROR_500))
                };

                return Err(Error::from(e));
            }
        };

    if !requested_file.starts_with(serve_path) {
        try!(stream.write(ERROR_403));
        return Err(Error::PermissionDenied);
    }

    let mut file = match File::open(&requested_file) {
        Ok(f) => f,
        Err(e) => {
            match e.kind() {
                ErrorKind::NotFound => try!(stream.write(ERROR_404)),
                _ => try!(stream.write(ERROR_500))
            };

            return Err(Error::from(e));
        }
    };

    let meta = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            try!(stream.write(ERROR_500));
            return Err(Error::from(e));
        }
    };

    let mime = guess_mime_type_opt(&requested_file)
        .map(mime_as_string)
        .unwrap_or(String::from("application/octet-stream"));

    try!(write!(stream,
                "HTTP/1.1 200 OK\r\nContent-type: {}\r\nContent-length: {}\r\n\r\n",
                mime,
                meta.len()));

    try!(io::copy(&mut file, &mut stream));

    Ok(())
}

/// Parses a path out of a reader
fn parse_path<R: Read>(mut stream: R, buffer: &mut [u8])
                       -> Result<(Vec<u8>, Vec<u8>)> {
    let mut read = 0;
    let mut headers = [httparse::EMPTY_HEADER; 16];
    loop {
        if read == buffer.len() {
            return Err(Error::RequestLineTooLong);
        }

        let mut request = Request::new(&mut headers);

        let read_this_cycle = try!(stream.read(&mut buffer[read ..]));
        if read_this_cycle == 0 {
            return Err(Error::RequestIncomplete);
        }

        read += read_this_cycle;
        let parse_result = try!(request.parse(&buffer[.. read]));

        if parse_result.is_complete() {
            return Ok((Vec::from(request.path.unwrap().as_bytes()),
                       Vec::from(request.method.unwrap())));
        }
    }
}

#[test]
fn parsing_overlong_line_errors() {
    let overlong = b"GET /foo/bar/baz/buz/quuux/nonsense/typing HTTP/1.1";
    let mut short_buffer = [0; 10];

    match parse_path(&overlong[..], &mut short_buffer) {
        Err(Error::RequestLineTooLong) => (),
        _ => panic!()
    }
}

#[test]
fn parse_basic() {
    let request = b"GET / HTTP/1.1\r\nHost: google.com\r\nUser-Agent: curl/7.47.1\r\nAccept: */*\r\n\r\n";
    let mut buffer = [0; 8000];

    assert_eq!(parse_path(&request[..], &mut buffer).unwrap().0, b"/");
}

#[test]
fn parser_does_not_percent_decode() {
    let request = b"GET /%20 HTTP/1.1\r\n\r\n";
    let mut buffer = [0; 100];

    assert_eq!(parse_path(&request[..], &mut buffer).unwrap().0, b"/%20");
}

#[test]
fn parser_does_not_fail_on_illegal_percent_encoding() {
    let request = b"GET /bogus%zz HTTP/1.1\r\n\r\n";
    let mut buffer = [0; 100];

    assert!(parse_path(&request[..], &mut buffer).is_ok());
}

#[test]
fn parser_fails_on_bad_bytes() {
    let request = b"GET /bogon\xff HTTP/1.1\r\n";
    let mut buffer = [0; 100];

    assert!(parse_path(&request[..], &mut buffer).is_err());
}

#[test]
fn parser_gives_method() {
    let request = b"GET / HTTP/1.1\r\n\r\n";
    let mut buffer = [0; 100];

    assert_eq!(parse_path(&request[..], &mut buffer).unwrap().1, b"GET");
}

/// Normalizes a path.
///
/// The following operations are performed:
///
/// 1. Sequences of multiple `'/'` characters are collapsed into a single `'/'`.
/// 2. Any leading `'/'` is stripped. (If the request path doesn’t lead with a
///    slash, the path is ill-formed for our purposes and we return an `Err`).
/// 3. Percent-encoded bytes are decoded. Bogus percent-encoding, like `b"%bo"`,
///    will return `Err`.
fn normalize_path(path: &[u8]) -> Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(path.len() - 1);

    // Check for a leading `'/'`
    if path[0] != 0x2F {
        return Err(Error::PathNotInOriginForm);
    }

    let mut i = 1;

    // skip leading '/' characters
    while i < path.len() && path[i] == 0x2F {
        i += 1;
    }

    while i < path.len() {
        match path[i] {
            // '/'
            0x2F => {
                buffer.push(0x2F);
                while i < path.len() && path[i] == 0x2F {
                    i += 1;
                }
            },
            // '%'
            0x25 => {
                if !(path.len() >= i + 2) {
                    return Err(Error::IllegalPercentEncoding);
                }

                let high_nybble = path[i + 1];
                let low_nybble = path[i + 2];

                if !is_hexit(high_nybble) || !is_hexit(low_nybble) {
                    return Err(Error::IllegalPercentEncoding);
                }

                buffer.push(from_hexit(high_nybble) << 4 |
                            from_hexit(low_nybble));

                i += 3;
            },
            b => {
                buffer.push(b);
                i += 1;
            }
        }
    }

    Ok(buffer)
}

#[test]
fn normalize_strips_leading_slashes() {
    assert_eq!(normalize_path(b"/blah").unwrap(), b"blah");
    assert_eq!(normalize_path(b"//bleh").unwrap(), b"bleh");
}

#[test]
fn normalize_collapses_embedded_slash_sequences() {
    assert_eq!(normalize_path(b"/foo//bar").unwrap(), b"foo/bar");
}

#[test]
fn normalize_decodes_percents() {
    assert_eq!(normalize_path(b"/foo%20bar").unwrap(), b"foo bar");
}

#[test]
fn normalize_handles_trailing_percents_correctly() {
    assert_eq!(normalize_path(b"/trail%20").unwrap(), b"trail ");
}

#[test]
fn normalize_errors_on_bogus_percent() {
    assert!(normalize_path(b"/bog%us").is_err());
}

#[test]
fn normalize_errors_without_leading_slash() {
    assert!(normalize_path(b"bogus").is_err());
}

/// Returns `true` iff the byte is a hexadecimal digit according to ASCII
fn is_hexit(x: u8) -> bool {
    (0x30 <= x && x <= 0x39) ||
    (0x41 <= x && x <= 0x46) ||
    (0x61 <= x && x <= 0x66)
}

/// Converts from a hexadecimal digit to its value
fn from_hexit(x: u8) -> u8 {
    if 0x30 <= x && x <= 0x39 {
        x - 0x30
    }
    else if 0x41 <= x && x <= 0x46 {
        x - 0x41 + 10
    }
    else if 0x61 <= x && x <= 0x66 {
        x - 0x61 + 10
    }
    else {
        panic!("Contract violation: from_hexit expected a hexit, got 0x{:X}", x);
    }
}

#[test]
fn from_hexit_works() {
    for x in 0x0 .. 0x10 {
        assert_eq!(from_hexit(char::from_digit(x, 16).unwrap() as u8), x as u8);
    }
}

/// Translates a strongly-typed Mime type into a string
fn mime_as_string(mime: Mime) -> String {
    let mut s = String::new();

    let Mime(toplevel, sublevel, _) = mime;

    s.push_str(toplevel.as_str());
    s.push_str("/");
    s.push_str(sublevel.as_str());
    
    s
}

#[test]
fn mime_as_string_html() {
    assert_eq!(mime_as_string(mime!(Text/Html)), "text/html");
}

#[test]
fn mime_as_string_css() {
    assert_eq!(mime_as_string(mime!(Text/Css)), "text/css");
}

#[test]
fn mime_as_string_javascript() {
    assert_eq!(mime_as_string(mime!(Text/Javascript)), "text/javascript");
}

const ERROR_500: &'static [u8] = b"HTTP/1.1 500 INTERNAL ERROR\r\nContent-Type: text/html\r\nContent-length: 192\r\n\r\n<!doctype html><html><head><title>Error</title></head><body><h1>Internal Error</h1><p>Something went wrong on my side.</p><p>There's nothing you can do; maybe come back later.</p></body></html>";

const ERROR_414: &'static [u8] = b"HTTP/1.1 414 URI TOO LONG CHILL OUT\r\nContent-type: text/html\r\nContent-Length: 169\r\n\r\n<!doctype html><html><head><title>Error</title></head><body><h1>URI Too Long</h1><p>Your user-agent produced a URI too long for this server; tell it to chill out.</p></body></html>";

const ERROR_405: &'static [u8] = b"HTTP/1.1 405 METHOD NOT ALLOWED\r\nContent-type: text/html\r\nContent-length: 181\r\n\r\n<!doctype html><html><head><title>Error</title></head><body><h1>Method Not Allowed</h1><p>This server only understands <code>GET</code> requests. Sorry about that.</p></body></html>";

const ERROR_404: &'static [u8] = b"HTTP/1.1 404 NOT FOUND\r\nContent-type: text/html\r\ncontent-Length: 132\r\n\r\n<!doctype html><html><head><title>Error</title></head><body><h1>Not Found</h1><p>I couldn't find that file. Sorry.</p></body></html>";

const ERROR_403: &'static [u8] = b"HTTP/1.1 403 FORBIDDEN\r\nContent-type: text/html\r\nContent-Length: 150\r\n\r\n<!doctype html><html><head><title>Error</titlpe></head><body><h1>Forbidden</h1><p>You don't have permission to view that file. Sorry.</p></body></html>";

const ERROR_400: &'static [u8] = b"HTTP/1.1 400 Bad Request\r\nContent-type: test/html\r\nContent-Length: 164\r\n\r\n<!doctype html><html><head><title>Error</title></head><body><h1>Bad Request</h1><p>Your request had some kind of bad syntax. Are you using netcat?</p></body></html>";
