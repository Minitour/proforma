//! File encoding. Composer (Java 8 on Windows, no `-Dfile.encoding`) reads and
//! writes `.pf` files in the platform code page, windows-1252. We read UTF-8 when
//! the bytes are valid UTF-8 and windows-1252 otherwise, and write windows-1252
//! whenever every character is representable (falling back to UTF-8).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Windows1252,
}

/// windows-1252 code points for bytes 0x80..=0x9F (undefined bytes map to themselves).
const HIGH: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
];

pub fn decode(bytes: &[u8]) -> (String, Encoding) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), Encoding::Utf8),
        Err(_) => (
            bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9F => char::from_u32(HIGH[(b - 0x80) as usize]).unwrap_or('\u{FFFD}'),
                    _ => b as char,
                })
                .collect(),
            Encoding::Windows1252,
        ),
    }
}

/// Encode as windows-1252 if possible; otherwise UTF-8.
pub fn encode(text: &str) -> (Vec<u8>, Encoding) {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let cp = ch as u32;
        let b = if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
            Some(cp as u8)
        } else {
            HIGH.iter().position(|&h| h == cp).map(|i| 0x80 + i as u8)
        };
        match b {
            Some(b) => out.push(b),
            None => return (text.as_bytes().to_vec(), Encoding::Utf8),
        }
    }
    (out, Encoding::Windows1252)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let (b, e) = encode("café – ok");
        assert_eq!(e, Encoding::Windows1252);
        assert_eq!(b, b"caf\xE9 \x96 ok");
        assert_eq!(decode(&b).0, "café – ok");
        assert_eq!(encode("日本").1, Encoding::Utf8);
        assert_eq!(decode("é".as_bytes()), ("é".to_string(), Encoding::Utf8));
    }
}
