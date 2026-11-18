const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\0', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\0', '\u{017D}', '\0', '\0',
    '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}',
    '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\0', '\u{017E}', '\u{0178}',
];

#[must_use]
pub fn decode_cp1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            0x00..=0x7F => out.push(byte as char),
            0x80..=0x9F => {
                let index = usize::from(byte).saturating_sub(0x80);
                match CP1252_HIGH.get(index) {
                    Some('\0') | None => out.push(char::REPLACEMENT_CHARACTER),
                    Some(&ch) => out.push(ch),
                }
            }
            _ => out.push(char::from(byte)),
        }
    }
    out
}

pub(crate) fn translate_escapes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut escaped = false;
    for &byte in bytes {
        if escaped {
            escaped = false;
            out.push(match byte {
                b'n' => b'\n',
                b't' => b'\t',
                other => other,
            });
        } else if byte == b'\\' {
            escaped = true;
        } else {
            out.push(byte);
        }
    }
    out
}

pub(crate) fn strip_spaces(bytes: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut skip_all = true;
    for &byte in bytes {
        if byte == b' ' && (skip_all || out.last() == Some(&b' ')) {
            continue;
        }
        if byte == b'\n' || byte == b'\t' {
            if out.last() == Some(&b' ') {
                out.pop();
            }
            skip_all = true;
            out.push(byte);
            continue;
        }
        out.push(byte);
        skip_all = false;
    }
    if out.last() == Some(&b' ') {
        out.pop();
    }
    out
}

pub(crate) fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while start < end {
        match bytes.get(start) {
            Some(byte) if byte.is_ascii_whitespace() => start = start.saturating_add(1),
            _ => break,
        }
    }
    while end > start {
        match bytes.get(end.saturating_sub(1)) {
            Some(byte) if byte.is_ascii_whitespace() => end = end.saturating_sub(1),
            _ => break,
        }
    }
    bytes.get(start..end).unwrap_or(&[])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn cp1252_high_range_is_not_latin1() {
        assert_eq!(decode_cp1252(&[b'a', 0x92, b's']), "a\u{2019}s");
        assert_eq!(decode_cp1252(&[0x85]), "\u{2026}");
        assert_eq!(decode_cp1252(&[0x96]), "\u{2013}");
    }

    #[test]
    fn cp1252_agrees_with_latin1_above_9f() {
        assert_eq!(decode_cp1252(&[0xA9]), "\u{00A9}");
        assert_eq!(decode_cp1252(&[0xFB]), "\u{00FB}");
    }

    #[test]
    fn undefined_cp1252_slots_are_replacement_characters_not_panics() {
        for byte in [0x81_u8, 0x8D, 0x8F, 0x90, 0x9D] {
            assert_eq!(decode_cp1252(&[byte]), "\u{FFFD}");
        }
    }

    #[test]
    fn escapes_follow_the_engines_table() {
        assert_eq!(translate_escapes(br"a\nb"), b"a\nb");
        assert_eq!(translate_escapes(br"a\tb"), b"a\tb");
        assert_eq!(translate_escapes(br#"a\"b"#), b"a\"b");
        assert_eq!(translate_escapes(br"a\\b"), br"a\b");
        assert_eq!(translate_escapes(br"a\'b"), b"a'b");
        assert_eq!(translate_escapes(br"a\?b"), b"a?b");
    }

    #[test]
    fn an_unknown_escape_yields_the_escaped_character() {
        assert_eq!(translate_escapes(br"http:\/\/x"), b"http://x");
        assert_eq!(translate_escapes(br"\Allows"), b"Allows");
    }

    #[test]
    fn strip_spaces_collapses_and_trims() {
        assert_eq!(strip_spaces(b"  a   b  "), b"a b");
        assert_eq!(strip_spaces(b"a \n b"), b"a\nb");
        assert_eq!(strip_spaces(b"a \t b"), b"a\tb");
    }
}
