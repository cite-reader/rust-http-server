use super::*;

use nom::*;

pub fn record(input: &[u8]) -> IResult<&[u8], Record> {
    let (in1, _) = try_parse!(input, be_u8); // protocol version
    let (in2, kind) = try_parse!(in1, be_u8);
    let (in3, id) = try_parse!(in2, be_u16);
    let (in4, content_length) = try_parse!(in3, be_u16);
    let (in5, padding_length) = try_parse!(in4, be_u8);
    let (in6, _) = try_parse!(in5, take!(1)); // reserved byte
    let (in7, content) = try_parse!(in6, take!(content_length));
    let (in8, _) = try_parse!(in7, take!(padding_length));

    let (_, parsed_content) = match kind {
        record_kind::BEGIN_REQUEST => try_parse!(content, begin_request),
        record_kind::ABORT_REQUEST => try_parse!(content, abort_request),
        record_kind::END_REQUEST => try_parse!(content, end_request),
        record_kind::PARAMS => try_parse!(content, params),
        record_kind::STDIN => (content, Content::Stdin(Vec::from(content))),
        record_kind::STDOUT => (content, Content::Stdout(Vec::from(content))),
        record_kind::STDERR => (content, Content::Stderr(Vec::from(content))),
        record_kind::DATA => (content, Content::Data(Vec::from(content))),
        record_kind::GET_VALUES => try_parse!(content, get_values),
        record_kind::UNKNOWN_TYPE => try_parse!(content, unknown_type),
        _ => return IResult::Error(Err::Position(
            ErrorKind::Custom(ParseError::UnknownType(kind).to_u32()),
            in8
        ))
    };

    IResult::Done(in8, Record { id: id, content: parsed_content })
}

named!(begin_request<Content>,
       chain!(
           role  : role     ~
           flags : be_u8    ~
                   take!(5) ,
           || {
               Content::BeginRequest(BeginRequest {
                   role: role,
                   flags: flags
               })
           }
       )
);

fn abort_request(input: &[u8]) -> IResult<&[u8], Content> {
    IResult::Done(input, Content::AbortRequest(AbortRequest))
}

named!(end_request<Content>,
       chain!(
           app_status      : be_u32   ~
           protocol_status : be_u8    ~
                             take!(3) ,
           || Content::EndRequest(EndRequest {
               app_status: app_status,
               protocol_status: protocol_status
           })
       )
);

named!(get_values<Content>,
       map!(name_value_pairs, Content::GetValues));

named!(get_values_result<Content>,
       map!(name_value_pairs, Content::GetValuesResult));

named!(params<Content>,
       map!(name_value_pairs, Content::Params));

named!(name_value_pairs< Vec<NameValuePair> >, many0!(name_value_pair));

fn name_value_pair(input: &[u8]) -> IResult<&[u8], NameValuePair> {
    let (in1, initial_name_length) = try_parse!(input, be_u8);
    let (in2, name_length) = if initial_name_length >> 7 == 1 {
        try_parse!(input, be_u32)
    }
    else {
        (in1, initial_name_length as u32)
    };
    
    let (in3, initial_value_length) = try_parse!(in2, be_u8);
    let (in4, value_length) = if initial_value_length >> 7 == 1 {
        try_parse!(in3, be_u32)
    }
    else {
        (in3, initial_value_length as u32)
    };

    let (in5, name) = try_parse!(in4, take!(name_length));
    let (in6, value) = try_parse!(in5, take!(value_length));

    IResult::Done(in6, NameValuePair {
        name: Vec::from(name),
        value: Vec::from(value)
    })
}

fn role(input: &[u8]) -> IResult<&[u8], Role> {
    let (in1, tag) = try_parse!(input, be_u16);
    let r = match tag {
        1 => Role::Responder,
        2 => Role::Authorizer,
        3 => Role::Filter,
        _ => return IResult::Error(Err::Position(
            ErrorKind::Custom(ParseError::UnknownRole(tag).to_u32()),
            input))
    };

    IResult::Done(in1, r)
}

named!(unknown_type<Content>,
       chain!(
           kind : be_u8    ~
                  take!(7) ,
           || { Content::UnknownType(UnknownType(kind)) }
       )
);

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnknownRole(u16),
    UnknownType(u8)
}

impl ParseError {
    pub fn to_u32(self) -> u32 {
        match self {
            ParseError::UnknownRole(role) => role as u32 | 1 << 17,
            ParseError::UnknownType(kind) => kind as u32
        }
    }

