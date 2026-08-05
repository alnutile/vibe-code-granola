//! Small self-contained helpers that would otherwise need a crate.
//!
//! Note images travel over two boundaries that both speak JSON — the Tauri IPC
//! bridge and the MCP transport — so bytes have to become text somewhere. Rather
//! than pull in a dependency for it, standard (padded) base64 lives here, encode
//! and decode, small enough to read and covered by tests.

use crate::error::{AppError, Result};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with `=` padding.
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decode standard base64. Whitespace and `=` padding are ignored, and a
/// `data:...;base64,` prefix is stripped, so a value pasted straight from a
/// browser `FileReader` result decodes without the caller pre-trimming it.
pub fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let input = match input.split_once(";base64,") {
        Some((_, tail)) => tail,
        None => input,
    };

    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for &c in input.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = sextet(c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

fn sextet(c: u8) -> Result<u8> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(AppError::Other("invalid base64 input".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_across_padding_lengths() {
        for input in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
        ] {
            let encoded = base64_encode(input);
            assert_eq!(base64_decode(&encoded).unwrap(), input, "for {input:?}");
        }
    }

    #[test]
    fn matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decode_ignores_whitespace_and_a_data_url_prefix() {
        assert_eq!(base64_decode("Zm9v\nYmFy").unwrap(), b"foobar");
        assert_eq!(
            base64_decode("data:image/png;base64,Zm9vYmFy").unwrap(),
            b"foobar"
        );
    }

    #[test]
    fn decode_rejects_stray_characters() {
        assert!(base64_decode("not valid *").is_err());
    }

    #[test]
    fn round_trips_arbitrary_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
    }
}
