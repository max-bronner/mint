use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Style {
    Solid {
        color: [f32; 4],
    },
    Line {
        width: f32,
        color: [f32; 4],
    },
    Texture {
        color: [f32; 4],
        character: u32,
        pixel_transform: [[f32; 2]; 2],
        pixel_translate: [f32; 2],
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub style: Style,
    pub triangles: Vec<[[f32; 2]; 3]>,
    pub lines: Vec<[[f32; 2]; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuFile {
    pub entries: Vec<Entry>,
}

fn syntax(line: usize, reason: impl Into<String>) -> Error {
    Error::RuSyntax {
        line,
        reason: reason.into(),
    }
}

fn parse_f32(line: usize, field: &str) -> Result<f32> {
    field
        .parse()
        .map_err(|_| syntax(line, format!("{field:?} is not a number")))
}

fn parse_u32(line: usize, field: &str) -> Result<u32> {
    field
        .parse()
        .map_err(|_| syntax(line, format!("{field:?} is not an integer")))
}

fn exact_floats(line: usize, fields: &[&str], count: usize) -> Result<Vec<f32>> {
    if fields.len() != count {
        return Err(syntax(
            line,
            format!("expected {count} fields, got {}", fields.len()),
        ));
    }
    fields.iter().map(|f| parse_f32(line, f)).collect()
}

fn at(values: &[f32], index: usize, line: usize) -> Result<f32> {
    values
        .get(index)
        .copied()
        .ok_or_else(|| syntax(line, "missing field"))
}

fn parse_style(line: usize, rest: &str) -> Result<Style> {
    let mut parts = rest.split(':');
    let kind = parts.next().unwrap_or_default();
    let fields: Vec<&str> = parts.collect();
    match kind {
        "s" => {
            let f = exact_floats(line, &fields, 4)?;
            Ok(Style::Solid {
                color: color_of(&f),
            })
        }
        "l" => {
            let f = exact_floats(line, &fields, 5)?;
            let width = at(&f, 0, line)?;
            let color = color_of(f.get(1..).unwrap_or(&[]));
            Ok(Style::Line { width, color })
        }
        "tc" => {
            if fields.len() != 11 {
                return Err(syntax(
                    line,
                    format!("texture style expected 11 fields, got {}", fields.len()),
                ));
            }
            let color_fields = fields
                .get(..4)
                .ok_or_else(|| syntax(line, "texture style missing colour fields"))?;
            let color = color_of(&exact_floats(line, color_fields, 4)?);
            let character_field = fields
                .get(4)
                .ok_or_else(|| syntax(line, "texture style missing character field"))?;
            let character = parse_u32(line, character_field)?;
            let matrix_fields = fields
                .get(5..11)
                .ok_or_else(|| syntax(line, "texture style missing matrix fields"))?;
            let m = exact_floats(line, matrix_fields, 6)?;
            let (m00, m01) = (at(&m, 0, line)?, at(&m, 1, line)?);
            let (m10, m11) = (at(&m, 2, line)?, at(&m, 3, line)?);
            let (tx, ty) = (at(&m, 4, line)?, at(&m, 5, line)?);
            Ok(Style::Texture {
                color,
                character,
                pixel_transform: [[m00, m10], [m01, m11]],
                pixel_translate: [tx, ty],
            })
        }
        other => Err(syntax(line, format!("unknown style kind {other:?}"))),
    }
}

fn color_of(values: &[f32]) -> [f32; 4] {
    let c = |i: usize| values.get(i).copied().unwrap_or(0.0) / 255.0;
    [c(0), c(1), c(2), c(3)]
}

pub fn parse_ru(text: &str) -> Result<RuFile> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Vec<Run> = Vec::new();
    let mut open = false;

    for (index, raw_line) in text.lines().enumerate() {
        let line_no = index.saturating_add(1);
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "c" {
            if open {
                entries.push(Entry {
                    runs: std::mem::take(&mut current),
                });
            }
            open = true;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("s ") {
            current.push(Run {
                style: parse_style(line_no, rest)?,
                triangles: Vec::new(),
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("t ") {
            let fields: Vec<&str> = rest.split(':').collect();
            let f = exact_floats(line_no, &fields, 6)?;
            let triangle = [
                [at(&f, 0, line_no)?, at(&f, 1, line_no)?],
                [at(&f, 2, line_no)?, at(&f, 3, line_no)?],
                [at(&f, 4, line_no)?, at(&f, 5, line_no)?],
            ];
            let run = current
                .last_mut()
                .ok_or_else(|| syntax(line_no, "triangle before any style line"))?;
            run.triangles.push(triangle);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("l ") {
            let fields: Vec<&str> = rest.split(':').collect();
            let f = exact_floats(line_no, &fields, 4)?;
            let segment = [
                [at(&f, 0, line_no)?, at(&f, 1, line_no)?],
                [at(&f, 2, line_no)?, at(&f, 3, line_no)?],
            ];
            let run = current
                .last_mut()
                .ok_or_else(|| syntax(line_no, "line before any style line"))?;
            run.lines.push(segment);
            continue;
        }
        return Err(syntax(line_no, format!("unrecognised line {trimmed:?}")));
    }
    if open {
        entries.push(Entry { runs: current });
    }
    Ok(RuFile { entries })
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
    fn parses_a_solid_triangle_entry() {
        let text = "c\ns s:35:16:16:255\nt 8.2:14.45:0:13.45:0.8:11.15\n";
        let ru = parse_ru(text).unwrap();
        assert_eq!(ru.entries.len(), 1);
        assert_eq!(ru.entries[0].runs.len(), 1);
        let run = &ru.entries[0].runs[0];
        assert_eq!(run.triangles.len(), 1);
        match run.style {
            Style::Solid { color } => {
                assert_eq!(color, [35.0 / 255.0, 16.0 / 255.0, 16.0 / 255.0, 1.0])
            }
            other => panic!("expected Solid, got {other:?}"),
        }
    }

    #[test]
    fn one_entry_may_carry_more_than_one_styled_run() {
        let text = "c\n\
                     s s:230:243:65:255\n\
                     t 2.6:-4.2:-2.85:-4.2:0:-10.9\n\
                     s l:1:230:243:65:255\n\
                     l 0:-10.9:-2.85:-4.2\n";
        let ru = parse_ru(text).unwrap();
        assert_eq!(ru.entries.len(), 1);
        assert_eq!(ru.entries[0].runs.len(), 2);
        assert_eq!(ru.entries[0].runs[0].triangles.len(), 1);
        assert_eq!(ru.entries[0].runs[1].lines.len(), 1);
        assert!(matches!(
            ru.entries[0].runs[1].style,
            Style::Line { width: 1.0, .. }
        ));
    }

    #[test]
    fn texture_style_matrix_is_transposed() {
        let text = "c\ns tc:255:255:255:255:12:1:2:3:4:5:6\nt 0:0:1:0:0:1\n";
        let ru = parse_ru(text).unwrap();
        match ru.entries[0].runs[0].style {
            Style::Texture {
                pixel_transform,
                pixel_translate,
                character,
                ..
            } => {
                assert_eq!(character, 12);
                assert_eq!(pixel_transform, [[1.0, 3.0], [2.0, 4.0]]);
                assert_eq!(pixel_translate, [5.0, 6.0]);
            }
            other => panic!("expected Texture, got {other:?}"),
        }
    }

    #[test]
    fn multiple_c_entries_are_kept_separate() {
        let text = "c\ns s:1:2:3:4\nt 0:0:1:1:2:2\nc\ns s:5:6:7:8\nt 0:0:1:1:2:2\n";
        let ru = parse_ru(text).unwrap();
        assert_eq!(ru.entries.len(), 2);
    }

    #[test]
    fn a_triangle_before_any_style_is_a_syntax_error() {
        let text = "c\nt 0:0:1:1:2:2\n";
        assert!(matches!(
            parse_ru(text),
            Err(Error::RuSyntax { line: 2, .. })
        ));
    }

    #[test]
    fn wrong_field_count_is_a_syntax_error_with_the_line_number() {
        let text = "c\ns s:1:2:3\n";
        assert!(matches!(
            parse_ru(text),
            Err(Error::RuSyntax { line: 2, .. })
        ));
    }

    #[test]
    fn an_unrecognised_line_kind_is_a_syntax_error() {
        let text = "q nonsense\n";
        assert!(matches!(
            parse_ru(text),
            Err(Error::RuSyntax { line: 1, .. })
        ));
    }

    #[test]
    fn blank_lines_are_ignored() {
        let text = "\nc\n\ns s:1:2:3:4\n\nt 0:0:1:1:2:2\n\n";
        assert!(parse_ru(text).is_ok());
    }

    #[test]
    fn empty_file_parses_to_no_entries() {
        assert_eq!(parse_ru("").unwrap().entries.len(), 0);
    }

    #[test]
    fn every_prefix_of_a_valid_ru_file_parses_without_panicking() {
        let text = "c\ns s:230:243:65:255\nt 2.6:-4.2:-2.85:-4.2:0:-10.9\n\
                     s l:1:230:243:65:255\nl 0:-10.9:-2.85:-4.2\n\
                     c\ns tc:255:255:255:255:12:1:0:0:1:1:1\nt 0:0:1:0:0:1\n";
        for len in 0..=text.len() {
            let _ = parse_ru(&text[..len]);
        }
    }
}
