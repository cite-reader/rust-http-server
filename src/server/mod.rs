//! Server functionality

mod static_files;
mod router;

use config::Config;
use errors::{Result, Error};
use fastcgi::driver as fcgi_driver;
use filesystem::normalize_path;
use server::router::Router;
use server::static_files::Statics;

use httparse;
use mime::Mime;
use log::LogLevel;

use std::ascii::AsciiExt;
use std::collections::HashMap;
use std::collections::hash_map::{self, Entry};
use std::ffi::OsStr;
use std::fs::canonicalize;
use std::io::{self, Read, BufRead, BufReader, Write, BufWriter, ErrorKind};
use std::marker::PhantomData;
use std::mem;
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

/// Binds the given port and begins serving the given directory.
///
/// This function has _no_ security. Wanna serve `/`? How about
/// `~/.ssh`? Sure! Put those bytes on the Web.
///
/// Fixing this is a project for post-`0.1`.
pub fn serve(mut config: Config) -> Result<()> {
    let listener = try!(TcpListener::bind(("0.0.0.0", config.port)));
    config.stat.webroot = try!(canonicalize(config.stat.webroot));

    let mut router = Router::new();

    let fcgi_conn = match fcgi_driver::Connection::establish("127.0.0.1:9000",
                                                             &config) {
        Ok(c) => c,
        Err(Error::Io(e)) => {
            match e.kind() {
                ErrorKind::ConnectionRefused =>
                    log!(LogLevel::Error, "FastCGI responder not responding"),
                _ => log!(LogLevel::Error, "{:?}", e)
            }

            return Err(Error::Io(e));
        },
        Err(e) => return Err(e)
    };

    router.route(config.stat.public_prefix.clone(), String::from("GET"),
                 Statics::new(config.clone()));
    router.route_any(PathBuf::from("/"), fcgi_conn);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                try!(stream.set_read_timeout(Some(Duration::new(5, 0))));
                try!(stream.set_write_timeout(Some(Duration::new(5, 0))));

                match make_request_pair(try!(stream.try_clone())) {
                    Ok((req, res)) => router.serve(req, res),
                    Err(Error::Parse(_)) =>
                        try!(error_messages::error_400(Response::new(stream))),
                    Err(e) => warn!("{:?}", e)
                }
            },
            Err(e) => {
                warn!("Failed connection: {}", e);
            }
        };
    }

    Ok(())
}

fn make_request_pair(stream: TcpStream) -> Result<(Request, Response<Fresh>)>
{
    let peer_addr = try!(stream.peer_addr());
    let local_port = try!(stream.local_addr()).port();
    let response_inner = try!(stream.try_clone());
    let request_inner = stream;

    let response = Response::new(response_inner);

    let request = Request {
        inner: try!(InnerRequest::parse(request_inner)),
        remote_addr: peer_addr,
        local_port: local_port
    };

    Ok((request, response))
}

/// Values which can handle requests
pub trait Handler {
    fn serve(&self, req: Request, res: Response<Fresh>);
}

impl<F> Handler for F where F: Fn(Request, Response<Fresh>) {
    fn serve(&self, req: Request, res: Response<Fresh>) {
        self(req, res)
    }
}

/// An incoming request from the client
#[derive(Debug)]
pub struct Request {
    inner: InnerRequest<TcpStream>,
    pub remote_addr: SocketAddr,
    pub local_port: u16
}

/// Internal, generic version of a Request
///
/// This division is primarily useful for testing; tests can wrap a simple byte
/// buffer, and the public impls can be trivial wrappers specialized to a
/// network stream.
#[derive(Debug)]
struct InnerRequest<R> {
    method: String,
    path: Vec<u8>,
    headers: Headers,

    rest: BufReader<R>
}

impl<R: Read> InnerRequest<R> {
    fn parse(stream: R) -> Result<InnerRequest<R>> {
        let mut reader = BufReader::new(stream);
        
        let (consumed,
             method,
             path,
             headers) = try!(parse_inner(&mut reader));

        reader.consume(consumed);

        Ok(InnerRequest {
            method: method,
            path: try!(normalize_path(path.as_bytes())),
            headers: headers,
            rest: reader
        })
    }
}

