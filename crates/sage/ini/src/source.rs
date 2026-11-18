use std::collections::{BTreeMap, BTreeSet};

use crate::lex::{decode_latin1, lex, Directive, Lexed};

pub trait Sources {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

#[must_use]
pub fn normalise_path(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

#[must_use]
pub fn resolve_include(current: &str, relative: &str) -> String {
    let current = normalise_path(current);
    let directory = current.rsplit_once('/').map_or("", |(dir, _)| dir);
    let relative = normalise_path(relative);

    let mut parts: Vec<&str> = if directory.is_empty() {
        Vec::new()
    } else {
        directory.split('/').collect()
    };
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[derive(Debug, Clone, Default)]
pub struct MemorySources {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemorySources {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(normalise_path(path), bytes);
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Sources for MemorySources {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }
}

#[must_use]
pub fn included_paths(sources: &MemorySources) -> BTreeSet<String> {
    let mut included = BTreeSet::new();
    for path in sources.paths() {
        let Some(bytes) = sources.read(path) else {
            continue;
        };
        for item in lex(&decode_latin1(&bytes)) {
            if let Lexed::Directive {
                directive: Directive::Include(target),
                ..
            } = item
            {
                included.insert(resolve_include(path, target));
            }
        }
    }
    included
}

impl<T: Sources + ?Sized> Sources for &T {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        (**self).read(path)
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn includes_resolve_relative_to_the_including_file() {
        assert_eq!(
            resolve_include(
                "data/ini/object/goodfaction/units/men/aragorn.ini",
                r"..\..\..\Includes\StunDrawModuleSmall.inc"
            ),
            "data/ini/object/includes/stundrawmodulesmall.inc"
        );
        assert_eq!(
            resolve_include("data/ini/x.ini", "CreateAHero.inc"),
            "data/ini/createahero.inc"
        );
        assert_eq!(
            resolve_include("data/ini/o.ini", r"Includes\a.inc"),
            resolve_include("data/ini/o.ini", r"includes\a.inc")
        );
    }

    #[test]
    fn a_relative_path_cannot_escape_above_the_root() {
        assert_eq!(resolve_include("a.ini", r"..\..\..\b.inc"), "b.inc");
        assert_eq!(resolve_include("", ".."), "");
        assert_eq!(resolve_include("", ""), "");
    }

    #[test]
    fn memory_sources_fold_case_and_separators() {
        let mut sources = MemorySources::new();
        sources.insert(r"Data\INI\Object\Rock.ini", b"Object Rock\nEnd\n".to_vec());
        assert_eq!(sources.len(), 1);
        assert!(sources.read("data/ini/object/rock.ini").is_some());
        assert!(sources.read("Data\\INI\\Object\\Rock.ini").is_none());
    }
}
