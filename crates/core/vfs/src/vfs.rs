use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::{Error, Mount, Result, VfsPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountReport {
    pub listed: usize,
    pub shadowed: usize,
}

#[derive(Default)]
pub struct Vfs {
    mounts: Vec<Arc<dyn Mount>>,
    index: BTreeMap<VfsPath, usize>,
}

impl Vfs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mount(&mut self, mount: Arc<dyn Mount>) -> MountReport {
        let id = self.mounts.len();
        let entries = mount.entries();
        let listed = entries.len();
        let mut shadowed = 0usize;

        for path in entries {
            if self.index.insert(path, id).is_some() {
                shadowed = shadowed.saturating_add(1);
            }
        }
        self.mounts.push(mount);

        MountReport { listed, shadowed }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn mounts(&self) -> impl Iterator<Item = &dyn Mount> {
        self.mounts.iter().map(Arc::as_ref)
    }

    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&VfsPath::new(path))
    }

    #[must_use]
    pub fn origin(&self, path: &str) -> Option<&dyn Mount> {
        self.mount_for(&VfsPath::new(path))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.read_path(&VfsPath::new(path))
    }

    pub fn read_path(&self, path: &VfsPath) -> Result<Vec<u8>> {
        let mount = self
            .mount_for(path)
            .ok_or_else(|| Error::NotFound { path: path.clone() })?;
        mount.read(path)
    }

    pub fn paths(&self) -> impl Iterator<Item = &VfsPath> {
        self.index.keys()
    }

    pub fn paths_under(&self, prefix: &str) -> impl Iterator<Item = &VfsPath> {
        let from = VfsPath::new(prefix);
        let test = from.clone();
        self.index
            .range(from..)
            .map(|(path, _)| path)
            .take_while(move |path| path.as_str().starts_with(test.as_str()))
    }

    fn mount_for(&self, path: &VfsPath) -> Option<&dyn Mount> {
        let id = *self.index.get(path)?;
        self.mounts.get(id).map(Arc::as_ref)
    }
}

impl fmt::Debug for Vfs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs")
            .field(
                "mounts",
                &self.mounts().map(Mount::label).collect::<Vec<_>>(),
            )
            .field("paths", &self.index.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeMount {
        label: String,
        files: Vec<(VfsPath, Vec<u8>)>,
    }

    impl FakeMount {
        fn new(label: &str, files: &[(&str, &[u8])]) -> Arc<Self> {
            Arc::new(Self {
                label: label.to_string(),
                files: files
                    .iter()
                    .map(|(name, body)| (VfsPath::new(name), body.to_vec()))
                    .collect(),
            })
        }
    }

    impl Mount for FakeMount {
        fn label(&self) -> &str {
            &self.label
        }

        fn entries(&self) -> Vec<VfsPath> {
            self.files.iter().map(|(path, _)| path.clone()).collect()
        }

        fn read(&self, path: &VfsPath) -> Result<Vec<u8>> {
            self.files
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, body)| body.clone())
                .ok_or_else(|| Error::NotFound { path: path.clone() })
        }
    }

    #[test]
    fn a_later_mount_shadows_an_earlier_one() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new("base", &[("a.txt", b"base")]));
        vfs.mount(FakeMount::new("patch", &[("a.txt", b"patch")]));

        assert_eq!(vfs.read("a.txt").unwrap(), b"patch");
    }

    #[test]
    fn shadowing_is_by_normalised_name_not_by_spelling() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new("base", &[(r"Data\A.TXT", b"base")]));
        vfs.mount(FakeMount::new("patch", &[("data/a.txt", b"patch")]));

        assert_eq!(vfs.len(), 1);
        assert_eq!(vfs.read(r"DATA/a.Txt").unwrap(), b"patch");
    }

    #[test]
    fn an_earlier_mount_still_supplies_what_the_later_one_lacks() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new(
            "base",
            &[("a.txt", b"base"), ("b.txt", b"b")],
        ));
        vfs.mount(FakeMount::new("patch", &[("a.txt", b"patch")]));

        assert_eq!(vfs.read("b.txt").unwrap(), b"b");
    }

    #[test]
    fn mounting_reports_how_many_entries_were_shadowed() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new("base", &[("a.txt", b"1"), ("b.txt", b"2")]));
        let report = vfs.mount(FakeMount::new("patch", &[("a.txt", b"3"), ("c.txt", b"4")]));

        assert_eq!(
            report,
            MountReport {
                listed: 2,
                shadowed: 1
            }
        );
    }

    #[test]
    fn origin_names_the_mount_that_won() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new("base", &[("a.txt", b"1")]));
        vfs.mount(FakeMount::new("patch", &[("a.txt", b"2")]));

        assert_eq!(vfs.origin("a.txt").map(Mount::label), Some("patch"));
    }

    #[test]
    fn reading_an_unmounted_path_is_not_found() {
        let vfs = Vfs::new();

        assert!(matches!(vfs.read("a.txt"), Err(Error::NotFound { .. })));
    }

    #[test]
    fn paths_are_enumerated_in_sorted_order() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new(
            "m",
            &[("c.txt", b""), ("a.txt", b""), ("b.txt", b"")],
        ));

        let paths: Vec<_> = vfs.paths().map(VfsPath::as_str).collect();
        assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn paths_under_returns_only_the_matching_prefix() {
        let mut vfs = Vfs::new();
        vfs.mount(FakeMount::new(
            "m",
            &[
                ("data/ini/a.ini", b""),
                ("data/ini/b.ini", b""),
                ("data/maps/c.map", b""),
                ("art/d.tga", b""),
            ],
        ));

        let found: Vec<_> = vfs.paths_under(r"Data\INI").map(VfsPath::as_str).collect();
        assert_eq!(found, ["data/ini/a.ini", "data/ini/b.ini"]);
    }

    #[test]
    fn a_mount_listing_a_name_twice_shadows_itself() {
        let mut vfs = Vfs::new();
        let report = vfs.mount(FakeMount::new(
            "m",
            &[(r"A\B.TXT", b"first"), ("a/b.txt", b"second")],
        ));

        assert_eq!(report.shadowed, 1);
        assert_eq!(vfs.len(), 1);
    }
}
