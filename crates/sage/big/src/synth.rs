use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct BigBuilder {
    magic: [u8; 4],
    files: Vec<(String, Vec<u8>)>,
    padding: usize,
}

impl BigBuilder {
    pub(crate) fn new() -> Self {
        Self {
            magic: *b"BIG4",
            files: Vec::new(),
            padding: 0,
        }
    }

    pub(crate) fn magic(mut self, magic: &[u8; 4]) -> Self {
        self.magic = *magic;
        self
    }

    pub(crate) fn padding(mut self, bytes: usize) -> Self {
        self.padding = bytes;
        self
    }

    pub(crate) fn file(mut self, name: &str, data: &[u8]) -> Self {
        self.files.push((name.to_string(), data.to_vec()));
        self
    }

    pub(crate) fn build(&self) -> Vec<u8> {
        let table_len: usize = self
            .files
            .iter()
            .map(|(name, _)| name.len().saturating_add(9))
            .sum::<usize>();
        let data_offset = table_len.saturating_add(16).saturating_add(self.padding);

        let mut header = Vec::new();
        let mut table = Vec::new();
        let mut data = Vec::new();

        for (name, bytes) in &self.files {
            let offset = data_offset.saturating_add(data.len());
            table.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes());
            table.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_be_bytes());
            table.extend_from_slice(name.as_bytes());
            table.push(0);
            data.extend_from_slice(bytes);
        }

        let total = data_offset.saturating_add(data.len());
        header.extend_from_slice(&self.magic);
        header.extend_from_slice(&u32::try_from(total).unwrap().to_le_bytes());
        header.extend_from_slice(&u32::try_from(self.files.len()).unwrap().to_be_bytes());
        header.extend_from_slice(&u32::try_from(data_offset).unwrap().to_be_bytes());

        let mut out = header;
        out.extend_from_slice(&table);
        out.extend_from_slice(&vec![0u8; self.padding]);
        out.extend_from_slice(&data);
        out
    }
}

pub(crate) fn put_be_u32(bytes: &mut [u8], at: usize, value: u32) {
    let end = at.saturating_add(4);
    if let Some(slot) = bytes.get_mut(at..end) {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

pub(crate) struct TempArchive {
    path: PathBuf,
}

impl TempArchive {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("mint-sage-big-{}-{unique}.big", std::process::id()));
        std::fs::write(&path, bytes).expect("write temp archive");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempArchive {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
