use crate::reader::{checked_add, checked_mul, cstr_at, f32_at, i32_at, u32_at};
use crate::{Error, Result};

pub const CHARACTER_SIGNATURE: u32 = 0x0987_6543;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Movie {
    pub frame_count: u32,
    pub frames_offset: u32,
    pub character_count: u32,
    pub characters_offset: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub unknown_36: u32,
    pub import_count: u32,
    pub imports_offset: u32,
    pub export_count: u32,
    pub exports_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonRecord {
    pub flags: u32,
    pub character: u32,
    pub depth: i32,
    pub transform: Transform,
    pub translate: Vector2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub bounds: Rect,
    pub hit_triangle_count: u32,
    pub hit_vertex_count: u32,
    pub hit_vertices_offset: u32,
    pub hit_triangles_offset: u32,
    pub records: Vec<ButtonRecord>,
    pub action_count: u32,
    pub actions_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub frame_count: u32,
    pub frames_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub index: u32,
    pub advance: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRecord {
    pub font: u32,
    pub color: [f32; 4],
    pub offset: Vector2,
    pub text_height: f32,
    pub glyphs: Vec<Glyph>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Character {
    Shape {
        bounds: Rect,
        geometry_id: u32,
    },
    EditText {
        bounds: Rect,
        font: u32,
        alignment: u32,
        color: u32,
        font_height: f32,
        readonly: u32,
        multiline: u32,
        wordwrap: u32,
        text: Option<String>,
        variable: Option<String>,
    },
    Font {
        name: Option<String>,
        glyphs: Vec<u32>,
    },
    Button(Button),
    Sprite(Sprite),
    Image {
        texture: u32,
    },
    Morph {
        start_shape: u32,
        end_shape: u32,
    },
    Movie(Movie),
    Text {
        bounds: Rect,
        transform: Transform,
        translate: Vector2,
        records: Vec<TextRecord>,
    },
    Placeholder,
    Unknown {
        character_type: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub movie: String,
    pub name: String,
    pub local_character: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub name: String,
    pub character: u32,
}

pub fn parse_movie(apt: &[u8], offset: usize) -> Result<Movie> {
    let ty = u32_at(apt, offset)?;
    if ty != 9 {
        return Err(Error::NotAMovieRecord { offset, ty });
    }
    let signature = u32_at(apt, field(offset, 4)?)?;
    if signature != CHARACTER_SIGNATURE {
        return Err(Error::BadSignature { offset, signature });
    }
    Ok(Movie {
        frame_count: u32_at(apt, field(offset, 8)?)?,
        frames_offset: u32_at(apt, field(offset, 12)?)?,
        character_count: u32_at(apt, field(offset, 20)?)?,
        characters_offset: u32_at(apt, field(offset, 24)?)?,
        screen_width: u32_at(apt, field(offset, 28)?)?,
        screen_height: u32_at(apt, field(offset, 32)?)?,
        unknown_36: u32_at(apt, field(offset, 36)?)?,
        import_count: u32_at(apt, field(offset, 40)?)?,
        imports_offset: u32_at(apt, field(offset, 44)?)?,
        export_count: u32_at(apt, field(offset, 48)?)?,
        exports_offset: u32_at(apt, field(offset, 52)?)?,
    })
}

fn field(base: usize, delta: usize) -> Result<usize> {
    checked_add(base, delta)
}

fn read_rect(apt: &[u8], offset: usize) -> Result<Rect> {
    Ok(Rect {
        left: f32_at(apt, field(offset, 0)?)?,
        top: f32_at(apt, field(offset, 4)?)?,
        right: f32_at(apt, field(offset, 8)?)?,
        bottom: f32_at(apt, field(offset, 12)?)?,
    })
}

fn read_transform(apt: &[u8], offset: usize) -> Result<Transform> {
    Ok(Transform {
        m00: f32_at(apt, field(offset, 0)?)?,
        m01: f32_at(apt, field(offset, 4)?)?,
        m10: f32_at(apt, field(offset, 8)?)?,
        m11: f32_at(apt, field(offset, 12)?)?,
    })
}

fn read_vector2(apt: &[u8], offset: usize) -> Result<Vector2> {
    Ok(Vector2 {
        x: f32_at(apt, field(offset, 0)?)?,
        y: f32_at(apt, field(offset, 4)?)?,
    })
}

fn opt_cstr(apt: &[u8], ptr: u32) -> Result<Option<String>> {
    if ptr == 0 {
        Ok(None)
    } else {
        Ok(Some(cstr_at(apt, ptr as usize)?))
    }
}

pub fn parse_characters(apt: &[u8], record: &Movie) -> Result<Vec<Character>> {
    let count = record.character_count as usize;
    let mut out = Vec::with_capacity(count.min(apt.len().saturating_add(1).saturating_div(4)));
    for index in 0..count {
        let slot = field(record.characters_offset as usize, checked_mul(index, 4)?)?;
        let ptr = u32_at(apt, slot)? as usize;
        let header = u32_at(apt, ptr).and_then(|ty| Ok((ty, u32_at(apt, field(ptr, 4)?)?)));
        let character = match header {
            Ok((ty, signature)) if signature == CHARACTER_SIGNATURE => {
                decode_character(apt, ptr, ty)?
            }
            _ => Character::Placeholder,
        };
        out.push(character);
    }
    Ok(out)
}

fn decode_character(apt: &[u8], ptr: usize, ty: u32) -> Result<Character> {
    match ty {
        1 => Ok(Character::Shape {
            bounds: read_rect(apt, field(ptr, 8)?)?,
            geometry_id: u32_at(apt, field(ptr, 24)?)?,
        }),
        2 => Ok(Character::EditText {
            bounds: read_rect(apt, field(ptr, 8)?)?,
            font: u32_at(apt, field(ptr, 24)?)?,
            alignment: u32_at(apt, field(ptr, 28)?)?,
            color: u32_at(apt, field(ptr, 32)?)?,
            font_height: f32_at(apt, field(ptr, 36)?)?,
            readonly: u32_at(apt, field(ptr, 40)?)?,
            multiline: u32_at(apt, field(ptr, 44)?)?,
            wordwrap: u32_at(apt, field(ptr, 48)?)?,
            text: opt_cstr(apt, u32_at(apt, field(ptr, 52)?)?)?,
            variable: opt_cstr(apt, u32_at(apt, field(ptr, 56)?)?)?,
        }),
        3 => Ok(Character::Font {
            name: opt_cstr(apt, u32_at(apt, field(ptr, 8)?)?)?,
            glyphs: read_u32_table(
                apt,
                u32_at(apt, field(ptr, 12)?)?,
                u32_at(apt, field(ptr, 16)?)?,
            )?,
        }),
        4 => decode_button(apt, ptr).map(Character::Button),
        5 => Ok(Character::Sprite(Sprite {
            frame_count: u32_at(apt, field(ptr, 8)?)?,
            frames_offset: u32_at(apt, field(ptr, 12)?)?,
        })),
        7 => Ok(Character::Image {
            texture: u32_at(apt, field(ptr, 8)?)?,
        }),
        8 => Ok(Character::Morph {
            start_shape: u32_at(apt, field(ptr, 8)?)?,
            end_shape: u32_at(apt, field(ptr, 12)?)?,
        }),
        9 => parse_movie(apt, ptr).map(Character::Movie),
        10 => Ok(Character::Text {
            bounds: read_rect(apt, field(ptr, 8)?)?,
            transform: read_transform(apt, field(ptr, 24)?)?,
            translate: read_vector2(apt, field(ptr, 40)?)?,
            records: read_text_records(
                apt,
                u32_at(apt, field(ptr, 48)?)?,
                u32_at(apt, field(ptr, 52)?)?,
            )?,
        }),
        other => Ok(Character::Unknown {
            character_type: other,
        }),
    }
}

fn decode_button(apt: &[u8], ptr: usize) -> Result<Button> {
    let record_count = u32_at(apt, field(ptr, 44)?)?;
    let records_offset = u32_at(apt, field(ptr, 48)?)?;
    let mut records = Vec::with_capacity(
        (record_count as usize).min(apt.len().saturating_add(1).saturating_div(68)),
    );
    for index in 0..record_count as usize {
        let base = field(records_offset as usize, checked_mul(index, 68)?)?;
        records.push(ButtonRecord {
            flags: u32_at(apt, field(base, 0)?)?,
            character: u32_at(apt, field(base, 4)?)?,
            depth: i32_at(apt, field(base, 8)?)?,
            transform: read_transform(apt, field(base, 12)?)?,
            translate: read_vector2(apt, field(base, 28)?)?,
        });
    }
    Ok(Button {
        bounds: read_rect(apt, field(ptr, 12)?)?,
        hit_triangle_count: u32_at(apt, field(ptr, 28)?)?,
        hit_vertex_count: u32_at(apt, field(ptr, 32)?)?,
        hit_vertices_offset: u32_at(apt, field(ptr, 36)?)?,
        hit_triangles_offset: u32_at(apt, field(ptr, 40)?)?,
        records,
        action_count: u32_at(apt, field(ptr, 52)?)?,
        actions_offset: u32_at(apt, field(ptr, 56)?)?,
    })
}

fn read_u32_table(apt: &[u8], count: u32, offset: u32) -> Result<Vec<u32>> {
    let mut out =
        Vec::with_capacity((count as usize).min(apt.len().saturating_add(1).saturating_div(4)));
    for index in 0..count as usize {
        let at = field(offset as usize, checked_mul(index, 4)?)?;
        out.push(u32_at(apt, at)?);
    }
    Ok(out)
}

fn read_glyph_table(apt: &[u8], count: u32, offset: u32) -> Result<Vec<Glyph>> {
    let mut out =
        Vec::with_capacity((count as usize).min(apt.len().saturating_add(1).saturating_div(8)));
    for index in 0..count as usize {
        let base = field(offset as usize, checked_mul(index, 8)?)?;
        out.push(Glyph {
            index: u32_at(apt, field(base, 0)?)?,
            advance: i32_at(apt, field(base, 4)?)?,
        });
    }
    Ok(out)
}

fn read_text_records(apt: &[u8], count: u32, offset: u32) -> Result<Vec<TextRecord>> {
    let mut out =
        Vec::with_capacity((count as usize).min(apt.len().saturating_add(1).saturating_div(56)));
    for index in 0..count as usize {
        let base = field(offset as usize, checked_mul(index, 56)?)?;
        let color = read_rect(apt, field(base, 4)?)?;
        out.push(TextRecord {
            font: u32_at(apt, field(base, 0)?)?,
            color: [color.top, color.left, color.bottom, color.right],
            offset: read_vector2(apt, field(base, 36)?)?,
            text_height: f32_at(apt, field(base, 44)?)?,
            glyphs: read_glyph_table(
                apt,
                u32_at(apt, field(base, 48)?)?,
                u32_at(apt, field(base, 52)?)?,
            )?,
        });
    }
    Ok(out)
}

pub fn parse_imports(apt: &[u8], record: &Movie) -> Result<Vec<Import>> {
    let count = record.import_count as usize;
    let mut out = Vec::with_capacity(count.min(apt.len().saturating_add(1).saturating_div(16)));
    for index in 0..count {
        let base = field(record.imports_offset as usize, checked_mul(index, 16)?)?;
        out.push(Import {
            movie: cstr_at(apt, u32_at(apt, field(base, 0)?)? as usize)?,
            name: cstr_at(apt, u32_at(apt, field(base, 4)?)? as usize)?,
            local_character: u32_at(apt, field(base, 8)?)?,
        });
    }
    Ok(out)
}

pub fn parse_exports(apt: &[u8], record: &Movie) -> Result<Vec<Export>> {
    let count = record.export_count as usize;
    let mut out = Vec::with_capacity(count.min(apt.len().saturating_add(1).saturating_div(8)));
    for index in 0..count {
        let base = field(record.exports_offset as usize, checked_mul(index, 8)?)?;
        out.push(Export {
            name: cstr_at(apt, u32_at(apt, field(base, 0)?)? as usize)?,
            character: u32_at(apt, field(base, 4)?)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AptMovie {
    pub name: String,
    pub record: Movie,
    pub characters: Vec<Character>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
}

impl AptMovie {
    pub fn load(name: &str, const_bytes: &[u8], apt_bytes: &[u8]) -> Result<Self> {
        let const_file = crate::const_file::parse_const(const_bytes)?;
        let record = parse_movie(apt_bytes, const_file.movie_offset as usize)?;
        let characters = parse_characters(apt_bytes, &record)?;
        let imports = parse_imports(apt_bytes, &record)?;
        let exports = parse_exports(apt_bytes, &record)?;
        Ok(Self {
            name: name.to_ascii_lowercase(),
            record,
            characters,
            imports,
            exports,
        })
    }
}

#[must_use]
pub fn ru_path(movie_name: &str, geometry_id: u32) -> String {
    format!("{movie_name}_geometry/{geometry_id}.ru")
}

#[must_use]
pub fn shared_atlas_path(movie_name: &str, page: u32) -> String {
    format!("art/textures/apt_{movie_name}_{page}.tga")
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
mod tests;