fn parse_inner<R: BufRead>(mut source: R) -> Result<(usize,
                                                     String,
                                                     String,
                                                     Headers)>
{
    let mut headers = [httparse::EMPTY_HEADER; 100];
    let mut last_buffer_len = 0;

    loop {
        let mut req = httparse::Request::new(&mut headers);
        let buffer = try!(source.fill_buf());

        let buffer_len = buffer.len();
        if buffer_len == last_buffer_len {
            return Err(Error::RequestIncomplete);
        }
        last_buffer_len = buffer_len;

        if let httparse::Status::Complete(bytes) = try!(req.parse(buffer)) {
            let mut headers = Headers::new();
            for header in req.headers {
                headers.insert(header.name, Vec::from(header.value));
            }

            return Ok(
                (bytes,
                 String::from(req.method.unwrap()),
                 String::from(req.path.unwrap()),
                 headers)
            );
        }
    }
}

#[test]
fn parse_request_basic() {
    let request: &[u8] = b"GET / HTTP/1.1\r\nHost: google.com\r\nUser-Agent: curl/7.47.1\r\nAccept: */*\r\n\r\n";

    let (_, method, path, _) = parse_inner(request).unwrap();

    assert_eq!(method, "GET");
    assert_eq!(path, "/");
}

#[test]
fn parse_request_does_not_percent_decode() {
    let request: &[u8] = b"GET /%20 HTTP/1.1\r\n\r\n";

    let (_, _, path, _) = parse_inner(request).unwrap();

    assert_eq!(path, "/%20");
}

#[test]
fn parse_request_does_not_fail_on_illegal_percent_decoding() {
    let request: &[u8] = b"GET /bogus%zz HTTP/1.1\r\n\r\n";

    let (_, _, path, _) = parse_inner(request).unwrap();

    assert_eq!(path, "/bogus%zz");
}

#[test]
fn parse_request_fails_on_bad_bytes() {
    let request: &[u8] = b"GET /bogon\xff HTTP/1.1\r\n";

    assert!(parse_inner(request).is_err());
}

impl Request {
    pub fn request_uri(&self) -> &OsStr {
        OsStr::from_bytes(self.inner.path.as_slice())
    }

    #[inline]
    pub fn method(&self) -> &str {
        &self.inner.method
    }

    #[inline]
    pub fn headers(&self) -> &Headers {
        &self.inner.headers
    }
}

impl Read for Request {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.rest.read(buf)
    }
}

impl BufRead for Request {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.rest.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.rest.consume(amt)
    }
}

/// The response being constructed by a `Handler`
///
/// The type parameter represents where in the cycle this response is. When
/// `Status = Fresh`, nothing has been sent to the client, headers can be
/// modified, and if the entire response-body is available at once it’s
/// possible to write the response in one shot.
///
/// When `Status = Streaming`, headers have already been sent, and use of the
/// `io::Write` interface will buffer chunks (as in Transfer-Encoding: Chunked)
/// to be sent to the client as they become available.
pub struct Response<Status> {
    writer: BufWriter<TcpStream>,
    buffer: Vec<u8>,
    status: ResponseStatus,
    headers: Headers,
    _status: PhantomData<Status>
}

/// A marker for `Response`, indicating nothing has been sent to the client
pub enum Fresh {}

/// A marker for `Response`, indicating headers have been sent and writes will
/// be sent in chunks
pub enum Streaming {}

struct ResponseStatus {
    code: u16,
    reason: String
}

/// A map of HTTP headers
///
/// This is just a newtype wrapper around a `HashMap<String, String>`, but the
/// keys are case-normalized on input. The first word, and any words after a
/// hyphen, are capitalized, with all other letters lowercased.
#[derive(Debug, Clone)]
pub struct Headers {
    map: HashMap<String, Vec<u8>>
}

fn normalize_header_name(name: &str) -> String {
    let lowercased = name.to_ascii_lowercase();
    let mut lower_chars = lowercased.chars();

    let mut normalized = String::with_capacity(lowercased.len());
    if let Some(ch) = lower_chars.next() {
        normalized.push(ch.to_ascii_uppercase());
    }
    else {
        return normalized;
    }

    let mut after_hyphen = false;
    for ch in lower_chars {
        if ch == '-' {
            after_hyphen = true;
            normalized.push(ch);
        }
        else if after_hyphen {
            normalized.push(ch.to_ascii_uppercase());
            after_hyphen = false;
        }
        else {
            normalized.push(ch);
        }
    }

    normalized
}

