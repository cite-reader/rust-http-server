//! Serialization of FastCGI messages

use errors::{Error, SerializationError, Result};
use fastcgi::{Role, record_kind, flags};

use byteorder::{BigEndian, WriteBytesExt};

use std::io::Write;
use std::u16;
use std::u32;

/// Writes a header from its bits
///
/// If succesful, returns the number of bytes of padding we told the other end
/// of the connection we were going to write.
fn write_header<W: Write>(mut output: W, kind: u8, id: u16,
                          content_length: usize)
                          -> Result<u8>
{
    if content_length > u16::MAX as usize {
        return Err(Error::Serialization(SerializationError::TooLong));
    }

    let padding_length = if content_length % 8 == 0 {
        0
    }
    else {
        8 - content_length % 8
    };

    try!(output.write_all(&[1, kind]));
    try!(output.write_u16::<BigEndian>(id));
    try!(output.write_u16::<BigEndian>(content_length as u16));
    try!(output.write_u8(padding_length as u8));
    try!(output.write_u8(0)); // reserved byte

    Ok(padding_length as u8)
}

/// Writes a `GetValues` record to the output stream
pub fn get_values<W: Write>(mut output: W, get_for: &[&[u8]]) -> Result<()>
{
    let content_length = get_for.iter()
        .map(|&name| name_length(name) + 1)
        .fold(0, |acc, x| acc + x);

    let padding_length = try!(write_header(&mut output,
                                           record_kind::GET_VALUES,
                                           0,
                                           content_length));

    for &name in get_for {
        try!(write_name_val_pair(&mut output, name, &vec![]));
    }

    try!(output.write_all(&vec![0; padding_length as usize]));

    Ok(())
}

/// Computes the number of bytes a name or value will take up on the wire once
/// serialized into the FastCGI name-value pair format
fn name_length(val: &[u8]) -> usize {
    let length = val.len();
    let length_length = if length > 127 { 4 } else { 1 };

    length + length_length
}

/// Writes a name-value pair to the stream
fn write_name_val_pair<W: Write>(mut output: W, name: &[u8], val: &[u8])
                                 -> Result<()>
{
    let name_length = name.len();
    let val_length = val.len();

    if name_length > u32::MAX as usize || val_length > u32::MAX as usize {
        return Err(Error::Serialization(SerializationError::TooLong));
    }

    if name_length > 127 {
        try!(output.write_u32::<BigEndian>(name_length as u32));
    }
    else {
        try!(output.write_u8(name_length as u8));
    }

    if val_length > 127 {
        try!(output.write_u32::<BigEndian>(val_length as u32));
    }
    else {
        try!(output.write_u8(val_length as u8));
    }

    try!(output.write_all(name));
    try!(output.write_all(val));

    Ok(())
}

/// Write a `BeginRequest` message
///
/// This is specialized for the Responder role, with the `FCGI_KEEP_CONN` flag
/// set.
pub fn start_request<W: Write>(mut output: W, id: u16) -> Result<()> {
    let padding_length = try!(write_header(&mut output,
                                           record_kind::BEGIN_REQUEST,
                                           id,
                                           8));
    try!(output.write_u16::<BigEndian>(Role::Responder.to_protocol_number()));
    try!(output.write_u8(flags::KEEP_CONN));
    try!(output.write_all(&[0; 5])); // reserved

    try!(output.write_all(&vec![0; padding_length as usize]));

    Ok(())
}

/// Write a stream of parameters
///
/// This will automatically emit the stream-terminating empty message as well.
pub fn params<W: Write>(mut output: W, id: u16, params: &[(&[u8], &[u8])])
                        -> Result<()> {
    let content_length = params.iter()
        .map(|&(name, value)| name_length(name) + name_length(value))
        .fold(0, |acc, x| acc + x);

    let padding_length = try!(write_header(&mut output,
                                           record_kind::PARAMS,
                                           id,
                                           content_length));

    for &(name, value) in params {
        try!(write_name_val_pair(&mut output, name, value));
    }
    try!(output.write_all(&vec![0; padding_length as usize]));

    let sentinal_padding =
        try!(write_header(&mut output, record_kind::PARAMS, id, 0));
    try!(output.write_all(&vec![0; sentinal_padding as usize]));

    Ok(())
}

/// Write a frame of a FCGI_STDIN stream
pub fn stdin<W: Write>(mut output: W, id: u16, content: &[u8]) -> Result<()> {
    let padding_length = try!(write_header(&mut output, record_kind::STDIN,
                                           id, content.len()));
    try!(output.write_all(content));
    try!(output.write_all(&vec![0; padding_length as usize]));

    Ok(())
}
