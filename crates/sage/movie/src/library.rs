use std::path::{Path, PathBuf};

use mint_sage_ini::table::BlockTable;
use mint_sage_ini::{Document, VideoIndex};
use mint_vfs::Vfs;

#[derive(Debug, Clone, Default)]
pub struct MovieLibrary {
    videos: VideoIndex,
}

impl MovieLibrary {
    #[must_use]
    pub fn load(vfs: &Vfs) -> Self {
        let mut paths: Vec<String> = vfs
            .paths_under("data/ini")
            .map(|path| path.as_str().to_owned())
            .filter(|path| path.ends_with("/video.ini"))
            .collect();
        paths.sort_by_key(|path| (!path.contains("/default/"), path.clone()));
        let table = BlockTable::corpus_path();
        let mut videos = VideoIndex::new();
        for path in &paths {
            let Ok(bytes) = vfs.read(path) else { continue };
            if let Ok(document) = Document::parse(path, &String::from_utf8_lossy(&bytes), &table) {
                videos.insert_document(&document);
            }
        }
        Self { videos }
    }

    #[must_use]
    pub fn filename(&self, name: &str) -> Option<&str> {
        let def = self.videos.get(name)?;
        if def.is_default == Some(true) {
            return None;
        }
        def.filename.as_deref()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.videos.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.videos.is_empty()
    }

    #[must_use]
    pub fn errors(&self) -> &[mint_sage_ini::diagnostic::ValueError] {
        self.videos.errors()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MovieRoots {
    dirs: Vec<PathBuf>,
}

impl MovieRoots {
    #[must_use]
    pub fn new(installs: &[PathBuf], language: &str, mod_root: Option<&Path>) -> Self {
        let mut dirs = Vec::new();
        if let Some(root) = mod_root {
            dirs.push(root.join("Data").join("Movies"));
        }
        for install in installs {
            dirs.push(
                install
                    .join("Lang")
                    .join(language)
                    .join("Data")
                    .join("Movies"),
            );
        }
        for install in installs {
            dirs.push(install.join("Data").join("Movies"));
        }
        Self { dirs }
    }

    #[must_use]
    pub fn find(&self, stem: &str) -> Option<PathBuf> {
        let wanted = format!("{stem}.vp6");
        self.dirs.iter().find_map(|dir| {
            let exact = dir.join(&wanted);
            if exact.is_file() {
                return Some(exact);
            }
            std::fs::read_dir(dir)
                .ok()?
                .filter_map(std::result::Result::ok)
                .find(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&wanted)
                })
                .map(|entry| entry.path())
        })
    }
}
