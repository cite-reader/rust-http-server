//! Smol utilities for logging

#![allow(dead_code)]

use std::ascii;

/// Make an Ascii-safe string
pub fn ascii_escape(s: &[u8]) -> String {
    String::from_utf8(
        s.into_iter().flat_map(|&b| ascii::escape_default(b)).collect()
    ).unwrap()
}
 
