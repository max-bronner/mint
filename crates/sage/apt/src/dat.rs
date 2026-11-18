use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureLocation {
    Shared {
        page: u32,
    },
    Standalone {
        left: u32,
        top: u32,
        right: u32,
        bottom: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatEntry {
    pub id: u32,
    pub location: TextureLocation,
}

fn syntax(line: usize, reason: impl Into<String>) -> Error {
    Error::DatSyntax {
        line,
        reason: reason.into(),
    }
}

fn parse_u32(line: usize, field: &str) -> Result<u32> {
    field
        .trim()
        .parse()
        .map_err(|_| syntax(line, format!("{field:?} is not an integer")))
}

pub fn parse_dat(text: &str) -> Result<Vec<DatEntry>> {
    let mut entries = Vec::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line_no = index.saturating_add(1);
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some((id, page)) = line.split_once("->") {
            entries.push(DatEntry {
                id: parse_u32(line_no, id)?,
                location: TextureLocation::Shared {
                    page: parse_u32(line_no, page)?,
                },
            });
            continue;
        }
        if let Some((id, rect)) = line.split_once('=') {
            let fields: Vec<&str> = rect.split_whitespace().collect();
            let [left, top, right, bottom] = fields.as_slice() else {
                return Err(syntax(
                    line_no,
                    format!("expected 4 rect fields, got {}", fields.len()),
                ));
            };
            entries.push(DatEntry {
                id: parse_u32(line_no, id)?,
                location: TextureLocation::Standalone {
                    left: parse_u32(line_no, left)?,
                    top: parse_u32(line_no, top)?,
                    right: parse_u32(line_no, right)?,
                    bottom: parse_u32(line_no, bottom)?,
                },
            });
            continue;
        }
        return Err(syntax(
            line_no,
            format!("neither 'id->page' nor 'id=l t r b': {line:?}"),
        ));
    }
    Ok(entries)
}

#[must_use]
pub fn standalone_texture_path(movie_name: &str, id: u32) -> String {
    format!("art/textures/apt_{movie_name}_{id}.tga")
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

    #[test]
    fn parses_the_shared_atlas_form() {
        let text = "; Created by AptToBigc.\n4->1\n; Created by AptToBigc.\n";
        let entries = parse_dat(text).unwrap();
        assert_eq!(
            entries,
            vec![DatEntry {
                id: 4,
                location: TextureLocation::Shared { page: 1 }
            }]
        );
    }

    #[test]
    fn parses_the_standalone_rect_form() {
        let text = "; Created by AptToBigc.\n3=0 0 112 5\n8=0 0 31 29\n";
        let entries = parse_dat(text).unwrap();
        assert_eq!(
            entries,
            vec![
                DatEntry {
                    id: 3,
                    location: TextureLocation::Standalone {
                        left: 0,
                        top: 0,
                        right: 112,
                        bottom: 5
                    }
                },
                DatEntry {
                    id: 8,
                    location: TextureLocation::Standalone {
                        left: 0,
                        top: 0,
                        right: 31,
                        bottom: 29
                    }
                },
            ]
        );
    }

    #[test]
    fn both_forms_in_one_file() {
        let text = "; Created by AptToBigc.\n3=0 0 112 5\n; Created by AptToBigc.\n4->1\n";
        let entries = parse_dat(text).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn rejects_a_malformed_line() {
        assert!(matches!(
            parse_dat("nonsense\n"),
            Err(Error::DatSyntax { line: 1, .. })
        ));
    }

    #[test]
    fn rejects_a_rect_with_the_wrong_field_count() {
        assert!(matches!(
            parse_dat("3=0 0 112\n"),
            Err(Error::DatSyntax { line: 1, .. })
        ));
    }

    #[test]
    fn standalone_texture_path_matches_the_measured_convention() {
        assert_eq!(
            standalone_texture_path("options", 3),
            "art/textures/apt_options_3.tga"
        );
    }
}
