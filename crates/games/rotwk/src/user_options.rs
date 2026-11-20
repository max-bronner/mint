use std::path::{Path, PathBuf};

const OPTIONS_FILE: &str = "Options.ini";
const SKIRMISH_FILE: &str = "Skirmish.ini";
const BONUS_KEY: &str = "BCU";
const SCROLL_FACTOR_KEY: &str = "ScrollFactor";
const HEALTH_BARS_KEY: &str = "AllHealthBars";
const SELECTED_NAME_KEY: &str = "CurrentUserName";
const DEFAULT_EOL: &[u8] = b"\r\n";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct UserOptions {
    raw: Vec<u8>,
}

struct Entry {
    key: String,
    value: std::ops::Range<usize>,
}

fn trim_range(bytes: &[u8], mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start = start.saturating_add(1);
    }
    while end > start
        && bytes
            .get(end.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        end = end.saturating_sub(1);
    }
    (start, end)
}

impl UserOptions {
    pub(crate) fn parse(bytes: &[u8]) -> Self {
        Self {
            raw: bytes.to_vec(),
        }
    }

    #[cfg(test)]
    pub(crate) fn render(&self) -> &[u8] {
        &self.raw
    }

    fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < self.raw.len() {
            let rest = self.raw.get(at..).unwrap_or_default();
            let length = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
            let line_end = at.saturating_add(length);
            if let Some(eq) = rest
                .get(..length)
                .and_then(|line| line.iter().position(|b| *b == b'='))
            {
                let eq = at.saturating_add(eq);
                let (key_start, key_end) = trim_range(&self.raw, at, eq);
                let (value_start, value_end) =
                    trim_range(&self.raw, eq.saturating_add(1), line_end);
                if key_start < key_end && value_start < value_end {
                    let key = self.raw.get(key_start..key_end).unwrap_or_default();
                    out.push(Entry {
                        key: String::from_utf8_lossy(key).into_owned(),
                        value: value_start..value_end,
                    });
                }
            }
            at = line_end.saturating_add(1);
        }
        out
    }

    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let entry = self.entries().into_iter().rev().find(|e| e.key == key)?;
        let bytes = self.raw.get(entry.value)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub(crate) fn set(&mut self, key: &str, value: &str) -> bool {
        if self.raw.contains(&0) || self.get(key).as_deref() == Some(value) {
            return false;
        }
        if let Some(entry) = self.entries().into_iter().rev().find(|e| e.key == key) {
            self.raw.splice(entry.value, value.bytes());
            return true;
        }
        let eol = self.eol();
        if !self.raw.is_empty() && self.raw.last() != Some(&b'\n') {
            self.raw.extend_from_slice(eol);
        }
        self.raw.extend_from_slice(key.as_bytes());
        self.raw.extend_from_slice(b" = ");
        self.raw.extend_from_slice(value.as_bytes());
        self.raw.extend_from_slice(eol);
        true
    }

    fn eol(&self) -> &'static [u8] {
        match self.raw.iter().position(|b| *b == b'\n') {
            Some(0) => b"\n",
            Some(at) if self.raw.get(at.saturating_sub(1)) == Some(&b'\r') => b"\r\n",
            Some(_) => b"\n",
            None => DEFAULT_EOL,
        }
    }

    pub(crate) fn bonus_unlocked(&self) -> bool {
        self.get(BONUS_KEY)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }

    pub(crate) fn all_health_bars(&self) -> bool {
        self.get(HEALTH_BARS_KEY).is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "t" | "true" | "y" | "yes" | "ok"
            )
        })
    }

    pub(crate) fn set_all_health_bars(&mut self, on: bool) -> bool {
        if self.all_health_bars() == on {
            return false;
        }
        self.set(HEALTH_BARS_KEY, if on { "yes" } else { "no" })
    }

    pub(crate) fn unlock_bonus(&mut self) {
        self.set(BONUS_KEY, "true");
    }

    pub(crate) fn scroll_factor(&self) -> Option<f32> {
        let value = self.get(SCROLL_FACTOR_KEY)?;
        let number = value.trim().parse::<i32>().unwrap_or(0);
        let held = if number < 0 { 1 } else { number.min(100) };
        Some(f32::from(u8::try_from(held).unwrap_or(100)) * 0.02)
    }
    pub(crate) fn skirmish_user_name(&self) -> String {
        self.get(SELECTED_NAME_KEY)
            .map(|value| quoted_printable_to_string(&value).trim().to_owned())
            .unwrap_or_default()
    }

    fn load_file(dir: &Path, name: &str) -> Self {
        std::fs::read(dir.join(name))
            .map(|bytes| Self::parse(&bytes))
            .unwrap_or_default()
    }

    fn save_file(&self, dir: &Path, name: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let temporary = dir.join(format!("{name}.mint-tmp"));
        std::fs::write(&temporary, &self.raw)?;
        std::fs::rename(&temporary, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })?;
        Ok(path)
    }

    #[cfg(test)]
    pub(crate) fn load_from(dir: &Path) -> Self {
        Self::load_file(dir, OPTIONS_FILE)
    }

    pub(crate) fn load() -> Self {
        crate::user_data::dir()
            .map_or_else(Self::default, |dir| Self::load_file(&dir, OPTIONS_FILE))
    }

    pub(crate) fn save(&self) -> std::io::Result<PathBuf> {
        self.save_in_user_dir(OPTIONS_FILE)
    }

    pub(crate) fn load_skirmish() -> Self {
        crate::user_data::dir()
            .map_or_else(Self::default, |dir| Self::load_file(&dir, SKIRMISH_FILE))
    }

    pub(crate) fn save_skirmish(&self) -> std::io::Result<PathBuf> {
        self.save_in_user_dir(SKIRMISH_FILE)
    }

    fn save_in_user_dir(&self, name: &str) -> std::io::Result<PathBuf> {
        let dir = crate::user_data::dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no user data folder is known")
        })?;
        self.save_file(&dir, name)
    }
}

