use std::borrow::Borrow;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VfsPath(String);

impl VfsPath {
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let mut out = String::with_capacity(raw.len());
        for ch in raw.chars() {
            out.push(if ch == '\\' {
                '/'
            } else {
                ch.to_ascii_lowercase()
            });
        }
        Self(out)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        let name = self.0.rsplit('/').next()?;
        let (stem, ext) = name.rsplit_once('.')?;
        if stem.is_empty() {
            None
        } else {
            Some(ext)
        }
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for VfsPath {
    fn from(raw: &str) -> Self {
        Self::new(raw)
    }
}

impl Borrow<str> for VfsPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_and_case_are_normalised() {
        assert_eq!(
            VfsPath::new(r"Data\INI\GameData.INI").as_str(),
            "data/ini/gamedata.ini"
        );
    }

    #[test]
    fn the_two_spellings_of_a_name_compare_equal() {
        assert_eq!(VfsPath::new(r"A\B\c.TXT"), VfsPath::new("a/b/c.txt"));
    }

    #[test]
    fn non_ascii_bytes_pass_through_unchanged() {
        assert_eq!(VfsPath::new("DÉJÀ/VU").as_str(), "dÉjÀ/vu");
    }

    #[test]
    fn dot_segments_are_left_alone() {
        assert_eq!(VfsPath::new(r"a\..\b").as_str(), "a/../b");
    }

    #[test]
    fn a_leading_separator_is_kept() {
        assert_eq!(VfsPath::new(r"\a\b").as_str(), "/a/b");
    }

    #[test]
    fn extension_is_the_text_after_the_final_dot() {
        assert_eq!(VfsPath::new("a/b.TGA").extension(), Some("tga"));
        assert_eq!(VfsPath::new("a/b").extension(), None);
        assert_eq!(VfsPath::new("a.b/c").extension(), None);
        assert_eq!(VfsPath::new("a/.hidden").extension(), None);
    }
}