#[test]
fn normalize_content_type() {
    let expected = "Content-Type";
    assert_eq!(expected, &normalize_header_name("Content-Type"));
    assert_eq!(expected, &normalize_header_name("content-type"));
    assert_eq!(expected, &normalize_header_name("CONTENT-TYPE"));
    assert_eq!(expected, &normalize_header_name("cOnTeNt-TyPe"));
}

impl Headers {
    pub fn new() -> Headers {
        Headers {
            map: HashMap::new()
        }
    }

    pub fn insert(&mut self, key: &str, mut value: Vec<u8>) {
        match self.map.entry(normalize_header_name(key)) {
            Entry::Vacant(e) => { e.insert(value); },
            Entry::Occupied(mut e) => {
                let entry = e.get_mut();
                entry.reserve(value.len() + 1);
                entry.push(b',');
                entry.append(&mut value);
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Vec<u8>> {
        self.map.get(&normalize_header_name(key))
    }
}

impl IntoIterator for Headers {
    type Item = (String, Vec<u8>);
    type IntoIter = hash_map::IntoIter<String, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}

impl<'a> IntoIterator for &'a Headers {
    type Item = (&'a String, &'a Vec<u8>);
    type IntoIter = hash_map::Iter<'a, String, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<'a> IntoIterator for &'a mut Headers {
    type Item = (&'a String, &'a mut Vec<u8>);
    type IntoIter = hash_map::IterMut<'a, String, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter_mut()
    }
}    

/*
impl<Status> Response<Status> {

}
*/
impl Response<Fresh> {
    pub fn new(stream: TcpStream) -> Self {
        Response {
            writer: BufWriter::new(stream),
            buffer: Vec::new(),
            status: ResponseStatus {
                code: 200,
                reason: String::from("Ok")
            },
            headers: Headers::new(),
            _status: PhantomData
        }
    }

    pub fn of_stream<R: Read>(mut self, mut stream: R) -> io::Result<()> {
        try!(self.write_headers());
        io::copy(&mut stream, &mut self.writer).map(|_| ())
    }

    #[inline]
    pub fn headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    pub fn set_status(&mut self, code: u16, reason: String) {
        self.status = ResponseStatus {
            code: code,
            reason: reason
        };
    }

    pub fn start(mut self) -> io::Result<Response<Streaming>> {
        self.headers.insert("Transfer-Encoding",
                            Vec::from(&b"Chunked"[..]));
                            
        try!(self.write_headers());
        self.buffer = Vec::with_capacity(4096);

        // Transmute to ourselves with a different phantom type
        Ok(unsafe { mem::transmute(self) })
    }

    fn write_headers(&mut self) -> io::Result<()> {
        // Status line
        try!(write!(self.writer, "HTTP/1.1 {} {}\r\n",
                    self.status.code, self.status.reason));

        for (header, content) in &self.headers {
            try!(write!(self.writer, "{}: ", header));
            try!(self.writer.write_all(content));
            try!(self.writer.write_all(b"\r\n"));
        }

        try!(self.writer.write_all(b"\r\n"));

        Ok(())
    }
}

impl Response<Streaming> {
    /// Writes a single chunk in the chunked transfer-encoding, clearing out
    /// all buffers.
    fn write_chunk(&mut self) -> io::Result<()> {
        if self.buffer.len() == 0 {
            return Ok(());
        }
        
        try!(write_chunk_raw(&mut self.writer, self.buffer.as_slice()));
        self.buffer.clear();
        Ok(())
    }
}

fn write_chunk_raw<W: Write>(sink: &mut W, chunk_content: &[u8])
                             -> io::Result<()>
{
    try!(write!(sink, "{:x}\r\n", chunk_content.len()));
    try!(sink.write_all(chunk_content));
    try!(sink.write_all(b"\r\n"));
    sink.flush()
}

impl Write for Response<Streaming> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        let buffer_cap_remaining = self.buffer.capacity() - self.buffer.len();

        if buf.len() > buffer_cap_remaining {
            if buf.len() > self.buffer.capacity() {
                try!(self.write_chunk());
                try!(write_chunk_raw(&mut self.writer, buf));
            }
            else {
                self.buffer.extend_from_slice(&buf[.. buffer_cap_remaining]);
                try!(self.write_chunk());
                self.buffer.extend_from_slice(&buf[buffer_cap_remaining ..]);
            }
        }
        else {
            self.buffer.extend_from_slice(buf);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.len() > 0 {
            self.write_chunk()
        }
        else {
            // Nothing to do
            Ok(())
        }
    }
}

impl<T> Drop for Response<T> {
    fn drop(&mut self) {
        // A non-trivial buffer implies the Response is streaming
        if self.buffer.capacity() > 0 {
            let _ = write_chunk_raw(&mut self.writer, self.buffer.as_slice());
            let _ = self.writer.write_all(b"0\r\n"); // last chunk
        }
    }
}

/// Translates a strongly-typed Mime type into a string
pub fn mime_as_string(mime: Mime) -> String {
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

pub mod error_messages {
    use super::Response;
    use super::Fresh;

    use std::io;

    pub fn error_500(mut res: Response<Fresh>) -> io::Result<()> {
        res.set_status(500, String::from("Internal Error"));
        {
            let headers = res.headers_mut();
            headers.insert("Content-Type", Vec::from(&b"text/html"[..]));
            headers.insert("Content-Length", Vec::from(&b"192"[..]));
        }

        res.of_stream(ERROR_500)
    }

    const ERROR_500: &'static [u8] = b"<!doctype html><html><head><title>Error</title></head><body><h1>Internal Error</h1><p>Something went wrong on my side.</p><p>There's nothing you can do; maybe come back later.</p></body></html>";

    pub fn error_405(mut res: Response<Fresh>) -> io::Result<()> {
        res.set_status(405, String::from("Method not allowed"));
        {
            let headers = res.headers_mut();
            headers.insert("Content-Type", Vec::from(&b"text/html"[..]));
            headers.insert("Content-Length", Vec::from(&b"181"[..]));
        }

        res.of_stream(ERROR_405)
    }

    const ERROR_405: &'static [u8] = b"<!doctype html><html><head><title>Error</title></head><body><h1>Method Not Allowed</h1><p>This server only understands <code>GET</code> requests. Sorry about that.</p></body></html>";

    pub fn error_404(mut res: Response<Fresh>) -> io::Result<()> {
        res.set_status(404, String::from("Not Found"));
        {
            let headers = res.headers_mut();
            headers.insert("Content-Type", Vec::from(&b"text/html"[..]));
            headers.insert("Content-Length", Vec::from(&b"132"[..]));
        }

        res.of_stream(ERROR_404)
    }

    const ERROR_404: &'static [u8] = b"<!doctype html><html><head><title>Error</title></head><body><h1>Not Found</h1><p>I couldn't find that file. Sorry.</p></body></html>";

    pub fn error_403(mut res: Response<Fresh>) -> io::Result<()> {
        res.set_status(403, String::from("Forbidden"));
        {
            let headers = res.headers_mut();
            headers.insert("Content-type", Vec::from(&b"text/html"[..]));
            headers.insert("Content-Length", Vec::from(&b"150"[..]));
        }

        res.of_stream(ERROR_403)
    }

    const ERROR_403: &'static [u8] = b"<!doctype html><html><head><title>Error</title></head><body><h1>Forbidden</h1><p>You don't have permission to view that file. Sorry.</p></body></html>";

    pub fn error_400(mut res: Response<Fresh>) -> io::Result<()> {
        res.set_status(400, String::from("Bad Request"));
        {
            let headers = res.headers_mut();
            headers.insert("Content-type", Vec::from(&b"text/html"[..]));
            headers.insert("Content-length", Vec::from(&b"164"[..]));
        }

        res.of_stream(ERROR_400)
    }

    const ERROR_400: &'static [u8] = b"<!doctype html><html><head><title>Error</title></head><body><h1>Bad Request</h1><p>Your request had some kind of bad syntax. Are you using netcat?</p></body></html>";
}
