use mint_vfs::{Vfs, VfsPath};

use crate::{AssetEntry, AssetRecord, Catalogue, Resolver, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset<'a> {
    pub path: VfsPath,
    pub record: Option<&'a AssetRecord>,
}

impl ResolvedAsset<'_> {
    #[must_use]
    pub fn entries(&self) -> &[AssetEntry] {
        self.record.map_or(&[], |record| record.entries.as_slice())
    }

    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&AssetEntry> {
        self.record.and_then(|record| record.entry(name))
    }
}

#[derive(Debug, Clone, Default)]
pub struct AssetLibrary {
    catalogues: Vec<Catalogue>,
    resolver: Resolver,
}

impl AssetLibrary {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_resolver(resolver: Resolver) -> Self {
        Self {
            catalogues: Vec::new(),
            resolver,
        }
    }

    pub fn add_catalogue(&mut self, catalogue: Catalogue) {
        self.catalogues.push(catalogue);
    }

    #[must_use]
    pub fn catalogues(&self) -> &[Catalogue] {
        &self.catalogues
    }

    #[must_use]
    pub fn resolver(&self) -> &Resolver {
        &self.resolver
    }

    #[must_use]
    pub fn record(&self, name: &str) -> Option<&AssetRecord> {
        self.catalogues
            .iter()
            .rev()
            .find_map(|catalogue| catalogue.record(name))
    }

    pub fn resolve(&self, vfs: &Vfs, name: &str) -> Result<ResolvedAsset<'_>> {
        let path = self.resolver.resolve(vfs, name)?;
        Ok(ResolvedAsset {
            path,
            record: self.record(name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{CatalogueBuilder, MemMount};
    use crate::{AssetKind, Error};

    fn vfs_with(paths: &[&str]) -> Vfs {
        let mut vfs = Vfs::new();
        vfs.mount(MemMount::new("test", paths));
        vfs
    }

    fn catalogue(name: &str, timestamp: u64) -> Catalogue {
        Catalogue::parse(
            &CatalogueBuilder::new()
                .record(name, timestamp, &[("MESH", AssetKind::Mesh, 0, 40)])
                .build(),
            "<synthetic>",
        )
        .unwrap()
    }

    #[test]
    fn a_resolved_model_carries_the_byte_ranges_within_it() {
        let mut library = AssetLibrary::new();
        library.add_catalogue(catalogue("b.w3d", 1));
        let vfs = vfs_with(&["art/w3d/b/b.w3d"]);

        let asset = library.resolve(&vfs, "b.w3d").unwrap();
        assert_eq!(asset.path.as_str(), "art/w3d/b/b.w3d");
        assert_eq!(asset.entry("mesh").unwrap().byte_range(), Some(0..40));
    }

    #[test]
    fn an_uncatalogued_name_still_resolves_to_a_path() {
        let library = AssetLibrary::new();
        let vfs = vfs_with(&["art/w3d/zz/zz.w3d"]);

        let asset = library.resolve(&vfs, "zz.w3d").unwrap();
        assert!(asset.record.is_none());
        assert!(asset.entries().is_empty());
    }

    #[test]
    fn a_catalogued_name_with_no_mounted_file_is_unresolved() {
        let mut library = AssetLibrary::new();
        library.add_catalogue(catalogue("b.w3d", 1));
        let vfs = vfs_with(&[]);

        assert!(matches!(
            library.resolve(&vfs, "b.w3d"),
            Err(Error::Unresolved { .. })
        ));
    }

    #[test]
    fn the_last_catalogue_added_wins_a_shared_name() {
        let mut library = AssetLibrary::new();
        library.add_catalogue(catalogue("b.w3d", 1));
        library.add_catalogue(catalogue("b.w3d", 2));

        assert_eq!(library.record("b.w3d").unwrap().timestamp, 2);
    }

    #[test]
    fn a_name_only_the_lower_catalogue_has_is_still_found() {
        let mut library = AssetLibrary::new();
        library.add_catalogue(catalogue("base.w3d", 1));
        library.add_catalogue(catalogue("patch.w3d", 2));

        assert_eq!(library.record("base.w3d").unwrap().timestamp, 1);
    }
}
