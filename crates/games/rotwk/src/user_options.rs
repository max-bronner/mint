use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "Options.ini";
const BONUS_KEY: &str = "BCU";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct UserOptions {
    entries: BTreeMap<String, String>,
}

impl UserOptions {
    pub(crate) fn parse(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if !key.is_empty() && !value.is_empty() {
                entries.insert(key.to_owned(), value.to_owned());
            }
        }
        Self { entries }
    }

    pub(crate) fn render(&self) -> String {
        self.entries
            .iter()
            .map(|(key, value)| format!("{key} = {value}\n"))
            .collect()
    }

    pub(crate) fn bonus_unlocked(&self) -> bool {
        self.entries
            .get(BONUS_KEY)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }

    pub(crate) fn unlock_bonus(&mut self) {
        self.entries.insert(BONUS_KEY.to_owned(), "true".to_owned());
    }

    pub(crate) fn load_from(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE_NAME))
            .map(|bytes| Self::parse(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default()
    }

    pub(crate) fn save_to(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, self.render())?;
        Ok(path)
    }

    pub(crate) fn load() -> Self {
        crate::user_data::dir().map_or_else(Self::default, |dir| Self::load_from(&dir))
    }
}

#[allow(dead_code)]
pub(crate) fn set_bonus_unlocked() -> std::io::Result<PathBuf> {
    let dir = crate::user_data::dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no user data folder is known")
    })?;
    let mut options = UserOptions::load_from(&dir);
    options.unlock_bonus();
    options.save_to(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewrite_keeps_entries_it_does_not_know() {
        let mut options =
            UserOptions::parse("Zeta = 1\nGammaTest = 0.5\n  Name With Space  =  a=b \n");
        options.unlock_bonus();
        let again = UserOptions::parse(&options.render());
        assert_eq!(again.entries.get("Zeta").map(String::as_str), Some("1"));
        assert_eq!(
            again.entries.get("GammaTest").map(String::as_str),
            Some("0.5")
        );
        assert_eq!(
            again.entries.get("Name With Space").map(String::as_str),
            Some("a=b")
        );
        assert!(again.bonus_unlocked());
        assert_eq!(options.render(), again.render());
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
                UserOptions::parse(text).bonus_unlocked(),
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let dir = std::env::temp_dir().join("mint-user-options-test-missing");
        assert_eq!(UserOptions::load_from(&dir), UserOptions::default());
        assert!(!UserOptions::load_from(&dir).bonus_unlocked());
    }

    #[test]
    fn lines_without_a_key_or_a_value_are_dropped() {
        let options = UserOptions::parse("junk\n= 5\nKeyOnly =\nKept = x\n");
        assert_eq!(options.render(), "Kept = x\n");
    }
}
