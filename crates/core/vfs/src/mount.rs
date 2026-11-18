use crate::{Result, VfsPath};

pub trait Mount: Send + Sync {
    fn label(&self) -> &str;

    fn entries(&self) -> Vec<VfsPath>;

    fn read(&self, path: &VfsPath) -> Result<Vec<u8>>;
}
