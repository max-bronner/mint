use crate::text::{strip_spaces, translate_escapes, trim};
use crate::{decode_cp1252, Error, Result, StringRecord};

pub fn parse_str(bytes: &[u8]) -> Result<Vec<StringRecord>> {
    let mut records = Vec::with_capacity(bytes.len().saturating_div(64));
    let mut cursor = Cursor::new(bytes);

    while let Some((line, line_no)) = cursor.read_line() {
        let label_bytes = trim(line);
        if label_bytes.is_empty() || label_bytes.starts_with(b"//") {
            continue;
        }
        let label = decode_cp1252(label_bytes);
        let label_line = line_no;

        let mut text: Option<String> = None;
        let mut speech: Option<String> = None;
        let mut closed = false;

        while let Some((line, line_no)) = cursor.read_line() {
            let line = trim(line);
            if line.first() == Some(&b'"') && text.is_none() {
                let (raw, wave) = read_to_end_of_quote(&mut cursor, line, line_no)?;
                text = Some(decode_cp1252(&strip_spaces(&translate_escapes(&raw))));
                speech = wave;
            } else if line.eq_ignore_ascii_case(b"END") {
                closed = true;
                break;
            }
        }

        if !closed {
            return Err(Error::UnterminatedRecord {
                line: label_line,
                label,
            });
        }
        if let Some(text) = text {
            records.push(StringRecord {
                label,
                text,
                speech,
            });
        }
    }

    Ok(records)
}

