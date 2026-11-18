use std::sync::Arc;

use mint_vfs::{Mount, VfsPath};

use crate::AssetKind;

type Entry = (String, AssetKind, u32, u32);

pub(crate) struct CatalogueBuilder {
    records: Vec<(String, u64, Vec<Entry>)>,
    dependencies: Vec<(String, String, Vec<String>)>,
}

impl CatalogueBuilder {
    pub(crate) fn new() -> Self {
        Self {
            records: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    pub(crate) fn record(
        mut self,
        name: &str,
        timestamp: u64,
        entries: &[(&str, AssetKind, u32, u32)],
    ) -> Self {
        self.records.push((
            name.to_owned(),
            timestamp,
            entries
                .iter()
                .map(|(name, kind, offset, size)| ((*name).to_owned(), *kind, *offset, *size))
                .collect(),
        ));
        self
    }

    pub(crate) fn dependency(mut self, file: &str, sub_asset: &str, references: &[&str]) -> Self {
        self.dependencies.push((
            file.to_owned(),
            sub_asset.to_owned(),
            references.iter().map(|name| (*name).to_owned()).collect(),
        ));
        self
    }

    pub(crate) fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ALAE");
        out.extend_from_slice(&crate::VERSION.to_le_bytes());
        out.extend_from_slice(&u32::try_from(self.records.len()).unwrap().to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(self.dependencies.len())
                .unwrap()
                .to_le_bytes(),
        );

        for (name, timestamp, entries) in &self.records {
            push_name(&mut out, name);
            out.extend_from_slice(
                &u32::try_from(timestamp & 0xFFFF_FFFF)
                    .unwrap()
                    .to_le_bytes(),
            );
            out.extend_from_slice(
                &u32::try_from(timestamp.wrapping_shr(32))
                    .unwrap()
                    .to_le_bytes(),
            );
            out.extend_from_slice(&u16::try_from(entries.len()).unwrap().to_le_bytes());
            for (name, kind, offset, size) in entries {
                push_name(&mut out, name);
                out.extend_from_slice(&kind.tag().to_le_bytes());
                out.extend_from_slice(&offset.to_le_bytes());
                out.extend_from_slice(&size.to_le_bytes());
            }
        }

        for (file, sub_asset, references) in &self.dependencies {
            push_name(&mut out, file);
            push_name(&mut out, sub_asset);
            out.extend_from_slice(&u16::try_from(references.len()).unwrap().to_le_bytes());
            for reference in references {
                push_name(&mut out, reference);
            }
        }

        out
    }
}

fn push_name(out: &mut Vec<u8>, name: &str) {
    out.push(u8::try_from(name.len()).unwrap());
    out.extend_from_slice(name.as_bytes());
}

pub(crate) fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at.saturating_add(4)].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at.saturating_add(2)].copy_from_slice(&value.to_le_bytes());
}

pub(crate) struct MemMount {
    label: String,
    paths: Vec<VfsPath>,
}

impl MemMount {
    pub(crate) fn new(label: &str, paths: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            label: label.to_owned(),
            paths: paths.iter().map(|path| VfsPath::new(path)).collect(),
        })
    }
}

impl Mount for MemMount {
    fn label(&self) -> &str {
        &self.label
    }

    fn entries(&self) -> Vec<VfsPath> {
        self.paths.clone()
    }

    fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
        if self.paths.contains(path) {
            Ok(path.as_str().as_bytes().to_vec())
        } else {
            Err(mint_vfs::Error::NotFound { path: path.clone() })
        }
    }
}
