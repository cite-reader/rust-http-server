//! A driver for FastCGI connections

use cgi;
use cgi::parser::doc_headers;
use config::Config;
use errors::{Result, Error};
use fastcgi::{Record, Content, EndRequest, protocol_status};
use fastcgi::parser::record;
use fastcgi::serializer::*;
use log_util::*;
use server::{Handler, Request, Response, Fresh};

use nom::IResult;

use std::ascii::AsciiExt;
use std::ffi::OsStr;
use std::io::{self, Write, Read, BufWriter, BufReader, BufRead};
use std::net::{ToSocketAddrs, TcpStream};
use std::os::unix::ffi::OsStrExt;
use std::str;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A connection to a FastCGI application server
pub struct Connection {
    conn: Mutex<TcpStream>,
    request_id: AtomicUsize,
    config: Config
}

impl Connection {
    pub fn establish<A: ToSocketAddrs>(addr: A, config: &Config)
                                       -> Result<Connection>
    {
        // I'd originally planned to configure the FCGI server to adapt to the
        // responder's capabilities, but Go's FCGI lib just says "Yes you can
        // multiplex requests" without giving me any idea what the limits are,
        // so I'm punting on dynamic config.
        Ok(Connection {
            conn: Mutex::new(try!(TcpStream::connect(addr))),
            request_id: AtomicUsize::new(0),
            config: config.clone()
        })
    }

    /// Like `Handler::serve` but with access to `try!`
    fn serve_inner(&self, mut req: Request, mut res: Response<Fresh>)
                   -> Result<()> {
        let request_number = self.request_id.load(Ordering::Acquire) + 1;
        self.request_id.store(request_number & 0xFF,
                              Ordering::Release);

        let mut conn = match self.conn.lock() {
            Ok(guard) => guard,
            Err(_poison) => return Err(Error::Poison)
        };

        try!(self.initialize_request(&mut *conn, request_number as u16, &req));

        // Send any request body there might be
        let mut client_buffer = [0; 4096];
        loop {
            let read = match req.read(&mut client_buffer) {
                Ok(size) => size,
                Err(e) => {
                    match e.kind() {
                        io::ErrorKind::WouldBlock => break,
                        _ => return Err(Error::from(e))
                    }
                }
            };
            if read == 0 {
                break;
            }

            try!(stdin(&mut *conn, request_number as u16, &client_buffer[.. read]));
        }
        // Write the stream's sentinel marker
        try!(stdin(&mut *conn, request_number as u16, &[][..]));

        // Parse CGI headers from the responder, translating them into HTTP
        // headers
        let mut reader = BufReader::new(&mut *conn);
        let mut buffer = Vec::with_capacity(4096);
        let mut unconsumed_buffer_index = 0;
        let mut last_buffer_length = 0;
        let mut headers_finished = false;
        while !headers_finished {
            let consumed = {
                let read_buffer = try!(reader.fill_buf());
                if last_buffer_length == read_buffer.len() {
                    // deal with unexpected eof
                    unimplemented!();
                }
                last_buffer_length = read_buffer.len();
                
                match record(read_buffer) {
                    IResult::Done(_, Record{id, ..})
                        if id as usize != request_number => {
                            warn!("Found a message for request {}; this is request {}", id, request_number);
                            return Err(Error::FastCgiProtocolViolation);
                        },
                    IResult::Done(rest,
                                  Record{
                                      content: Content::Stdout(content),
                                      ..})
                        =>{
                            buffer.write_all(&content[..]).unwrap();

                            match doc_headers(&buffer[..]) {
                                IResult::Done(body, hdrs) => {
                                    res.headers_mut().insert(
                                        "Content-Type",
                                        hdrs.content_type.content
                                    );
                                    if let Some(cgi::Status{code, reason_phrase})
                                        = hdrs.status {
                                            res.set_status(
                                                code,
                                                try!(String::from_utf8(
                                                    reason_phrase))
                                            );
                                        }

                                    for cgi::Header{name, content}
                                    in hdrs.headers {
                                        res.headers_mut().insert(
                                            try!(str::from_utf8(&name[..])),
                                            content
                                        );
                                    }

                                    unconsumed_buffer_index =
                                        buffer.len() - body.len();
                                    headers_finished = true;
                            },
                            IResult::Incomplete(_) => (),
                            IResult::Error(_) => unimplemented!()
                        }

                        read_buffer.len() - rest.len()
                    },
                    IResult::Done(rest, Record{
                        content: Content::Stderr(content),
                        ..
                    }) => {
                        warn!("Error message from responder: \"{}\"",
                              ascii_escape(&content[..]));
                        read_buffer.len() - rest.len()
                    },
                    IResult::Done(_, record) => {
                        warn!("Got an unexpected record type {}",
                              record.kind());
                        return Err(Error::FastCgiProtocolViolation);
                    },
                    IResult::Incomplete(_) => 0,
                    IResult::Error(_) => unimplemented!()
                }
            };
            reader.consume(consumed);
        }
        let mut res = try!(res.start());

        // Send responder output to the client, error to a log, until we get
        // an END_REQUEST message
        try!(res.write_all(&buffer[unconsumed_buffer_index ..]));
        
        last_buffer_length = 0;
        loop {
            let consume = {
                let buffer = try!(reader.fill_buf());
                if last_buffer_length == buffer.len() {
                    warn!("Out of responder input before end of headers");
                    return Err(Error::FastCgiProtocolViolation);
                }
                last_buffer_length = buffer.len();

                match record(buffer) {
                    IResult::Done(rest, Record { id, content }) => {
                        last_buffer_length = rest.len();

                        if id as usize != request_number {
                            warn!("Found a message for request {}, this is request {}",
                                  id, request_number);
                            return Err(Error::FastCgiProtocolViolation);
                        }

                        match content {
                            Content::Stdout(data) => try!(res.write_all(&data[..])),
                            Content::Stderr(msg) =>
                                warn!("Error from responder: \"{}\"",
                                      ascii_escape(&msg[..])),
                            Content::EndRequest(EndRequest {
                                app_status, protocol_status
                            }) => {
                                if protocol_status != protocol_status::REQUEST_COMPLETE {
                                    warn!("Got protocol status {}, expected 0",
                                          protocol_status);
                                }

                                if app_status != 0 {
                                    warn!("Responder closed unsuccesfully with code {}",
                                          app_status);
                                }

                                break;
                            },
                            other => {
                                warn!("Saw unexpected record kind {}",
                                      other.kind());
                                return Err(Error::FastCgiProtocolViolation);
                            }
                        };

                        buffer.len() - rest.len()
                    },
                    IResult::Error(_e) => {
                        unimplemented!()
                    },
                    IResult::Incomplete(_) => 0
                }
            };

            reader.consume(consume);
        }
        

        Ok(())
    }

