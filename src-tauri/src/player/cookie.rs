//! Cookie-value codecs for the page → Rust state channel.

/// Percent-decode an `encodeURIComponent`-style cookie value (`%7B` → `{`).
/// Invalid escapes are passed through untouched rather than dropped.
pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decode standard base64 (the form the page's `btoa` produces). `None` on any
/// invalid character, so a corrupt cookie is dropped rather than half-decoded.
pub(crate) fn base64_decode(input: &str) -> Option<String> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{base64_decode, percent_decode};

    #[test]
    fn decodes_base64_queue_cookie() {
        // btoa('{"base":0,"items":[]}') === 'eyJiYXNlIjowLCJpdGVtcyI6W119'
        let encoded = "eyJiYXNlIjowLCJpdGVtcyI6W119";
        assert_eq!(base64_decode(encoded).unwrap(), r#"{"base":0,"items":[]}"#);
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("not base64!!").is_none());
    }

    #[test]
    fn decodes_encoded_json() {
        let json = r#"{"isPlaying":true,"position":12.5}"#;
        let encoded: String = json
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() {
                    (b as char).to_string()
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect();
        assert_eq!(percent_decode(&encoded), json);
    }

    #[test]
    fn leaves_invalid_escapes_alone() {
        assert_eq!(percent_decode("100%25 %zz %"), "100% %zz %");
        assert_eq!(percent_decode("plain"), "plain");
    }
}
