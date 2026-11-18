use std::collections::BTreeMap;

use crate::movie::{AptMovie, Character, Import};

#[derive(Debug, Default)]
pub struct MovieSet(BTreeMap<String, AptMovie>);

impl MovieSet {
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, movie: AptMovie) {
        self.0.insert(movie.name.clone(), movie);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AptMovie> {
        self.0.get(&normalize_movie_name(name))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn movies(&self) -> impl Iterator<Item = &AptMovie> {
        self.0.values()
    }
}

fn normalize_movie_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".apt").unwrap_or(&lower).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("import names movie {movie:?}, not present in this set")]
    MovieNotFound { movie: String },
    #[error("{movie}: no export named {name:?}")]
    NameNotFound { movie: String, name: String },
    #[error("{movie}: export {name:?} names character {character}, but {movie} has only {count} characters")]
    CharacterOutOfRange {
        movie: String,
        name: String,
        character: u32,
        count: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedImport<'a> {
    pub movie: &'a AptMovie,
    pub character_id: u32,
    pub character: &'a Character,
}

pub fn resolve_import<'a>(
    movies: &'a MovieSet,
    imp: &Import,
) -> std::result::Result<ResolvedImport<'a>, ImportError> {
    let target = movies
        .get(&imp.movie)
        .ok_or_else(|| ImportError::MovieNotFound {
            movie: imp.movie.clone(),
        })?;
    let export = target
        .exports
        .iter()
        .find(|export| export.name.eq_ignore_ascii_case(&imp.name))
        .ok_or_else(|| ImportError::NameNotFound {
            movie: target.name.clone(),
            name: imp.name.clone(),
        })?;
    let character = target
        .characters
        .get(export.character as usize)
        .ok_or_else(|| ImportError::CharacterOutOfRange {
            movie: target.name.clone(),
            name: imp.name.clone(),
            character: export.character,
            count: u32::try_from(target.characters.len()).unwrap_or(u32::MAX),
        })?;
    Ok(ResolvedImport {
        movie: target,
        character_id: export.character,
        character,
    })
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;
    use crate::movie::{Export, Movie};

    fn empty_record() -> Movie {
        Movie {
            frame_count: 0,
            frames_offset: 0,
            character_count: 0,
            characters_offset: 0,
            screen_width: 0,
            screen_height: 0,
            unknown_36: 0,
            import_count: 0,
            imports_offset: 0,
            export_count: 0,
            exports_offset: 0,
        }
    }

    fn movie_with_export(
        name: &str,
        export_name: &str,
        character: u32,
        characters: Vec<Character>,
    ) -> AptMovie {
        AptMovie {
            name: name.to_string(),
            record: empty_record(),
            characters,
            imports: Vec::new(),
            exports: vec![Export {
                name: export_name.to_string(),
                character,
            }],
        }
    }

    #[test]
    fn resolves_across_movies() {
        let mut set = MovieSet::new();
        set.insert(movie_with_export(
            "menuexport",
            "ButtonMed",
            3,
            vec![
                Character::Placeholder,
                Character::Placeholder,
                Character::Placeholder,
                Character::Image { texture: 3 },
            ],
        ));
        let imp = Import {
            movie: "MenuExport".to_string(),
            name: "buttonmed".to_string(),
            local_character: 0,
        };
        let resolved = resolve_import(&set, &imp).unwrap();
        assert_eq!(resolved.movie.name, "menuexport");
        assert_eq!(resolved.character_id, 3);
        assert!(matches!(
            resolved.character,
            Character::Image { texture: 3 }
        ));
    }

    #[test]
    fn missing_movie_is_reported_not_panicked() {
        let set = MovieSet::new();
        let imp = Import {
            movie: "nowhere".to_string(),
            name: "x".to_string(),
            local_character: 0,
        };
        assert_eq!(
            resolve_import(&set, &imp),
            Err(ImportError::MovieNotFound {
                movie: "nowhere".to_string()
            })
        );
    }

    #[test]
    fn wrong_movie_resolves_to_nothing_the_negative_control() {
        let mut set = MovieSet::new();
        set.insert(movie_with_export(
            "right",
            "thing",
            0,
            vec![Character::Placeholder],
        ));
        set.insert(movie_with_export(
            "wrong",
            "other",
            0,
            vec![Character::Placeholder],
        ));
        let imp = Import {
            movie: "wrong".to_string(),
            name: "thing".to_string(),
            local_character: 0,
        };
        assert!(matches!(
            resolve_import(&set, &imp),
            Err(ImportError::NameNotFound { .. })
        ));
    }

    #[test]
    fn missing_export_name_is_reported() {
        let mut set = MovieSet::new();
        set.insert(movie_with_export(
            "menuexport",
            "Something",
            0,
            vec![Character::Placeholder],
        ));
        let imp = Import {
            movie: "menuexport".to_string(),
            name: "nope".to_string(),
            local_character: 0,
        };
        assert!(matches!(
            resolve_import(&set, &imp),
            Err(ImportError::NameNotFound { .. })
        ));
    }

    #[test]
    fn out_of_range_character_is_reported_not_panicked() {
        let mut set = MovieSet::new();
        set.insert(movie_with_export(
            "menuexport",
            "thing",
            99,
            vec![Character::Placeholder],
        ));
        let imp = Import {
            movie: "menuexport".to_string(),
            name: "thing".to_string(),
            local_character: 0,
        };
        assert!(matches!(
            resolve_import(&set, &imp),
            Err(ImportError::CharacterOutOfRange {
                character: 99,
                count: 1,
                ..
            })
        ));
    }

    #[test]
    fn movie_name_normalisation_strips_apt_suffix_and_case() {
        let mut set = MovieSet::new();
        set.insert(movie_with_export(
            "menuexport",
            "thing",
            0,
            vec![Character::Placeholder],
        ));
        let imp = Import {
            movie: "MenuExport.apt".to_string(),
            name: "THING".to_string(),
            local_character: 0,
        };
        assert!(resolve_import(&set, &imp).is_ok());
    }

    #[test]
    fn name_matching_takes_the_first_export_when_a_name_is_ambiguous() {
        let mut set = MovieSet::new();
        let mut movie = movie_with_export(
            "menuexport",
            "dup",
            0,
            vec![
                Character::Image { texture: 0 },
                Character::Image { texture: 1 },
            ],
        );
        movie.exports.push(Export {
            name: "dup".to_string(),
            character: 1,
        });
        set.insert(movie);
        let imp = Import {
            movie: "menuexport".to_string(),
            name: "dup".to_string(),
            local_character: 0,
        };
        let resolved = resolve_import(&set, &imp).unwrap();
        assert_eq!(resolved.character_id, 0, "first match, not last");
    }
}
