use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct TerrainTypes {
    textures: BTreeMap<String, String>,
    redefinitions: usize,
}

impl TerrainTypes {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut textures = BTreeMap::new();
        let mut redefinitions = 0usize;
        let mut current: Option<String> = None;

        for line in text.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = keyword_value(line, "Terrain") {
                current = rest.split_whitespace().next().map(str::to_ascii_lowercase);
                continue;
            }
            if line.eq_ignore_ascii_case("End") {
                current = None;
                continue;
            }
            let (Some(name), Some(rest)) = (current.as_ref(), assignment(line, "Texture")) else {
                continue;
            };
            let Some(file) = rest.split_whitespace().next() else {
                continue;
            };
            if textures.insert(name.clone(), file.to_owned()).is_some() {
                redefinitions = redefinitions.saturating_add(1);
            }
        }

        Self {
            textures,
            redefinitions,
        }
    }

    #[must_use]
    pub fn texture(&self, name: &str) -> Option<&str> {
        self.textures
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    #[must_use]
    pub fn redefinitions(&self) -> usize {
        self.redefinitions
    }
}

fn strip_comment(line: &str) -> &str {
    line.split(';').next().unwrap_or("")
}

fn keyword_value<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line
        .get(..keyword.len())
        .filter(|head| head.eq_ignore_ascii_case(keyword))?;
    let _ = rest;
    let tail = line.get(keyword.len()..)?;
    if tail.starts_with(char::is_whitespace) {
        Some(tail.trim_start())
    } else {
        None
    }
}

fn assignment<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let (key, value) = line.split_once('=')?;
    if key.trim().eq_ignore_ascii_case(keyword) {
        Some(value.trim())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_block_yields_its_texture_and_the_lookup_ignores_case() {
        let types = TerrainTypes::parse(
            "Terrain GrassRhun01\n  Texture = TGras_Rhu01.tga\n  Class = Type Grass\nEnd\n",
        );
        assert_eq!(types.len(), 1);
        assert_eq!(types.texture("grassrhun01"), Some("TGras_Rhu01.tga"));
        assert_eq!(types.texture("GRASSRHUN01"), Some("TGras_Rhu01.tga"));
        assert_eq!(types.texture("nothing"), None);
    }

    #[test]
    fn trailing_junk_after_the_filename_is_dropped_rather_than_kept() {
        let types =
            TerrainTypes::parse("Terrain SandType\n  Texture = TXSand01a.tga NEXT Region X\nEnd\n");
        assert_eq!(types.texture("sandtype"), Some("TXSand01a.tga"));
    }

    #[test]
    fn a_texture_outside_a_block_belongs_to_nothing() {
        let types = TerrainTypes::parse(
            "Terrain A\n  Texture = a.tga\nEnd\n  Texture = stray.tga\nTerrain B\nEnd\n",
        );
        assert_eq!(types.texture("a"), Some("a.tga"));
        assert_eq!(types.texture("b"), None);
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn comments_and_unknown_keywords_are_skipped_and_redefinitions_counted() {
        let types = TerrainTypes::parse(
            "; a comment\n\
             Terrain A\n  Texture = a.tga\n  TerrainObject = Grass01 50\n; Texture = no.tga\nEnd\n\
             Terrain A\n  Texture = second.tga\nEnd\n",
        );
        assert_eq!(types.texture("a"), Some("second.tga"));
        assert_eq!(types.redefinitions(), 1);
    }

    #[test]
    fn arbitrary_input_never_panics() {
        for text in [
            "",
            "Terrain",
            "Terrain\n",
            "Texture =",
            "Terrain \nTexture = \nEnd",
            "=====",
            ";;;;",
            "Terrain \u{e9}\n  Texture = \u{e9}.tga\nEnd",
        ] {
            let _ = TerrainTypes::parse(text);
        }
    }
}