fn read_to_end_of_quote(
    cursor: &mut Cursor<'_>,
    line: &[u8],
    line_no: usize,
) -> Result<(Vec<u8>, Option<String>)> {
    let mut pending: Vec<u8> = line.get(1..).unwrap_or(&[]).to_vec();
    pending.push(b'\n');
    let mut at = 0usize;

    let next = |cursor: &mut Cursor<'_>, at: &mut usize| -> Option<u8> {
        match pending.get(*at) {
            Some(&byte) => {
                *at = at.saturating_add(1);
                Some(byte)
            }
            None => cursor.read_byte(),
        }
    };

    let mut out: Vec<u8> = Vec::new();
    let mut escaped = false;
    let mut closed = false;
    while let Some(byte) = next(cursor, &mut at) {
        let mut byte = byte;
        if byte == b'\n' {
            escaped = false;
            byte = b' ';
        } else if byte == b'\\' {
            escaped = !escaped;
        } else if byte == b'"' && !escaped {
            closed = true;
            break;
        } else {
            escaped = false;
        }
        if byte.is_ascii_whitespace() {
            byte = b' ';
        }
        out.push(byte);
    }
    if !closed {
        return Err(Error::UnterminatedString { line: line_no });
    }

    let mut wave: Vec<u8> = Vec::new();
    let mut state = WaveScan::Leading;
    while let Some(byte) = next(cursor, &mut at) {
        if byte == b'\n' {
            break;
        }
        if state == WaveScan::Leading {
            if byte.is_ascii_whitespace() || byte == b'=' {
                continue;
            }
            state = WaveScan::Token;
        }
        if state == WaveScan::Token {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                wave.push(byte);
                continue;
            }
            state = WaveScan::Trailing;
        }
    }

    let speech = if wave.is_empty() {
        None
    } else {
        let mut token = decode_cp1252(&wave);
        if token
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_digit())
        {
            token.push('e');
        }
        Some(token)
    };
    Ok((out, speech))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaveScan {
    Leading,
    Token,
    Trailing,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    line: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            line: 0,
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at = self.at.saturating_add(1);
        if byte == b'\n' {
            self.line = self.line.saturating_add(1);
        }
        Some(byte)
    }

    fn read_line(&mut self) -> Option<(&'a [u8], usize)> {
        if self.at >= self.bytes.len() {
            return None;
        }
        let started_on = self.line.saturating_add(1);
        let start = self.at;
        while let Some(&byte) = self.bytes.get(self.at) {
            self.at = self.at.saturating_add(1);
            if byte == b'\n' {
                self.line = self.line.saturating_add(1);
                return Some((
                    self.bytes.get(start..self.at.saturating_sub(1))?,
                    started_on,
                ));
            }
        }
        Some((self.bytes.get(start..)?, started_on))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"// String file\r\n\r\nAPT:SoloPlay\r\n// context: a comment\r\n\"SOLO PLAY\"\r\nEND\r\n\r\nAPT:Multi\r\n\"MULTIPLAYER\"\r\nEND\r\n";

    #[test]
    fn parses_the_shipped_three_line_record() {
        let records = parse_str(SAMPLE).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].label, "APT:SoloPlay");
        assert_eq!(records[0].text, "SOLO PLAY");
        assert_eq!(records[1].label, "APT:Multi");
        assert_eq!(records[1].text, "MULTIPLAYER");
    }

    #[test]
    fn a_comment_line_inside_a_record_is_not_a_label() {
        let records = parse_str(SAMPLE).unwrap();
        assert!(records.iter().all(|r| !r.label.starts_with("//")));
    }

    #[test]
    fn a_string_may_run_across_lines_and_joins_with_one_space() {
        let records = parse_str(b"K\r\n\"one\r\ntwo\"\r\nEND\r\n").unwrap();
        assert_eq!(records[0].text, "one two");
    }

    #[test]
    fn escaped_quote_does_not_close_the_string() {
        let records = parse_str(b"K\r\n\"a \\\"b\\\" c\"\r\nEND\r\n").unwrap();
        assert_eq!(records[0].text, "a \"b\" c");
    }

    #[test]
    fn a_literal_backslash_n_becomes_a_newline_and_loses_the_space_before_it() {
        let records = parse_str(b"K\r\n\"a \\n b\"\r\nEND\r\n").unwrap();
        assert_eq!(records[0].text, "a\nb");
    }

    #[test]
    fn a_wave_token_after_the_quote_is_captured_and_digit_suffixed() {
        let records = parse_str(b"K\r\n\"hi\" tone01\r\nEND\r\n").unwrap();
        assert_eq!(records[0].speech.as_deref(), Some("tone01e"));
        let records = parse_str(b"K\r\n\"hi\" tone\r\nEND\r\n").unwrap();
        assert_eq!(records[0].speech.as_deref(), Some("tone"));
        let records = parse_str(b"K\r\n\"hi\"\r\nEND\r\n").unwrap();
        assert_eq!(records[0].speech, None);
    }

    #[test]
    fn only_the_first_string_in_a_record_is_used() {
        let records = parse_str(b"K\r\n\"first\"\r\n\"second\"\r\nEND\r\n").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].text, "first");
    }

    #[test]
    fn end_is_case_insensitive() {
        assert_eq!(parse_str(b"K\r\n\"v\"\r\nend\r\n").unwrap().len(), 1);
    }

    #[test]
    fn a_missing_end_is_an_error_naming_the_label_and_line() {
        let err = parse_str(b"// c\r\nK\r\n\"v\"\r\n").unwrap_err();
        assert_eq!(
            err,
            Error::UnterminatedRecord {
                line: 2,
                label: "K".to_owned()
            }
        );
    }

    #[test]
    fn a_missing_closing_quote_is_an_error_naming_the_line() {
        let err = parse_str(b"K\r\n\"v\r\n").unwrap_err();
        assert_eq!(err, Error::UnterminatedString { line: 2 });
    }

    #[test]
    fn lf_only_line_endings_parse_the_same_as_crlf() {
        let crlf = parse_str(SAMPLE).unwrap();
        let lf = parse_str(
            &SAMPLE
                .iter()
                .copied()
                .filter(|b| *b != b'\r')
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(crlf, lf);
    }

    #[test]
    fn truncation_sweep_never_panics() {
        for end in 0..SAMPLE.len() {
            let _ = parse_str(&SAMPLE[..end]);
        }
    }

    #[test]
    fn bit_corruption_sweep_never_panics() {
        for index in 0..SAMPLE.len() {
            for bit in 0..8u32 {
                let mut bytes = SAMPLE.to_vec();
                bytes[index] ^= 1 << bit;
                let _ = parse_str(&bytes);
            }
        }
    }

    #[test]
    fn a_file_that_is_one_enormous_unterminated_quote_errors_rather_than_hanging() {
        let mut bytes = b"K\r\n\"".to_vec();
        bytes.extend(std::iter::repeat_n(b'x', 200_000));
        assert_eq!(
            parse_str(&bytes).unwrap_err(),
            Error::UnterminatedString { line: 2 }
        );
    }
}
