use std::collections::BTreeSet;

const XOR_KEY: u16 = 0x5555;
const SEPARATOR: u16 = 0x0020;
const DELIMITERS: &str = " ;,.!?:=\\/><`~()&^%#\n\t";
const IGNORED: &str = "-_*'\"";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageFilter {
    words: BTreeSet<Vec<u16>>,
}

fn fold(unit: u16) -> u16 {
    char::from_u32(u32::from(unit))
        .and_then(|c| c.to_lowercase().next())
        .and_then(|c| u16::try_from(u32::from(c)).ok())
        .unwrap_or(unit)
}

fn unit_in(set: &str, unit: u16) -> bool {
    set.encode_utf16().any(|c| c == unit)
}

fn unhaxor(word: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(word.len());
    let mut units = word.iter().copied().peekable();
    while let Some(unit) = units.next() {
        let letter = |c: char| u16::try_from(u32::from(c)).unwrap_or(unit);
        let mapped = match char::from_u32(u32::from(unit)) {
            Some('p' | 'P') => {
                if matches!(
                    units
                        .peek()
                        .copied()
                        .and_then(|n| char::from_u32(u32::from(n))),
                    Some('h' | 'H')
                ) {
                    units.next();
                    letter('f')
                } else {
                    unit
                }
            }
            Some('1') => letter('l'),
            Some('3') => letter('e'),
            Some('4' | '@') => letter('a'),
            Some('5' | '$') => letter('s'),
            Some('6') => letter('b'),
            Some('7' | '+') => letter('t'),
            Some('0') => letter('o'),
            _ if unit_in(IGNORED, unit) => continue,
            _ => unit,
        };
        out.push(mapped);
    }
    out.iter().map(|&unit| fold(unit)).collect()
}

impl LanguageFilter {
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Self {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| {
                u16::from_le_bytes([
                    pair.first().copied().unwrap_or(0),
                    pair.get(1).copied().unwrap_or(0),
                ])
            })
            .collect();
        let words = units
            .split(|&unit| unit == SEPARATOR)
            .filter(|word| !word.is_empty())
            .map(|word| {
                let plain: Vec<u16> = word.iter().map(|&unit| unit ^ XOR_KEY).collect();
                unhaxor(&plain)
            })
            .collect();
        Self { words }
    }

    #[must_use]
    pub fn filter_line(&self, line: &str) -> String {
        if self.words.is_empty() {
            return line.to_owned();
        }
        let units: Vec<u16> = line.encode_utf16().collect();
        let mut buffer = units.clone();
        let star = u16::try_from(u32::from('*')).unwrap_or(0);
        for token in units.split(|&unit| unit_in(DELIMITERS, unit)) {
            if token.is_empty() {
                continue;
            }
            let Some(at) = buffer
                .windows(token.len())
                .position(|window| window == token)
            else {
                continue;
            };
            if self.words.contains(&unhaxor(token)) {
                for unit in buffer.iter_mut().skip(at).take(token.len()) {
                    *unit = star;
                }
            }
        }
        String::from_utf16_lossy(&buffer)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn file(words: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for word in words {
            for unit in word.encode_utf16() {
                out.extend_from_slice(&(unit ^ XOR_KEY).to_le_bytes());
            }
            out.extend_from_slice(&SEPARATOR.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_listed_word_becomes_stars_and_the_rest_is_kept() {
        let filter = LanguageFilter::parse(&file(&["frodo", "orc"]));
        assert_eq!(
            filter.filter_line("Big Frodo, the orc!"),
            "Big *****, the ***!"
        );
    }

    #[test]
    fn disguised_spellings_match_their_plain_word() {
        let filter = LanguageFilter::parse(&file(&["flower"]));
        assert_eq!(filter.filter_line("ph10wer x"), "******* x");
        assert_eq!(filter.filter_line("fl-ower"), "*******");
    }

    #[test]
    fn a_file_that_ends_mid_unit_still_reads() {
        let mut bytes = file(&["orc"]);
        bytes.push(0x41);
        assert_eq!(LanguageFilter::parse(&bytes).filter_line("orc"), "***");
    }
}
