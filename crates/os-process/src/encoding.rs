//! Decode arbitrary subtitle bytes to UTF-8 text.
//!
//! Strategy: honor a BOM → try strict UTF-8 → otherwise detect with `chardetng`
//! and decode via `encoding_rs`. Line endings are normalized to `\n`.

use encoding_rs::Encoding;

/// Decode bytes to UTF-8 `String`, detecting the source encoding.
pub fn to_utf8(bytes: &[u8]) -> String {
    // BOM sniffing first.
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        let (text, _, _) = enc.decode(&bytes[bom_len..]);
        return normalize_newlines(&text);
    }

    // Strict UTF-8 fast path.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return normalize_newlines(s);
    }

    // Detect with chardetng.
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let enc = detector.guess(None, true);
    let (text, _, _) = enc.decode(bytes);
    normalize_newlines(&text)
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_utf8() {
        assert_eq!(to_utf8(b"Hello\r\nWorld\r"), "Hello\nWorld\n");
    }

    #[test]
    fn utf8_bom_stripped() {
        let mut v = vec![0xEF, 0xBB, 0xBF];
        v.extend_from_slice(b"caf\xC3\xA9");
        assert_eq!(to_utf8(&v), "café");
    }

    #[test]
    fn latin1_fallback() {
        // 0xE9 is 'é' in ISO-8859-1/Windows-1252; invalid UTF-8 alone.
        let out = to_utf8(b"caf\xE9");
        assert!(out.contains('é') || out.contains('\u{e9}'));
    }
}
