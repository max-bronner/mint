use crate::array::Element;
use crate::chunk::Chunk;
use crate::reader::Reader;
use crate::{Array, ChunkType, Error, Result, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Element for Rgba {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            r: r.u8()?,
            g: r.u8()?,
            b: r.u8()?,
            a: r.u8()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Element for Rgb {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let out = Self {
            r: r.u8()?,
            g: r.u8()?,
            b: r.u8()?,
        };
        r.skip(1)?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaterialInfo {
    pub pass_count: u32,
    pub vertex_material_count: u32,
    pub shader_count: u32,
    pub texture_count: u32,
}

impl MaterialInfo {
    pub(crate) const SIZE: usize = 16;

    pub(crate) fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "MATERIAL_INFO")?;
        let mut r = Reader::new(payload);
        Ok(Self {
            pass_count: r.u32()?,
            vertex_material_count: r.u32()?,
            shader_count: r.u32()?,
            texture_count: r.u32()?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VertexMaterialInfo {
    pub attributes: u32,
    pub ambient: Rgb,
    pub diffuse: Rgb,
    pub specular: Rgb,
    pub emissive: Rgb,
    pub shininess: f32,
    pub opacity: f32,
    pub translucency: f32,
}

impl Element for VertexMaterialInfo {
    const SIZE: usize = 32;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            attributes: r.u32()?,
            ambient: Rgb::read(r)?,
            diffuse: Rgb::read(r)?,
            specular: Rgb::read(r)?,
            emissive: Rgb::read(r)?,
            shininess: r.f32()?,
            opacity: r.f32()?,
            translucency: r.f32()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UvMapperType {
    #[default]
    Uv,
    Environment,
    CheapEnvironment,
    Screen,
    LinearOffset,
    Silhouette,
    Scale,
    Grid,
    Rotate,
    SineLinearOffset,
    StepLinearOffset,
    ZigzagLinearOffset,
    WsClassicEnv,
    WsEnvironment,
    GridClassicEnv,
    GridEnvironment,
    Random,
    Edge,
    BumpEnv,
    GridWsClassicEnv,
    GridWsEnvironment,
    Unknown(u8),
}

impl UvMapperType {
    fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::Uv,
            0x01 => Self::Environment,
            0x02 => Self::CheapEnvironment,
            0x03 => Self::Screen,
            0x04 => Self::LinearOffset,
            0x05 => Self::Silhouette,
            0x06 => Self::Scale,
            0x07 => Self::Grid,
            0x08 => Self::Rotate,
            0x09 => Self::SineLinearOffset,
            0x0A => Self::StepLinearOffset,
            0x0B => Self::ZigzagLinearOffset,
            0x0C => Self::WsClassicEnv,
            0x0D => Self::WsEnvironment,
            0x0E => Self::GridClassicEnv,
            0x0F => Self::GridEnvironment,
            0x10 => Self::Random,
            0x11 => Self::Edge,
            0x12 => Self::BumpEnv,
            0x13 => Self::GridWsClassicEnv,
            0x14 => Self::GridWsEnvironment,
            other => Self::Unknown(other),
        }
    }

    #[must_use]
    pub fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::Uv
                | Self::LinearOffset
                | Self::SineLinearOffset
                | Self::Rotate
                | Self::Grid
                | Self::Environment
                | Self::CheapEnvironment
                | Self::WsClassicEnv
                | Self::WsEnvironment
                | Self::BumpEnv
        )
    }
}

impl VertexMaterialInfo {
    #[must_use]
    pub fn stage0_mapper(&self) -> UvMapperType {
        #[allow(clippy::cast_possible_truncation)]
        let code = ((self.attributes & 0x00FF_0000) >> 16) as u8;
        UvMapperType::from_code(code)
    }

    #[must_use]
    pub fn stage1_mapper(&self) -> UvMapperType {
        #[allow(clippy::cast_possible_truncation)]
        let code = ((self.attributes & 0x0000_FF00) >> 8) as u8;
        UvMapperType::from_code(code)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapperArgs {
    pub u_per_sec: f32,
    pub v_per_sec: f32,
    pub u_scale: f32,
    pub v_scale: f32,
    pub speed: f32,
    pub u_center: f32,
    pub v_center: f32,
    pub u_amp: f32,
    pub u_freq: f32,
    pub u_phase: f32,
    pub v_amp: f32,
    pub v_freq: f32,
    pub v_phase: f32,
    pub fps: f32,
    pub log2_width: u32,
    pub last: u32,
    pub offset: u32,
    pub bump_rotation: f32,
    pub bump_scale: f32,
    pub unrecognized: Vec<(String, String)>,
}

impl Default for MapperArgs {
    fn default() -> Self {
        Self {
            u_per_sec: 0.0,
            v_per_sec: 0.0,
            u_scale: 1.0,
            v_scale: 1.0,
            speed: 0.1,
            u_center: 0.0,
            v_center: 0.0,
            u_amp: 1.0,
            u_freq: 1.0,
            u_phase: 0.0,
            v_amp: 1.0,
            v_freq: 1.0,
            v_phase: 0.0,
            fps: 1.0,
            log2_width: 1,
            last: 0,
            offset: 0,
            bump_rotation: 0.0,
            bump_scale: 1.0,
            unrecognized: Vec::new(),
        }
    }
}

impl MapperArgs {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for line in text.split(['\n']).map(|line| line.trim_end_matches('\r')) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let mut value = value.trim();
            if let Some(comment_at) = value.find(';') {
                #[allow(clippy::indexing_slicing)]
                {
                    value = value[..comment_at].trim();
                }
            }
            match key {
                "BumpRotation" => {
                    out.bump_rotation = ea_get_float(value, out.bump_rotation);
                    continue;
                }
                "BumpScale" => {
                    out.bump_scale = ea_get_float(value, out.bump_scale);
                    continue;
                }
                _ => {}
            }
            let Some(parsed) = parse_mapper_value(value) else {
                out.unrecognized.push((key.to_owned(), value.to_owned()));
                continue;
            };
            match key {
                "UPerSec" | "UperSec" => out.u_per_sec = parsed,
                "VPerSec" | "VPreSec" => out.v_per_sec = parsed,
                "UScale" => out.u_scale = parsed,
                "VScale" => out.v_scale = parsed,
                "Speed" => out.speed = parsed,
                "UCenter" => out.u_center = parsed,
                "VCenter" => out.v_center = parsed,
                "UAmp" => out.u_amp = parsed,
                "UFreq" => out.u_freq = parsed,
                "UPhase" => out.u_phase = parsed,
                "VAmp" => out.v_amp = parsed,
                "VFreq" => out.v_freq = parsed,
                "VPhase" => out.v_phase = parsed,
                "FPS" => out.fps = parsed,
                "Log2Width" => out.log2_width = mapper_count(parsed),
                "Last" => out.last = mapper_count(parsed),
                "Offset" => out.offset = mapper_count(parsed),
                _ => out.unrecognized.push((key.to_owned(), value.to_owned())),
            }
        }
        out
    }
}

fn mapper_count(value: f32) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = value as u32;
    count
}

fn ea_get_float(value: &str, default: f32) -> f32 {
    let value = value.trim_start();
    let numeric = value
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')))
        .unwrap_or(value.len());
    let parsed = (1..=numeric)
        .rev()
        .find_map(|end| {
            value
                .get(..end)
                .and_then(|prefix| prefix.parse::<f32>().ok())
        })
        .unwrap_or(default);
    if value.contains('%') {
        parsed / 100.0
    } else {
        parsed
    }
}

fn parse_mapper_value(value: &str) -> Option<f32> {
    if let Ok(parsed) = value.parse::<f32>() {
        return Some(parsed);
    }
    let fixed = value.trim_end_matches(['f', 'F']).replace("..", ".");
    fixed.parse::<f32>().ok()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VertexMaterial {
    pub name: String,
    pub info: Option<VertexMaterialInfo>,
    pub mapper_args0: Option<String>,
    pub mapper_args1: Option<String>,
}

impl VertexMaterial {
    #[must_use]
    pub fn mapper_args0_parsed(&self) -> MapperArgs {
        self.mapper_args0
            .as_deref()
            .map_or_else(MapperArgs::default, MapperArgs::parse)
    }

    #[must_use]
    pub fn mapper_args1_parsed(&self) -> MapperArgs {
        self.mapper_args1
            .as_deref()
            .map_or_else(MapperArgs::default, MapperArgs::parse)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shader {
    pub depth_compare: u8,
    pub depth_mask: u8,
    pub color_mask: u8,
    pub dest_blend: u8,
    pub fog_func: u8,
    pub pri_gradient: u8,
    pub sec_gradient: u8,
    pub src_blend: u8,
    pub texturing: u8,
    pub detail_color_func: u8,
    pub detail_alpha_func: u8,
    pub shader_preset: u8,
    pub alpha_test: u8,
    pub post_detail_color_func: u8,
    pub post_detail_alpha_func: u8,
}

impl Element for Shader {
    const SIZE: usize = 16;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let out = Self {
            depth_compare: r.u8()?,
            depth_mask: r.u8()?,
            color_mask: r.u8()?,
            dest_blend: r.u8()?,
            fog_func: r.u8()?,
            pri_gradient: r.u8()?,
            sec_gradient: r.u8()?,
            src_blend: r.u8()?,
            texturing: r.u8()?,
            detail_color_func: r.u8()?,
            detail_alpha_func: r.u8()?,
            shader_preset: r.u8()?,
            alpha_test: r.u8()?,
            post_detail_color_func: r.u8()?,
            post_detail_alpha_func: r.u8()?,
        };
        r.skip(1)?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextureInfo {
    pub attributes: u16,
    pub anim_type: u16,
    pub frame_count: u32,
    pub frame_rate: f32,
}

impl TextureInfo {
    pub(crate) const SIZE: usize = 12;

    pub const PUBLISH: u16 = 0x0001;
    pub const NO_LOD: u16 = 0x0004;
    pub const CLAMP_U: u16 = 0x0008;
    pub const CLAMP_V: u16 = 0x0010;
    pub const ALPHA_BITMAP: u16 = 0x0020;

    fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "TEXTURE_INFO")?;
        let mut r = Reader::new(payload);
        Ok(Self {
            attributes: r.u16()?,
            anim_type: r.u16()?,
            frame_count: r.u32()?,
            frame_rate: r.f32()?,
        })
    }

    #[must_use]
    pub fn clamp_u(&self) -> bool {
        self.attributes & Self::CLAMP_U != 0
    }

    #[must_use]
    pub fn clamp_v(&self) -> bool {
        self.attributes & Self::CLAMP_V != 0
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Texture {
    pub name: String,
    pub info: Option<TextureInfo>,
}

#[derive(Debug, Clone, Copy)]
pub enum IndexTable<'a> {
    Shared(u32),
    PerElement(Array<'a, u32>),
}

impl<'a> IndexTable<'a> {
    fn parse(what: &'static str, payload: &'a [u8]) -> Result<Self> {
        if payload.len() == 4 {
            let mut r = Reader::new(payload);
            Ok(Self::Shared(r.u32()?))
        } else {
            Ok(Self::PerElement(Array::new(what, payload)?))
        }
    }

    #[must_use]
    pub fn first(&self) -> Option<u32> {
        let id = match self {
            Self::Shared(id) => *id,
            Self::PerElement(a) => a.get(0)?.ok()?,
        };
        (id != u32::MAX).then_some(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Shared(_) => 1,
            Self::PerElement(a) => a.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct TextureStage<'a> {
    pub texture_ids: Option<IndexTable<'a>>,
    pub texcoords: Array<'a, Vec2>,
    pub per_face_texcoord_ids: Array<'a, u32>,
}

impl<'a> TextureStage<'a> {
    fn parse(chunk: &Chunk<'a>) -> Result<Self> {
        let mut out = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::TEXTURE_IDS => {
                    out.texture_ids = Some(IndexTable::parse("TEXTURE_IDS", kid.payload())?);
                }
                ChunkType::STAGE_TEXCOORDS => out.texcoords = kid.array("STAGE_TEXCOORDS")?,
                ChunkType::PER_FACE_TEXCOORD_IDS => {
                    out.per_face_texcoord_ids = kid.array("PER_FACE_TEXCOORD_IDS")?;
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MaterialPass<'a> {
    pub vertex_material_ids: Option<IndexTable<'a>>,
    pub shader_ids: Option<IndexTable<'a>>,
    pub diffuse_colors: Array<'a, Rgba>,
    pub shader_material_ids: Option<IndexTable<'a>>,
    pub texture_stages: Vec<TextureStage<'a>>,
    pub texcoords: Array<'a, Vec2>,
}

impl<'a> MaterialPass<'a> {
    pub(crate) fn parse(chunk: &Chunk<'a>) -> Result<Self> {
        let mut out = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::VERTEX_MATERIAL_IDS => {
                    out.vertex_material_ids =
                        Some(IndexTable::parse("VERTEX_MATERIAL_IDS", kid.payload())?);
                }
                ChunkType::SHADER_IDS => {
                    out.shader_ids = Some(IndexTable::parse("SHADER_IDS", kid.payload())?);
                }
                ChunkType::DCG => out.diffuse_colors = kid.array("DCG")?,
                ChunkType::SHADER_MATERIAL_ID => {
                    out.shader_material_ids =
                        Some(IndexTable::parse("SHADER_MATERIAL_ID", kid.payload())?);
                }
                ChunkType::TEXTURE_STAGE => out.texture_stages.push(TextureStage::parse(&kid)?),
                ChunkType::STAGE_TEXCOORDS => out.texcoords = kid.array("STAGE_TEXCOORDS")?,
                _ => {}
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShaderMaterialValue {
    Texture(String),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Int(u32),
    Bool(bool),
    Unknown { tag: u32, bytes: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShaderMaterialProperty {
    pub name: String,
    pub value: ShaderMaterialValue,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderMaterial {
    pub shader_name: String,
    pub version: u8,
    pub unknown: u32,
    pub properties: Vec<ShaderMaterialProperty>,
}

impl ShaderMaterial {
    const HEADER_SIZE: usize = 37;

    #[must_use]
    pub fn property(&self, name: &str) -> Option<&ShaderMaterialValue> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| &p.value)
    }

    #[must_use]
    pub fn diffuse_texture(&self) -> Option<&str> {
        let value = self
            .property("DiffuseTexture")
            .or_else(|| self.property("Texture_0"))?;
        match value {
            ShaderMaterialValue::Texture(t) => Some(t.as_str()),
            _ => None,
        }
    }

    #[must_use]
    pub fn normal_map(&self) -> Option<&str> {
        match self.property("NormalMap")? {
            ShaderMaterialValue::Texture(t) => Some(t.as_str()),
            _ => None,
        }
    }

    fn parse_header(&mut self, payload: &[u8]) -> Result<()> {
        need(payload, Self::HEADER_SIZE, "SHADER_MATERIAL_HEADER")?;
        let mut r = Reader::new(payload);
        self.version = r.u8()?;
        self.shader_name = r.fixed_name(32)?;
        self.unknown = r.u32()?;
        Ok(())
    }

    fn parse_property(payload: &[u8]) -> Result<ShaderMaterialProperty> {
        let mut r = Reader::new(payload);
        let tag = r.u32()?;
        let name_len = usize::try_from(r.u32()?).unwrap_or(usize::MAX);
        let name = crate::reader::decode_name(r.take(name_len)?);
        let value = match tag {
            1 => {
                let len = usize::try_from(r.u32()?).unwrap_or(usize::MAX);
                ShaderMaterialValue::Texture(crate::reader::decode_name(r.take(len)?))
            }
            2 => ShaderMaterialValue::Float(r.f32()?),
            3 => ShaderMaterialValue::Vec2([r.f32()?, r.f32()?]),
            4 => ShaderMaterialValue::Vec3([r.f32()?, r.f32()?, r.f32()?]),
            5 => ShaderMaterialValue::Vec4([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
            6 => ShaderMaterialValue::Int(r.u32()?),
            7 => ShaderMaterialValue::Bool(r.u8()? != 0),
            tag => ShaderMaterialValue::Unknown {
                tag,
                bytes: r.rest().to_vec(),
            },
        };
        Ok(ShaderMaterialProperty { name, value })
    }
}

pub(crate) fn parse_shader_materials(chunk: &Chunk<'_>) -> Result<Vec<ShaderMaterial>> {
    let mut out = Vec::new();
    for kid in chunk.children()? {
        let kid = kid?;
        if kid.kind() != ChunkType::SHADER_MATERIAL {
            continue;
        }
        let mut mat = ShaderMaterial::default();
        for field in kid.children()? {
            let field = field?;
            match field.kind() {
                ChunkType::SHADER_MATERIAL_HEADER => mat.parse_header(field.payload())?,
                ChunkType::SHADER_MATERIAL_PROPERTY => {
                    mat.properties
                        .push(ShaderMaterial::parse_property(field.payload())?);
                }
                _ => {}
            }
        }
        out.push(mat);
    }
    Ok(out)
}

pub(crate) fn parse_textures(chunk: &Chunk<'_>) -> Result<Vec<Texture>> {
    let mut out = Vec::new();
    for kid in chunk.children()? {
        let kid = kid?;
        if kid.kind() != ChunkType::TEXTURE {
            continue;
        }
        let mut tex = Texture::default();
        for field in kid.children()? {
            let field = field?;
            match field.kind() {
                ChunkType::TEXTURE_NAME => tex.name = field.text(),
                ChunkType::TEXTURE_INFO => tex.info = Some(TextureInfo::parse(field.payload())?),
                _ => {}
            }
        }
        out.push(tex);
    }
    Ok(out)
}

pub(crate) fn parse_vertex_materials(chunk: &Chunk<'_>) -> Result<Vec<VertexMaterial>> {
    let mut out = Vec::new();
    for kid in chunk.children()? {
        let kid = kid?;
        if kid.kind() != ChunkType::VERTEX_MATERIAL {
            continue;
        }
        let mut vm = VertexMaterial::default();
        for field in kid.children()? {
            let field = field?;
            match field.kind() {
                ChunkType::VERTEX_MATERIAL_NAME => vm.name = field.text(),
                ChunkType::VERTEX_MATERIAL_INFO => {
                    need(
                        field.payload(),
                        VertexMaterialInfo::SIZE,
                        "VERTEX_MATERIAL_INFO",
                    )?;
                    vm.info = Some(VertexMaterialInfo::read(&mut Reader::new(field.payload()))?);
                }
                ChunkType::VERTEX_MAPPER_ARGS0 => vm.mapper_args0 = Some(field.text()),
                ChunkType::VERTEX_MAPPER_ARGS1 => vm.mapper_args1 = Some(field.text()),
                _ => {}
            }
        }
        out.push(vm);
    }
    Ok(out)
}

pub(crate) fn need(payload: &[u8], expected: usize, what: &'static str) -> Result<()> {
    if payload.len() < expected {
        return Err(Error::ShortStruct {
            what,
            expected,
            actual: payload.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
