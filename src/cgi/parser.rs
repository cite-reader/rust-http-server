//! Parsers for CGI/1.1 responses

use cgi::{Status, Location, Header, DocumentHeaders};

use nom::*;

use std::str::{self, FromStr};

named!(pub status < Status >, chain!(
             tag!("Status:")      ~
             opt!(tag!(" "))      ~
    code   : code                 ~
             tag!(" ")            ~
    phrase : text                 ,
    || { Status { code: code, reason_phrase: Vec::from(phrase) } }));

named!(code < u16 >,
       map_res!(
           map_res!(
               flat_map!(take!(3), verify_status_number),
                   str::from_utf8),
           FromStr::from_str )
);

fn verify_status_number(number: &[u8]) -> IResult<&[u8], &[u8]> {
    if !number.iter().all(|&x| is_digit(x)) {
        return IResult::Error(Err::Position(ErrorKind::Digit, number));
    }

    IResult::Done(&[][..], number)
}

named!(text, take_till!(cr_or_lf));

fn cr_or_lf(&x: &u8) -> bool {
    x == b'\n' || x == b'\r'
}

named!(pub location < Location >, chain!(
          tag!("Location:")      ~
          opt!(tag!(" "))        ~
    uri : take_till!(cr_or_lf) ,
    || { Location { url: Vec::from(uri) } }
));

named!(pub header < Header >, chain!(
    name:    take_till!(is_colon)   ~
             tag!(":")              ~
             take_while!(lwsp)      ~
    content: take_till!(cr_or_lf) ,
    || { Header { name: Vec::from(name), content: Vec::from(content) }}
));

#[test]
fn header_works() {
    let input: &[u8] = b"Foo: bar\r\n\r\n";
    let expected = Header {
        name: Vec::from(&b"Foo"[..]),
        content: Vec::from(&b"bar"[..])
    };

    match header(input) {
        IResult::Done(rest, res) => {
            assert_eq!(expected, res);
            assert_eq!(b"\r\n\r\n", rest);
        },
        other => panic!("{:?}", other)
    }
}

#[test]
fn header_empty() {

}

fn lwsp(x: u8) -> bool {
    x == b' ' || x == b'\t' || x == b'\n'
}

fn is_colon(&x: &u8) -> bool {
    x == b':'
}

pub fn headers(input: &[u8]) -> IResult<&[u8], Vec<Header>> {
    let mut hdrs = Vec::new();

    let mut next = input;
    loop {
        let (nxt1, hdr) = try_parse!(next, header);
        hdrs.push(hdr);
        match double_newline(nxt1) {
            IResult::Done(nxt2, _) => {
                next = nxt2;
                break;
            },
            IResult::Error(_) => (),
            IResult::Incomplete(needed) => {return IResult::Incomplete(needed);}
        }

        let (nxt2, _) = try_parse!(nxt1, alt!(crlf | newline));
        next = nxt2;
    }

    IResult::Done(next, hdrs)
}

fn double_newline(input: &[u8]) -> IResult<&[u8], ()> {
    if input.len() < 2 {
        return IResult::Incomplete(Needed::Size(2 - input.len()));
    }

    if &input[.. 2] == b"\n\n" {
        return IResult::Done(&input[2 ..], ());
    }

    if input.len() < 3 {
        return IResult::Incomplete(Needed::Size(3 - input.len()));
    }

    if &input[.. 3] == b"\r\n\n" {
        return IResult::Done(&input[3 ..], ());
    }

    if input.len() < 4 {
        return IResult::Incomplete(Needed::Size(4 - input.len()));
    }

    if &input[.. 4] == b"\r\n\r\n" {
        return IResult::Done(&input[4 ..], ());
    }

    IResult::Error(Err::Position(ErrorKind::CrLf, input))
}

#[test]
fn test_headers() {
    let input: &[u8] = b"Foo: bar\r\nBaz: buz\r\n\r\n";

    let expected = vec![
        Header {
            name: Vec::from(&b"Foo"[..]),
            content: Vec::from(&b"bar"[..])
        },
        Header {
            name: Vec::from(&b"Baz"[..]),
            content: Vec::from(&b"buz"[..])
        }
    ];
    match headers(input) {
        IResult::Done(rest, hdrs) => {
            assert_eq!(expected, hdrs);
            assert_eq!(b"", rest);
        },
        other => panic!("{:?}", other)
    }
}

