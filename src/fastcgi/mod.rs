#![allow(dead_code)]

pub mod driver;
pub mod parser;
mod serializer;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Record {
    pub id: u16,
    pub content: Content
}

impl Record {
    #[inline]
    pub fn kind(&self) -> u8 {
        self.content.kind()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Content {
    GetValues(GetValues),
    GetValuesResult(GetValuesResult),
    UnknownType(UnknownType),
    BeginRequest(BeginRequest),
    Params(Params),
    Stdin(Vec<u8>),
    Data(Vec<u8>),
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    AbortRequest(AbortRequest),
    EndRequest(EndRequest)
}

impl Content {
    pub fn kind(&self) -> u8 {
        match *self {
            Content::GetValues(_) => record_kind::GET_VALUES,
            Content::GetValuesResult(_) => record_kind::GET_VALUES_RESULT,
            Content::UnknownType(_) => record_kind::UNKNOWN_TYPE,
            Content::BeginRequest(_) => record_kind::BEGIN_REQUEST,
            Content::Params(_) => record_kind::PARAMS,
            Content::Stdin(_) => record_kind::STDIN,
            Content::Data(_) => record_kind::DATA,
            Content::Stdout(_) => record_kind::STDOUT,
            Content::Stderr(_) => record_kind::STDERR,
            Content::AbortRequest(_) => record_kind::ABORT_REQUEST,
            Content::EndRequest(_) => record_kind::END_REQUEST
        }
    }
}

pub type Params = Vec<NameValuePair>;

pub type GetValues = Vec<NameValuePair>;

pub type GetValuesResult = Vec<NameValuePair>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct NameValuePair {
    name: Vec<u8>,
    value: Vec<u8>
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BeginRequest {
    pub role: Role,
    pub flags: u8
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UnknownType(pub u8);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct AbortRequest;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EndRequest {
    pub app_status: u32,
    pub protocol_status: u8
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Role {
    Responder,
    Authorizer,
    Filter
}

impl Role {
    /// Returns the protocol's number for this role
    pub fn to_protocol_number(self) -> u16 {
        match self {
            Role::Responder => 1,
            Role::Authorizer => 2,
            Role::Filter => 3
        }
    }
}

pub mod flags {
    pub const KEEP_CONN: u8 = 1;
}

pub mod record_kind {
    pub const BEGIN_REQUEST: u8 = 1;
    pub const ABORT_REQUEST: u8 = 2;
    pub const END_REQUEST: u8 = 3;
    pub const PARAMS: u8 = 4;
    pub const STDIN: u8 = 5;
    pub const STDOUT: u8 = 6;
    pub const STDERR: u8 = 7;
    pub const DATA: u8 = 8;
    pub const GET_VALUES: u8 = 9;
    pub const GET_VALUES_RESULT: u8 = 10;
    pub const UNKNOWN_TYPE: u8 = 11;
}

pub mod protocol_status {
    pub const REQUEST_COMPLETE: u8 = 0;
    pub const CANT_MPX_CONN: u8 = 1;
    pub const OVERLOADED: u8 = 2;
    pub const UNKNOWN_ROLE: u8 = 3;
}

pub mod management_records {
    pub const MAX_CONNS: &'static [u8] = b"FCGI_MAX_CONNS";
    pub const MAX_REQS: &'static [u8] = b"FCGI_MAX_REQS";
    pub const MPXS_CONNS: &'static [u8] = b"FCGI_MPXS_CONNS";
}