fn quoted_printable_to_string(text: &str) -> String {
    const MAX_BYTES: usize = 2046;
    let hex = |digit: u8| {
        char::from(digit)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
            .unwrap_or(0)
    };
    let source = text.as_bytes();
    let mut bytes = Vec::new();
    let mut at = 0;
    while let Some(&byte) = source.get(at) {
        if bytes.len() >= MAX_BYTES {
            break;
        }
        if byte != b'_' {
            bytes.push(byte);
        } else {
            let Some(&high) = source.get(at + 1) else {
                break;
            };
            at += 1;
            let mut value = hex(high);
            if let Some(&low) = source.get(at + 1) {
                value = (value << 4) | hex(low);
                at += 1;
            }
            bytes.push(value);
        }
        at += 1;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .filter_map(|pair| match pair {
            [low, high] => Some(u16::from_le_bytes([*low, *high])),
            _ => None,
        })
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[allow(dead_code)]
pub(crate) fn set_bonus_unlocked() -> std::io::Result<PathBuf> {
    let mut options = UserOptions::load();
    options.unlock_bonus();
    options.save()
}

pub(crate) fn store_all_health_bars(on: bool) {
    let mut options = UserOptions::load();
    if !options.set_all_health_bars(on) {
        return;
    }
    match options.save() {
        Ok(path) => println!(
            "options: AllHealthBars = {} saved to {}",
            on,
            path.display()
        ),
        Err(error) => println!("options: could not write Options.ini ({error})"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    const RETAIL: &[u8] = b"AllHealthBars = yes\r\nAmbientVolume = 50.000000\r\nZeta = 1\r\n";

    #[test]
    fn an_edit_changes_only_its_own_value() {
        let mut options = UserOptions::parse(RETAIL);
        assert!(options.set_all_health_bars(false));
        assert_eq!(
            options.render(),
            b"AllHealthBars = no\r\nAmbientVolume = 50.000000\r\nZeta = 1\r\n"
        );
    }

    #[test]
    fn a_missing_key_is_appended_in_the_files_line_endings() {
        let mut options = UserOptions::parse(b"Zeta = 1\nAlpha = 2");
        assert!(options.set_all_health_bars(true));
        assert_eq!(
            options.render(),
            b"Zeta = 1\nAlpha = 2\nAllHealthBars = yes\n"
        );
        let mut fresh = UserOptions::default();
        assert!(fresh.set_all_health_bars(true));
        assert_eq!(fresh.render(), b"AllHealthBars = yes\r\n");
    }

    #[test]
    fn a_value_retail_reads_the_same_is_not_rewritten() {
        let mut options = UserOptions::parse(b"AllHealthBars = TRUE\r\n");
        assert!(options.all_health_bars());
        assert!(!options.set_all_health_bars(true));
        assert_eq!(options.render(), b"AllHealthBars = TRUE\r\n");
        let mut off = UserOptions::parse(b"Zeta = 1\r\n");
        assert!(!off.set_all_health_bars(false), "absent already means off");
    }

    #[test]
    fn a_file_that_is_not_ascii_text_is_not_edited() {
        let utf16 = b"A\0l\0l\0H\0=\0y\0\r\0\n\0".to_vec();
        let mut options = UserOptions::parse(&utf16);
        assert!(!options.set_all_health_bars(true));
        assert_eq!(options.render(), utf16.as_slice());
    }

    #[test]
    fn the_last_duplicate_wins_and_is_the_one_edited() {
        let mut options = UserOptions::parse(b"K = a\nK = b\n");
        assert_eq!(options.get("K").as_deref(), Some("b"));
        assert!(options.set("K", "c"));
        assert_eq!(options.render(), b"K = a\nK = c\n");
    }

    #[test]
    fn lines_without_a_key_or_a_value_are_ignored_but_kept() {
        let mut options = UserOptions::parse(b"junk\n= 5\nKeyOnly =\nKept = x\n");
        assert_eq!(options.get("KeyOnly"), None);
        assert_eq!(options.get("Kept").as_deref(), Some("x"));
        assert!(options.set("Kept", "y"));
        assert_eq!(options.render(), b"junk\n= 5\nKeyOnly =\nKept = y\n");
    }

    #[test]
    fn bcu_compares_case_insensitively() {
        for (text, expected) in [
            ("BCU = TRUE\n", true),
            ("BCU = tRuE\n", true),
            ("BCU = 1\n", false),
            ("BCU = false\n", false),
            ("bcu = true\n", false),
            ("", false),
        ] {
            assert_eq!(
                UserOptions::parse(text.as_bytes()).bonus_unlocked(),
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_selected_user_name_is_decoded_from_hex_escaped_utf16() {
        let file = b"CurrentUserName = t_00e_00s_00t_00\nUserNames = t_00e_00s_00t_00_2C_00\n";
        assert_eq!(UserOptions::parse(file).skirmish_user_name(), "test");
        let spaced = b"CurrentUserName = _20_00A_00b_00_20_00\n";
        assert_eq!(UserOptions::parse(spaced).skirmish_user_name(), "Ab");
        let accented = b"CurrentUserName = _E9_00x_00\n";
        assert_eq!(UserOptions::parse(accented).skirmish_user_name(), "\u{e9}x");
    }

    #[test]
    fn no_selected_user_name_is_the_empty_string() {
        assert_eq!(UserOptions::parse(b"").skirmish_user_name(), "");
        let listed_only = b"UserNames = t_00e_00s_00t_00_2C_00\n";
        assert_eq!(UserOptions::parse(listed_only).skirmish_user_name(), "");
    }

    #[test]
    fn a_malformed_escape_ends_the_name_without_failing() {
        assert_eq!(quoted_printable_to_string("a_00b_00_"), "ab");
        assert_eq!(quoted_printable_to_string("a_00b_00_0"), "ab");
        assert_eq!(quoted_printable_to_string("_zz_zz"), "");
        assert_eq!(quoted_printable_to_string("a_00b"), "a");
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let dir = std::env::temp_dir().join("mint-user-options-test-missing");
        assert_eq!(UserOptions::load_from(&dir), UserOptions::default());
    }

    #[test]
    fn a_save_replaces_the_file_whole_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join("mint-user-options-test-save");
        let _ = std::fs::remove_dir_all(&dir);
        let mut options = UserOptions::parse(RETAIL);
        options.set_all_health_bars(false);
        options.save_file(&dir, OPTIONS_FILE).unwrap();
        let again = UserOptions::load_from(&dir);
        assert_eq!(again, options);
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from(OPTIONS_FILE)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
