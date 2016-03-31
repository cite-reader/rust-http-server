//! Helpers for filesystem manipulations

use errors::{Result, Error};

/// Normalizes a path.
///
/// The following operations are performed:
///
/// 1. Sequences of multiple `'/'` characters are collapsed into a single `'/'`.
/// 2. Any leading `'/'` is stripped. (If the request path doesn’t lead with a
///    slash, the path is ill-formed for our purposes and we return an `Err`).
/// 3. Percent-encoded bytes are decoded. Bogus percent-encoding, like `b"%bo"`,
///    will return `Err`.
pub fn normalize_path(path: &[u8]) -> Result<Vec<u8>> {
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

#[cfg(test)]
mod test {
    use super::*;

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
    use std::char;

    for x in 0x0 .. 0x10 {
        assert_eq!(from_hexit(char::from_digit(x, 16).unwrap() as u8), x as u8);
    }
}
