//! Process CGI/1.1 response messages
//!
//! FastCGI 1 inherits its message format and semantics from CGI/1.1.

pub mod parser;

/// A status line
#[derive(Debug, PartialEq, Eq)]
pub struct Status {
    pub code: u16,
    pub reason_phrase: Vec<u8>
}

/// A location redirect
#[derive(Debug, PartialEq, Eq)]
pub struct Location {
    url: Vec<u8>
}

/// Other headers
#[derive(Debug, PartialEq, Eq)]
pub struct Header {
    pub name: Vec<u8>,
    pub content: Vec<u8>
}

/// The header portion of a document
#[derive(Debug, PartialEq, Eq)]
pub struct DocumentHeaders {
    pub content_type: Header,
    pub status: Option<Status>,
    pub headers: Vec<Header>
}
