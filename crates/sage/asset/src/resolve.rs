use mint_vfs::{Vfs, VfsPath};

use crate::{Error, Result};

pub const DEFAULT_TEXTURE_EXTENSIONS: [&str; 4] = ["dds", "tga", "jpg", "png"];

#[must_use]
pub fn alpha_sidecar(path: &VfsPath) -> Option<VfsPath> {
    let (stem, extension) = path.as_str().rsplit_once('.')?;
    (extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg"))
        .then(|| VfsPath::new(&format!("{stem}.png")))
}

const W3D_DIR: &str = "art/w3d/";
const MOLDS_DIR: &str = "data/editor/molds/";
const SHADERS_DIR: &str = "shaders/compiled/";
const TEXTURES_DIR: &str = "art/textures/";
const COMPILED_TEXTURES_DIR: &str = "art/compiledtextures/";

const APT_PREFIX: &str = "apt_";

const PREFIX_LEN: usize = 2;

#[derive(Debug, Clone)]
pub struct Resolver {
    texture_extensions: Vec<String>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver {
    #[must_use]
    pub fn new() -> Self {
        Self::with_texture_extensions(&DEFAULT_TEXTURE_EXTENSIONS)
    }

    #[must_use]
    pub fn with_texture_extensions(extensions: &[&str]) -> Self {
        Self {
            texture_extensions: extensions
                .iter()
                .map(|extension| extension.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
        }
    }

    pub fn texture_extensions(&self) -> impl Iterator<Item = &str> {
        self.texture_extensions.iter().map(String::as_str)
    }

    pub fn candidates(&self, name: &str) -> Result<Vec<VfsPath>> {
        let bad = |reason| Error::BadLogicalName {
            name: name.to_owned(),
            reason,
        };

        if name.contains('/') || name.contains('\\') {
            return Err(bad("a logical name is a bare filename, with no directory"));
        }
        let lowered = name.to_ascii_lowercase();
        let (stem, extension) = lowered
            .rsplit_once('.')
            .ok_or_else(|| bad("a logical name carries an extension"))?;
        if stem.is_empty() {
            return Err(bad("a logical name has a stem before its extension"));
        }

        let prefix: String = stem.chars().take(PREFIX_LEN).collect();

        Ok(match extension {
            "w3d" => vec![
                path(&[W3D_DIR, &prefix, "/", stem, ".w3d"]),
                path(&[MOLDS_DIR, stem, ".w3d"]),
            ],
            "fx" => vec![path(&[SHADERS_DIR, stem, ".fxo"])],
            _ if stem.starts_with(APT_PREFIX) => self.texture_candidates(TEXTURES_DIR, "", stem),
            _ => self.texture_candidates(COMPILED_TEXTURES_DIR, &prefix, stem),
        })
    }

    pub fn resolve(&self, vfs: &Vfs, name: &str) -> Result<VfsPath> {
        let candidates = self.candidates(name)?;
        for candidate in &candidates {
            if vfs.contains(candidate.as_str()) {
                return Ok(candidate.clone());
            }
        }
        Err(Error::Unresolved {
            name: name.to_owned(),
            tried: candidates,
        })
    }

    pub fn present_candidates(&self, vfs: &Vfs, name: &str) -> Result<Vec<VfsPath>> {
        Ok(self
            .candidates(name)?
            .into_iter()
            .filter(|candidate| vfs.contains(candidate.as_str()))
            .collect())
    }

    fn texture_candidates(&self, directory: &str, prefix: &str, stem: &str) -> Vec<VfsPath> {
        self.texture_extensions
            .iter()
            .map(|extension| {
                if prefix.is_empty() {
                    path(&[directory, stem, ".", extension])
                } else {
                    path(&[directory, prefix, "/", stem, ".", extension])
                }
            })
            .collect()
    }
}

fn path(parts: &[&str]) -> VfsPath {
    VfsPath::new(&parts.concat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::MemMount;

    fn vfs_with(paths: &[&str]) -> Vfs {
        let mut vfs = Vfs::new();
        vfs.mount(MemMount::new("test", paths));
        vfs
    }

    fn candidates(name: &str) -> Vec<String> {
        Resolver::new()
            .candidates(name)
            .unwrap()
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect()
    }

    #[test]
    fn a_texture_goes_under_the_first_two_characters_of_its_stem() {
        assert_eq!(
            candidates("foo.tga"),
            [
                "art/compiledtextures/fo/foo.dds",
                "art/compiledtextures/fo/foo.tga",
                "art/compiledtextures/fo/foo.jpg",
                "art/compiledtextures/fo/foo.png",
            ]
        );
    }

    #[test]
    fn a_model_is_tried_under_art_w3d_then_the_editor_molds() {
        assert_eq!(
            candidates("aicamp_skn.w3d"),
            [
                "art/w3d/ai/aicamp_skn.w3d",
                "data/editor/molds/aicamp_skn.w3d"
            ]
        );
    }

    #[test]
    fn a_shader_becomes_a_compiled_shader() {
        assert_eq!(candidates("shroud.fx"), ["shaders/compiled/shroud.fxo"]);
    }

    #[test]
    fn an_apt_texture_is_flat_with_no_two_character_directory() {
        assert_eq!(
            candidates("apt_button.tga"),
            [
                "art/textures/apt_button.dds",
                "art/textures/apt_button.tga",
                "art/textures/apt_button.jpg",
                "art/textures/apt_button.png",
            ]
        );
    }

    #[test]
    fn the_logical_extension_of_a_texture_does_not_survive_into_the_path() {
        assert_eq!(candidates("shared.png"), candidates("shared.tga"));
    }

    #[test]
    fn resolution_is_case_insensitive_in_the_name_it_is_given() {
        let vfs = vfs_with(&["art/compiledtextures/fo/foo.dds"]);
        let resolver = Resolver::new();

        let expected = VfsPath::new("art/compiledtextures/fo/foo.dds");
        assert_eq!(resolver.resolve(&vfs, "foo.tga").unwrap(), expected);
        assert_eq!(resolver.resolve(&vfs, "FOO.TGA").unwrap(), expected);
        assert_eq!(resolver.resolve(&vfs, "FoO.TgA").unwrap(), expected);
    }

    #[test]
    fn the_first_extension_in_the_order_wins_when_several_exist() {
        let vfs = vfs_with(&[
            "art/compiledtextures/fo/foo.jpg",
            "art/compiledtextures/fo/foo.png",
        ]);

        assert_eq!(
            Resolver::new().resolve(&vfs, "foo.tga").unwrap().as_str(),
            "art/compiledtextures/fo/foo.jpg"
        );
    }

    #[test]
    fn reversing_the_order_changes_which_of_two_candidates_wins() {
        let vfs = vfs_with(&[
            "art/compiledtextures/fo/foo.jpg",
            "art/compiledtextures/fo/foo.png",
        ]);
        let resolver = Resolver::with_texture_extensions(&["png", "jpg"]);

        assert_eq!(
            resolver.resolve(&vfs, "foo.tga").unwrap().as_str(),
            "art/compiledtextures/fo/foo.png"
        );
    }

    #[test]
    fn an_ambiguous_name_reports_every_candidate_that_exists() {
        let vfs = vfs_with(&[
            "art/compiledtextures/fo/foo.jpg",
            "art/compiledtextures/fo/foo.png",
        ]);

        assert_eq!(
            Resolver::new()
                .present_candidates(&vfs, "foo.tga")
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn a_model_missing_from_art_w3d_falls_back_to_the_editor_molds() {
        let vfs = vfs_with(&["data/editor/molds/box.w3d"]);

        assert_eq!(
            Resolver::new().resolve(&vfs, "box.w3d").unwrap().as_str(),
            "data/editor/molds/box.w3d"
        );
    }

    #[test]
    fn a_leading_dot_on_a_configured_extension_is_accepted() {
        let resolver = Resolver::with_texture_extensions(&[".DDS"]);

        assert_eq!(resolver.texture_extensions().collect::<Vec<_>>(), ["dds"]);
    }

    #[test]
    fn a_name_nothing_supplies_lists_what_was_tried() {
        let vfs = vfs_with(&[]);

        let error = Resolver::new().resolve(&vfs, "foo.tga").unwrap_err();
        let Error::Unresolved { tried, .. } = &error else {
            panic!("expected Unresolved, got {error:?}");
        };
        assert_eq!(tried.len(), 4);
        assert!(error
            .to_string()
            .contains("art/compiledtextures/fo/foo.dds"));
    }

    #[test]
    fn a_name_with_no_extension_is_refused() {
        assert!(matches!(
            Resolver::new().candidates("foo"),
            Err(Error::BadLogicalName { .. })
        ));
    }

    #[test]
    fn a_name_with_a_directory_component_is_refused() {
        assert!(matches!(
            Resolver::new().candidates("art/w3d/ai/foo.w3d"),
            Err(Error::BadLogicalName { .. })
        ));
        assert!(matches!(
            Resolver::new().candidates(r"art\w3d\foo.w3d"),
            Err(Error::BadLogicalName { .. })
        ));
    }

    #[test]
    fn a_name_that_is_only_an_extension_is_refused() {
        assert!(matches!(
            Resolver::new().candidates(".tga"),
            Err(Error::BadLogicalName { .. })
        ));
        assert!(matches!(
            Resolver::new().candidates(""),
            Err(Error::BadLogicalName { .. })
        ));
    }

    #[test]
    fn a_one_character_stem_gets_a_one_character_directory() {
        assert_eq!(
            candidates("a.tga").first().map(String::as_str),
            Some("art/compiledtextures/a/a.dds")
        );
    }
}
