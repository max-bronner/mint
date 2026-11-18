use std::path::{Path, PathBuf};

pub(crate) struct TempTree {
    root: PathBuf,
}

impl TempTree {
    pub(crate) fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("mint-vfs-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp tree");
        Self { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent directory");
        }
        std::fs::write(&path, bytes).expect("write temp file");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
