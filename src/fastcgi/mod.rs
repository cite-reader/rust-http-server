mod parser;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Record {
    pub id: u16,
    pub content: Content
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
