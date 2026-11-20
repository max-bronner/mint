use mint_sage_ini::{
    BlockTable, Diagnostic, Document, FxListIndex, MemorySources, ParticleIndex, Reader,
    DEFINE_DOCUMENTS, PARTICLE_DOCUMENTS,
};
use mint_vfs::Vfs;

#[derive(Debug, Clone, Default)]
pub struct ParticleLoadReport {
    pub documents: usize,
    pub parsed: usize,
    pub statements: usize,
    pub stray_ends: usize,
    pub unclosed_blocks: usize,
    pub failures: Vec<String>,
}

#[must_use]
pub fn load_particle_index(vfs: &Vfs) -> (ParticleIndex, FxListIndex, ParticleLoadReport) {
    load_particle_index_over(vfs, None)
}

#[must_use]
pub fn load_particle_index_over(
    vfs: &Vfs,
    map_ini: Option<&Document>,
) -> (ParticleIndex, FxListIndex, ParticleLoadReport) {
    let mut sources = MemorySources::new();
    let mut present = 0usize;
    for path in PARTICLE_DOCUMENTS {
        if let Ok(bytes) = vfs.read(path) {
            present = present.saturating_add(1);
            sources.insert(path, bytes);
        }
    }
    for path in DEFINE_DOCUMENTS {
        if let Ok(bytes) = vfs.read(path) {
            sources.insert(path, bytes);
        }
    }

    let table = BlockTable::particle_path();
    let reader = Reader::new(&sources);
    let mut documents = Vec::new();
    let mut failures = Vec::new();
    for path in PARTICLE_DOCUMENTS.into_iter().chain(DEFINE_DOCUMENTS) {
        if sources.paths().all(|held| held != path) {
            continue;
        }
        match reader.read(path, &table) {
            Ok(document) => documents.push(document),
            Err(error) => failures.push(format!("{path}: {error}")),
        }
    }
    let (mut systems, lists, mut report) = particle_index_from_documents(&documents);
    if let Some(map_ini) = map_ini {
        systems.insert_override_document(&documents, map_ini);
    }
    report.documents = present;
    report.failures = failures;
    (systems, lists, report)
}

#[must_use]
pub fn particle_index_from_documents(
    documents: &[Document],
) -> (ParticleIndex, FxListIndex, ParticleLoadReport) {
    let mut report = ParticleLoadReport::default();
    let mut systems = ParticleIndex::new();
    let mut lists = FxListIndex::new();
    for document in documents {
        lists.insert_defines(document.defines());
    }
    for path in PARTICLE_DOCUMENTS {
        let Some(document) = documents.iter().find(|document| document.name() == path) else {
            continue;
        };
        report.documents = report.documents.saturating_add(1);
        report.parsed = report.parsed.saturating_add(1);
        report.statements = report.statements.saturating_add(document.statements());
        for diagnostic in document.diagnostics() {
            match diagnostic {
                Diagnostic::StrayEnd { .. } => {
                    report.stray_ends = report.stray_ends.saturating_add(1);
                }
                Diagnostic::UnclosedBlock { .. } => {
                    report.unclosed_blocks = report.unclosed_blocks.saturating_add(1);
                }
                _ => {}
            }
        }
        systems.insert_document(document);
        lists.insert_document(document);
    }
    (systems, lists, report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use mint_vfs::VfsPath;

    struct BytesMount(Vec<(VfsPath, Vec<u8>)>);

    impl mint_vfs::Mount for BytesMount {
        fn label(&self) -> &str {
            "bytes"
        }
        fn entries(&self) -> Vec<VfsPath> {
            self.0.iter().map(|(path, _)| path.clone()).collect()
        }
        fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
            self.0
                .iter()
                .find(|(candidate, _)| candidate == path)
                .map(|(_, bytes)| bytes.clone())
                .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })
        }
    }

    fn vfs_with(entries: &[(&str, &str)]) -> Vfs {
        let mut vfs = Vfs::new();
        vfs.mount(Arc::new(BytesMount(
            entries
                .iter()
                .map(|(path, text)| (VfsPath::new(path), text.as_bytes().to_vec()))
                .collect(),
        )));
        vfs
    }

    #[test]
    fn a_partial_mount_loads_what_is_there() {
        let vfs = vfs_with(&[(
            "data/ini/fxparticlesystem.ini",
            "FXParticleSystem Puff\n  System\n    Size = 4 4\n  End\nEnd\n",
        )]);
        let (systems, lists, report) = load_particle_index(&vfs);
        assert_eq!(report.documents, 1);
        assert_eq!(report.parsed, 1);
        assert_eq!(report.stray_ends, 0);
        assert_eq!(report.unclosed_blocks, 0);
        assert!(report.failures.is_empty());
        assert_eq!(systems.len(), 1);
        assert!(lists.is_empty());
    }

    #[test]
    fn the_vfs_load_equals_the_indexes_built_from_parsed_documents() {
        let files = [
            (
                "data/ini/fxparticlesystem.ini",
                "FXParticleSystem Puff\n  System\n    Lifetime = 10 20\n  End\nEnd\n",
            ),
            (
                "data/ini/fxlist.ini",
                "FXList FX_Puff\n  ParticleSystem\n    Name = Puff\n  End\nEnd\n",
            ),
            (
                "data/ini/other.ini",
                "FXList FX_Elsewhere\n  ParticleSystem\n    Name = Puff\n  End\nEnd\n",
            ),
        ];
        let (systems, lists, _) = load_particle_index(&vfs_with(&files));

        let mut sources = MemorySources::new();
        for (path, text) in files {
            sources.insert(path, text.as_bytes().to_vec());
        }
        let reader = Reader::new(&sources);
        let table = BlockTable::corpus_path();
        let mut documents: Vec<Document> = files
            .iter()
            .map(|(path, _)| reader.read(path, &table).unwrap())
            .collect();
        mint_sage_ini::expand_macros(&mut documents);
        let (shared_systems, shared_lists, _) = particle_index_from_documents(&documents);

        assert_eq!(lists.len(), 1);
        assert_eq!(
            format!("{systems:?}{lists:?}"),
            format!("{shared_systems:?}{shared_lists:?}")
        );
    }

    #[test]
    fn an_unbalanced_document_still_loads_and_the_count_says_so() {
        let vfs = vfs_with(&[(
            "data/ini/fxparticlesystem.ini",
            "FXParticleSystem Puff\n  System\n  End\nEnd\nEnd\n",
        )]);
        let (systems, _, report) = load_particle_index(&vfs);
        assert_eq!(systems.len(), 1, "the definition before it survives");
        assert_eq!(report.stray_ends, 1);
    }
}