    pub fn from_u32(repr: u32) -> ParseError {
        if repr & 1 << 17 != 0 {
            ParseError::UnknownRole(repr as u16)
        }
        else {
            ParseError::UnknownType(repr as u8)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fastcgi::*;

    use nom::IResult;

    #[test]
    fn begin_request() {
        let input = [01, 01, 00, 01, 00, 08, 00, 00, 00, 01, 00, 00, 00, 00,
                     00, 00];

        match record(&input[..]) {
            IResult::Done(_, result) => assert_eq!(
                result,
                Record {
                    id: 1,
                    content: Content::BeginRequest(BeginRequest {
                        role: Role::Responder,
                        flags: 0
                    })
                }
            ),
            _ => panic!()
        }
    }

    #[test]
    fn params() {
        let input = [1, 4, 0, 1, 2, 152, 0, 0, 15, 16, 83, 67, 82, 73, 80, 84, 95, 70, 73, 76, 69, 78, 65, 77, 69, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 104, 116, 109, 108, 47, 12, 0, 81, 85, 69, 82, 89, 95, 83, 84, 82, 73, 78, 71, 14, 3, 82, 69, 81, 85, 69, 83, 84, 95, 77, 69, 84, 72, 79, 68, 71, 69, 84, 12, 0, 67, 79, 78, 84, 69, 78, 84, 95, 84, 89, 80, 69, 14, 0, 67, 79, 78, 84, 69, 78, 84, 95, 76, 69, 78, 71, 84, 72, 11, 1, 83, 67, 82, 73, 80, 84, 95, 78, 65, 77, 69, 47, 11, 1, 82, 69, 81, 85, 69, 83, 84, 95, 85, 82, 73, 47, 12, 1, 68, 79, 67, 85, 77, 69, 78, 84, 95, 85, 82, 73, 47, 13, 15, 68, 79, 67, 85, 77, 69, 78, 84, 95, 82, 79, 79, 84, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 104, 116, 109, 108, 15, 8, 83, 69, 82, 86, 69, 82, 95, 80, 82, 79, 84, 79, 67, 79, 76, 72, 84, 84, 80, 47, 49, 46, 49, 17, 7, 71, 65, 84, 69, 87, 65, 89, 95, 73, 78, 84, 69, 82, 70, 65, 67, 69, 67, 71, 73, 47, 49, 46, 49, 15, 11, 83, 69, 82, 86, 69, 82, 95, 83, 79, 70, 84, 87, 65, 82, 69, 110, 103, 105, 110, 120, 47, 49, 46, 56, 46, 49, 11, 9, 82, 69, 77, 79, 84, 69, 95, 65, 68, 68, 82, 49, 50, 55, 46, 48, 46, 48, 46, 49, 11, 5, 82, 69, 77, 79, 84, 69, 95, 80, 79, 82, 84, 52, 55, 57, 48, 52, 11, 9, 83, 69, 82, 86, 69, 82, 95, 65, 68, 68, 82, 49, 50, 55, 46, 48, 46, 48, 46, 49, 11, 4, 83, 69, 82, 86, 69, 82, 95, 80, 79, 82, 84, 56, 48, 48, 48, 11, 9, 83, 69, 82, 86, 69, 82, 95, 78, 65, 77, 69, 108, 111, 99, 97, 108, 104, 111, 115, 116, 15, 3, 82, 69, 68, 73, 82, 69, 67, 84, 95, 83, 84, 65, 84, 85, 83, 50, 48, 48, 9, 14, 72, 84, 84, 80, 95, 72, 79, 83, 84, 108, 111, 99, 97, 108, 104, 111, 115, 116, 58, 56, 48, 48, 48, 15, 68, 72, 84, 84, 80, 95, 85, 83, 69, 82, 95, 65, 71, 69, 78, 84, 77, 111, 122, 105, 108, 108, 97, 47, 53, 46, 48, 32, 40, 88, 49, 49, 59, 32, 76, 105, 110, 117, 120, 32, 120, 56, 54, 95, 54, 52, 59, 32, 114, 118, 58, 52, 53, 46, 48, 41, 32, 71, 101, 99, 107, 111, 47, 50, 48, 49, 48, 48, 49, 48, 49, 32, 70, 105, 114, 101, 102, 111, 120, 47, 52, 53, 46, 48, 11, 63, 72, 84, 84, 80, 95, 65, 67, 67, 69, 80, 84, 116, 101, 120, 116, 47, 104, 116, 109, 108, 44, 97, 112, 112, 108, 105, 99, 97, 116, 105, 111, 110, 47, 120, 104, 116, 109, 108, 43, 120, 109, 108, 44, 97, 112, 112, 108, 105, 99, 97, 116, 105, 111, 110, 47, 120, 109, 108, 59, 113, 61, 48, 46, 57, 44, 42, 47, 42, 59, 113, 61, 48, 46, 56, 20, 14, 72, 84, 84, 80, 95, 65, 67, 67, 69, 80, 84, 95, 76, 65, 78, 71, 85, 65, 71, 69, 101, 110, 45, 85, 83, 44, 101, 110, 59, 113, 61, 48, 46, 53, 20, 13, 72, 84, 84, 80, 95, 65, 67, 67, 69, 80, 84, 95, 69, 78, 67, 79, 68, 73, 78, 71, 103, 122, 105, 112, 44, 32, 100, 101, 102, 108, 97, 116, 101, 8, 1, 72, 84, 84, 80, 95, 68, 78, 84, 49, 15, 10, 72, 84, 84, 80, 95, 67, 79, 78, 78, 69, 67, 84, 73, 79, 78, 107, 101, 101, 112, 45, 97, 108, 105, 118, 101];

        let expected_params = vec![
            NameValuePair {
                name: Vec::from(&b"SCRIPT_FILENAME"[..]),
                value: Vec::from(&b"/etc/nginx/html/"[..])
            },
            NameValuePair {
                name: Vec::from(&b"QUERY_STRING"[..]),
                value: vec![]
            },
            NameValuePair {
                name: Vec::from(&b"REQUEST_METHOD"[..]),
                value: Vec::from(&b"GET"[..])
            },
            NameValuePair {
                name: Vec::from(&b"CONTENT_TYPE"[..]),
                value: vec![]
            },
            NameValuePair {
                name: Vec::from(&b"CONTENT_LENGTH"[..]),
                value: vec![]
            },
            NameValuePair {
                name: Vec::from(&b"SCRIPT_NAME"[..]),
                value: Vec::from(&b"/"[..])
            },
            NameValuePair {
                name: Vec::from(&b"REQUEST_URI"[..]),
                value: Vec::from(&b"/"[..])
            },
            NameValuePair {
                name: Vec::from(&b"DOCUMENT_URI"[..]),
                value: Vec::from(&b"/"[..])
            },
            NameValuePair {
                name: Vec::from(&b"DOCUMENT_ROOT"[..]),
                value: Vec::from(&b"/etc/nginx/html"[..])
            },
            NameValuePair {
                name: Vec::from(&b"SERVER_PROTOCOL"[..]),
                value: Vec::from(&b"HTTP/1.1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"GATEWAY_INTERFACE"[..]),
                value: Vec::from(&b"CGI/1.1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"SERVER_SOFTWARE"[..]),
                value: Vec::from(&b"nginx/1.8.1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"REMOTE_ADDR"[..]),
                value: Vec::from(&b"127.0.0.1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"REMOTE_PORT"[..]),
                value: Vec::from(&b"47904"[..])
            },
            NameValuePair {
                name: Vec::from(&b"SERVER_ADDR"[..]),
                value: Vec::from(&b"127.0.0.1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"SERVER_PORT"[..]),
                value: Vec::from(&b"8000"[..])
            },
            NameValuePair {
                name: Vec::from(&b"SERVER_NAME"[..]),
                value: Vec::from(&b"localhost"[..])
            },
            NameValuePair {
                name: Vec::from(&b"REDIRECT_STATUS"[..]),
                value: Vec::from(&b"200"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_HOST"[..]),
                value: Vec::from(&b"localhost:8000"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_USER_AGENT"[..]),
                value: Vec::from(&b"Mozilla/5.0 (X11; Linux x86_64; rv:45.0) Gecko/20100101 Firefox/45.0"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_ACCEPT"[..]),
                value: Vec::from(&b"text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_ACCEPT_LANGUAGE"[..]),
                value: Vec::from(&b"en-US,en;q=0.5"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_ACCEPT_ENCODING"[..]),
                value: Vec::from(&b"gzip, deflate"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_DNT"[..]),
                value: Vec::from(&b"1"[..])
            },
            NameValuePair {
                name: Vec::from(&b"HTTP_CONNECTION"[..]),
                value: Vec::from(&b"keep-alive"[..])
            }
        ];
        let expected = Record {
            id: 1,
            content: Content::Params(expected_params.clone())
        };

        match record(&input[..]) {
            IResult::Done(_, result) => {
                assert_eq!(result.id, expected.id);
                match result.content {
                    Content::Params(result_params) => {
                        assert_eq!(result_params.len(), expected_params.len());
                        for (r, e) in result_params.into_iter()
                            .zip(expected_params.into_iter()) {
                                assert_eq!(r, e);
                            }
                    },
                    _ => panic!()
                }
            },
            _ => panic!()
        }
    }

    #[test]
    fn params_empty() {
        let input = [1, 4, 0, 1, 0, 0, 0, 0];

        match record(&input[..]) {
            IResult::Done(_, result) => assert_eq!(
                result,
                Record {
                    id: 1,
                    content: Content::Params(vec![])
                }),
            _ => panic!()
        }
    }

    #[test]
    fn stdin() {
        let input = [1, 5, 0, 1, 0, 0, 0, 0];

        match record(&input[..]) {
            IResult::Done(_, result) => assert_eq!(
                result,
                Record {
                    id: 1,
                    content: Content::Stdin(vec![])
                }),
            _ => panic!()
        }
    }

    #[test]
    fn end_request() {
        let input = [1, 3, 0, 1, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        match record(&input[..]) {
            IResult::Done(_, result) => assert_eq!(
                result,
                Record {
                    id: 1,
                    content: Content::EndRequest(EndRequest {
                        app_status: 0,
                        protocol_status: protocol_status::REQUEST_COMPLETE
                    })
                }),
            _ => panic!()
        }
    }
}