    /// Initializes the request to the responder
    ///
    /// This function writes the BeginRequest record and any Params records it
    /// needs to.
    fn initialize_request<W: Write>(&self, responder: W, request_number: u16,
                                    req: &Request) -> Result<()>
    {
        let mut buf_responder = BufWriter::new(responder);
        let remote_addr = format!("{}", req.remote_addr.ip());
        let local_port_str = format!("{}", req.local_port);
        let headers: Vec<_> = req.headers().into_iter()
            .map(|(name, value)|
                 (format!("HTTP_{}",
                          name.replace("-", "_").to_ascii_uppercase()),
                  value))
            .collect();
        let translated_path = self.config.stat.webroot
            .join(OsStr::from_bytes(&req.request_uri().as_bytes()[1..]));

        let mut metavars = Vec::new();
        metavars.push((&b"GATEWAY_INTERFACE"[..], &b"CGI/1.1"[..]));
        metavars.push((&b"PATH_INFO"[..], req.request_uri().as_bytes()));
        metavars.push((&b"PATH_TRANSLATED"[..],
                       translated_path.as_os_str().as_bytes()));

        let query_string = req.request_uri().as_bytes().iter()
            .position(|&b| b == b'?')
            .map_or(&b""[..], |i| req.request_uri().as_bytes().split_at(i).1);
        metavars.push((&b"QUERY_STRING"[..], query_string));

        metavars.push((&b"REMOTE_ADDR"[..], remote_addr.as_bytes()));
        metavars.push((&b"REMOTE_HOST"[..], remote_addr.as_bytes()));
        metavars.push((&b"REQUEST_METHOD"[..], req.method().as_bytes()));
        metavars.push((&b"SCRIPT_NAME"[..], &b""[..]));
        metavars.push((&b"SERVER_NAME"[..],
                       req.headers().get("Host").map_or(&b""[..], Vec::as_slice)));
        metavars.push((&b"SERVER_PORT"[..], local_port_str.as_bytes()));
        metavars.push((&b"SERVER_PROTOCOL"[..], &b"HTTP/1.1"[..]));
        metavars.push((&b"SERVER_SOFTWARE"[..],
                       &b"toy http-server 0.2 (hella unstable)"[..]));

        for &(ref name, value) in &headers {
            metavars.push((name.as_bytes(), value));
        }

        try!(start_request(&mut buf_responder, request_number));
        try!(params(&mut buf_responder, request_number, &metavars[..]));

        Ok(())
    }

}

impl Handler for Connection {
    fn serve(&self, req: Request, res: Response<Fresh>) {
        if let Err(e) = self.serve_inner(req, res) {
            warn!("Error serving FastCGI: {:?}", e);
        }
    }
}