named!(pub content_type < Header >, chain!(
    tag!("Content-Type:") ~
        opt!(tag!(" ")) ~
    media: take_till!(cr_or_lf)  ,
    || { Header { name: Vec::from(&b"Content-Type"[..]),
                  content: Vec::from(media) }}
));

named!(pub doc_headers < DocumentHeaders >, chain!(
        stat: opt!(status) ~
        alt!(crlf | newline) ~
        ctype: content_type ~
        alt!(crlf | newline) ~
        hdrs: headers ,
    || { DocumentHeaders {
        content_type: ctype,
        status: stat,
        headers: hdrs
    }}
));

#[test]
fn doc_headers_on_captured_traffic() {
    let input: &[u8] = b"Status: 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nDate: Thu, 07 Apr 2016 20:42:43 GMT\r\n\r\n<!DOCTYPE html>\n<html>\n  <head>\n    <title>Guestbook</title>\n    <link rel=\"stylesheet\" type=\"text/css\" href=\"/static/css/base.css\" />\n  </head>\n  <body>\n    <section id=\"content\"><h1>Guestbook</h1>\n<p>Hello, and welcome to my guestbook, because I needed a Web project and immediately <a href=\"https://eev.ee/blog/2012/07/28/quick-doesnt-mean-dirty/\">cribbed from Eevee.</a></p>\n<ul class=\"guests\">\n  <li>\n    <blockquote>New\r\nLines\r\nAre\r\nGreat!</blockquote>\n    <p>\xe2\x80\x94 <cite>newliner</cite>, <time datetime=\"2016-03-20T15:05&#43;0000\">Sat Mar 20 3:05 PM 2016</time></p>\n  </li><li>\n    <blockquote>&lt;script&gt;alert(&#39;pwned from message&#39;)&lt;/script&gt;</blockquote>\n    <p>\xe2\x80\x94 <cite>&lt;script&gt;alert(&#39;pwned from name&#39;)&lt;/script&gt;</cite>, <time datetime=\"2016-03-20T14:33&#43;0000\">Sat Mar 20 2:33 PM 2016</time></p>\n  </li><li>\n    <blockquote>\xf0\x9f\x94\xa5 This is a test \xf0\x9f\x94\xa5</blockquote>\n    <p>\xe2\x80\x94 <cite>Tester MacTesterson</cite>, <time datetime=\"2016-03-20T14:31&#43;0000\">Sat Mar 20 2:31 PM 2016</time></p>\n  </li><li>\n    <blockquote>Hooray I can display a thing</blockquote>\n    <p>\xe2\x80\x94 <cite>An Wobsite Developer</cite>, <time datetime=\"2016-03-19T22:22&#43;0000\">Sat Mar 19 10:22 PM 2016</time></p>\n  </li>\n</ul>\n<hr />\n<form action=\"\" method=\"POST\">\n  <p><label>Name: <input type=\"text\" name=\"name\" /></label></p>\n  <p><label>Message: <textarea name=\"message\" rows=\"10\" cols=\"40\"></textarea></label></p>\n  <p><button>Sign</button></p>\n</form></section>\n    <footer>An Guestbook \xc2\xa9 2016 Alex</footer>\n  </body>\n</html>\n";

        let expected = DocumentHeaders {
            content_type: Header {
                name: Vec::from(&b"Content-Type"[..]),
                content: Vec::from(&b"text/html; charset=utf-8"[..])
            },
            status: Some(Status {
                code: 200,
                reason_phrase: Vec::from(&b"OK"[..])
            }),
            headers: vec![
                Header {
                    name: Vec::from(&b"Date"[..]),
                    content: Vec::from(&b"Thu, 07 Apr 2016 20:42:43 GMT"[..])
                }
            ]
        };

    match doc_headers(input) {
        IResult::Done(_, actual) => assert_eq!(expected, actual),
        other => panic!("{:?}", other)
    }
}
