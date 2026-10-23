//! `metaprops` values are Java `.properties` text. Composer re-serializes them
//! with `PropertyUtils.propsToString`: keys sorted, `saveConvert` escaping and a
//! CRLF after every line. These helpers reproduce that round trip.

/// Parse `.properties` text (java.util.Properties.load semantics; later keys win).
pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    while let Some(first) = lines.next() {
        let mut logical = first.trim_start_matches([' ', '\t', '\x0c']).to_string();
        if logical.is_empty() || logical.starts_with('#') || logical.starts_with('!') {
            continue;
        }
        // continuation lines: odd number of trailing backslashes
        while ends_with_odd_backslashes(&logical) {
            logical.pop();
            match lines.next() {
                Some(next) => logical.push_str(next.trim_start_matches([' ', '\t', '\x0c'])),
                None => break,
            }
        }
        let chars: Vec<char> = logical.chars().collect();
        let mut i = 0;
        let mut key_end = chars.len();
        let mut value_start = chars.len();
        while i < chars.len() {
            match chars[i] {
                '\\' => i += 2,
                '=' | ':' => {
                    key_end = i;
                    value_start = i + 1;
                    break;
                }
                ' ' | '\t' | '\x0c' => {
                    key_end = i;
                    let mut j = i;
                    while j < chars.len() && matches!(chars[j], ' ' | '\t' | '\x0c') {
                        j += 1;
                    }
                    if j < chars.len() && matches!(chars[j], '=' | ':') {
                        j += 1;
                    }
                    value_start = j;
                    break;
                }
                _ => i += 1,
            }
        }
        let key = unescape(&chars[..key_end.min(chars.len())]);
        let mut vs = value_start.min(chars.len());
        while vs < chars.len() && matches!(chars[vs], ' ' | '\t' | '\x0c') {
            vs += 1;
        }
        let value = unescape(&chars[vs..]);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key, value)),
        }
    }
    out
}

fn ends_with_odd_backslashes(s: &str) -> bool {
    s.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

fn unescape(chars: &[char]) -> String {
    let mut o = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match n {
                't' => o.push('\t'),
                'n' => o.push('\n'),
                'r' => o.push('\r'),
                'f' => o.push('\x0c'),
                'u' => {
                    let hex: String = chars[i..(i + 4).min(chars.len())].iter().collect();
                    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        Some(ch) if hex.len() == 4 => {
                            o.push(ch);
                            i += 4;
                        }
                        _ => o.push('u'),
                    }
                }
                other => o.push(other),
            }
        } else {
            o.push(c);
            i += 1;
        }
    }
    o
}

/// `Properties.saveConvert`.
fn save_convert(s: &str, escape_space: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        match c {
            ' ' if i == 0 || escape_space => o.push_str("\\ "),
            '\t' => o.push_str("\\t"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\x0c' => o.push_str("\\f"),
            '=' | ':' | '#' | '!' => {
                o.push('\\');
                o.push(c);
            }
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    o.push_str(&format!("\\u{:04X}", unit));
                }
            }
            c => o.push(c),
        }
    }
    o
}

/// `PropertyUtils.propsToString`: sorted `key=value` lines, each ending with CRLF.
pub fn to_string(props: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = props.iter().collect();
    sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    let mut o = String::new();
    for (k, v) in sorted {
        o.push_str(&save_convert(k, true));
        o.push('=');
        o.push_str(&save_convert(v, false));
        o.push_str("\r\n");
    }
    o
}

/// Normalize a metaprops string the way a Composer load/save cycle does.
pub fn normalize(text: &str) -> String {
    to_string(&parse(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_and_escapes() {
        assert_eq!(
            normalize("zeta=1\nalpha=two words\nk=v\n"),
            "alpha=two words\r\nk=v\r\nzeta=1\r\n"
        );
        assert_eq!(
            normalize("a\\:b=x y\\=z\\u00E9 "),
            "a\\:b=x y\\=z\\u00E9 \r\n"
        );
        assert_eq!(normalize(""), "");
    }
}
