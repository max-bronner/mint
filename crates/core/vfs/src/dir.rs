use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::{Error, Mount, Result, VfsPath};

const MAX_DEPTH: usize = 64;

#[derive(Debug)]
pub struct DirMount {
    root: PathBuf,
    label: String,
    files: BTreeMap<VfsPath, PathBuf>,
}

impl DirMount {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Self::with_prefix(root, "")
    }

    pub fn with_prefix(root: impl AsRef<Path>, prefix: &str) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let prefix = VfsPath::new(prefix.trim_matches(['/', '\\']));
        let files = walk(&root, prefix.as_str())?;
        Ok(Self {
            label: root.display().to_string(),
            root,
            files,
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn real_path(&self, path: &VfsPath) -> Option<&Path> {
        self.files.get(path).map(PathBuf::as_path)
    }
}

impl Mount for DirMount {
    fn label(&self) -> &str {
        &self.label
    }

    fn entries(&self) -> Vec<VfsPath> {
        self.files.keys().cloned().collect()
    }

    fn read(&self, path: &VfsPath) -> Result<Vec<u8>> {
        let real = self
            .files
            .get(path)
            .ok_or_else(|| Error::NotFound { path: path.clone() })?;
        std::fs::read(real).map_err(|source| Error::Io {
            path: real.clone(),
            source,
        })
    }
}

fn walk(root: &Path, prefix: &str) -> Result<BTreeMap<VfsPath, PathBuf>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let listing = std::fs::read_dir(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;

        for entry in listing {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;

            if file_type.is_dir() {
                if depth >= MAX_DEPTH {
                    return Err(Error::TooDeep {
                        path,
                        limit: MAX_DEPTH,
                    });
                }
                stack.push((path, depth.saturating_add(1)));
            } else if file_type.is_file() {
                insert(&mut files, root, &path, prefix);
            } else if std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                insert(&mut files, root, &path, prefix);
            }
        }
    }

    Ok(files)
}

fn insert(files: &mut BTreeMap<VfsPath, PathBuf>, root: &Path, file: &Path, prefix: &str) {
    if let Some(logical) = logical_path(root, file, prefix) {
        files.insert(logical, file.to_path_buf());
    }
}

fn logical_path(root: &Path, file: &Path, prefix: &str) -> Option<VfsPath> {
    let relative = file.strip_prefix(root).ok()?;
    let mut name = String::new();

    if !prefix.is_empty() {
        name.push_str(prefix);
    }
    for component in relative.components() {
        if let Component::Normal(part) = component {
            if !name.is_empty() {
                name.push('/');
            }
            name.push_str(&part.to_string_lossy());
        }
    }

    if name.is_empty() || name == prefix {
        None
    } else {
        Some(VfsPath::new(&name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempTree;

    #[test]
    fn a_tree_is_visible_as_slash_separated_lowercase_paths() {
        let tree = TempTree::new("dir_basic");
        tree.write("Data/INI/GameData.INI", b"x");

        let mount = DirMount::new(tree.root()).unwrap();

        assert_eq!(mount.entries(), vec![VfsPath::new("data/ini/gamedata.ini")]);
    }

    #[test]
    fn a_prefix_is_prepended_to_every_entry() {
        let tree = TempTree::new("dir_prefix");
        tree.write("ini/x.ini", b"x");

        let mount = DirMount::with_prefix(tree.root(), "Data").unwrap();

        assert_eq!(mount.entries(), vec![VfsPath::new("data/ini/x.ini")]);
    }

    #[test]
    fn reading_an_entry_returns_the_file_contents() {
        let tree = TempTree::new("dir_read");
        tree.write("a/b.txt", b"hello");

        let mount = DirMount::new(tree.root()).unwrap();

        assert_eq!(mount.read(&VfsPath::new(r"A\B.TXT")).unwrap(), b"hello");
    }

    #[test]
    fn reading_a_path_the_mount_does_not_have_is_not_found() {
        let tree = TempTree::new("dir_missing");
        tree.write("a.txt", b"x");

        let mount = DirMount::new(tree.root()).unwrap();

        assert!(matches!(
            mount.read(&VfsPath::new("b.txt")),
            Err(Error::NotFound { .. })
        ));
    }

    #[test]
    fn mounting_a_directory_that_does_not_exist_is_an_io_error() {
        let tree = TempTree::new("dir_absent");
        let missing = tree.root().join("nope");

        assert!(matches!(DirMount::new(missing), Err(Error::Io { .. })));
    }

    #[test]
    fn a_tree_deeper_than_the_cap_errors_rather_than_walking_forever() {
        let tree = TempTree::new("dir_deep");
        let mut deep = String::new();
        for _ in 0..=MAX_DEPTH {
            deep.push_str("d/");
        }
        deep.push_str("x.txt");
        tree.write(&deep, b"x");

        assert!(matches!(
            DirMount::new(tree.root()),
            Err(Error::TooDeep { .. })
        ));
    }
}
