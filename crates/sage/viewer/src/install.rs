use std::path::{Path, PathBuf};
use std::sync::Arc;

use mint_sage_big::BigArchive;
use mint_vfs::{DirMount, Vfs};

use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountSummary {
    pub archives: usize,
    pub entries: usize,
    pub loose: usize,
    pub failures: Vec<String>,
}

pub fn mount_install(vfs: &mut Vfs, root: &Path) -> Result<MountSummary> {
    if !root.is_dir() {
        return Err(Error::NoInstall {
            path: root.to_path_buf(),
        });
    }

    let mut summary = MountSummary::default();
    for path in archives(root) {
        match BigArchive::open(&path) {
            Ok(archive) => {
                summary.archives = summary.archives.saturating_add(1);
                let report = vfs.mount(Arc::new(archive));
                summary.entries = summary.entries.saturating_add(report.listed);
            }
            Err(error) => summary
                .failures
                .push(format!("{}: {error}", path.display())),
        }
    }

    let data = root.join("data");
    if data.is_dir() {
        match DirMount::with_prefix(&data, "data") {
            Ok(mount) => {
                let report = vfs.mount(Arc::new(mount));
                summary.loose = report.listed;
            }
            Err(error) => summary
                .failures
                .push(format!("{}: {error}", data.display())),
        }
    }

    Ok(summary)
}

fn archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    let is_big = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("big"));
                    if is_big {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_is_not_a_directory_is_an_error_and_not_an_empty_mount() {
        let mut vfs = Vfs::new();
        let missing = Path::new("no-such-installation-anywhere");
        assert!(matches!(
            mount_install(&mut vfs, missing),
            Err(Error::NoInstall { .. })
        ));
        assert!(vfs.is_empty());
    }

    #[test]
    fn a_directory_with_no_archives_mounts_nothing_and_says_so() {
        let mut vfs = Vfs::new();
        let summary = mount_install(&mut vfs, Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("the manifest directory exists");
        assert_eq!(summary.archives, 0);
        assert_eq!(summary.entries, 0);
        assert!(summary.failures.is_empty());
    }
}
