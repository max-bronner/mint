use crate::array::Element;
use crate::chunk::Chunk;
use crate::material::{need, parse_shader_materials, parse_textures, parse_vertex_materials};
use crate::reader::Reader;
use crate::{
    Array, ChunkType, IndexTable, MaterialInfo, MaterialPass, Result, Rgba, Shader, ShaderMaterial,
    Texture, TextureInfo, Vec2, Vec3, VertexMaterial,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshHeader3 {
    pub version: u32,
    pub attributes: u32,
    pub mesh_name: String,
    pub container_name: String,
    pub num_tris: u32,
    pub num_vertices: u32,
    pub num_materials: u32,
    pub num_damage_stages: u32,
    pub sort_level: i32,
    pub prelit_version: u32,
    pub vertex_channels: u32,
    pub face_channels: u32,
    pub bound_box_min: Vec3,
    pub bound_box_max: Vec3,
    pub bound_sphere_center: Vec3,
    pub bound_sphere_radius: f32,
}

impl MeshHeader3 {
    pub const SIZE: usize = 116;

    pub const GEOMETRY_TYPE_MASK: u32 = 0x00FF_0000;
    pub const GEOMETRY_TYPE_NORMAL: u32 = 0x0000_0000;
    pub const GEOMETRY_TYPE_CAMERA_ALIGNED: u32 = 0x0001_0000;
    pub const GEOMETRY_TYPE_SKIN: u32 = 0x0002_0000;
    pub const GEOMETRY_TYPE_OBSOLETE_SHADOW: u32 = 0x0003_0000;
    pub const GEOMETRY_TYPE_AABOX: u32 = 0x0004_0000;
    pub const GEOMETRY_TYPE_OBBOX: u32 = 0x0005_0000;
    pub const GEOMETRY_TYPE_CAMERA_ORIENTED: u32 = 0x0006_0000;

    pub const HIDDEN: u32 = 0x0000_1000;
    pub const TWO_SIDED: u32 = 0x0000_2000;

    pub(crate) fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "MESH_HEADER3")?;
        let mut r = Reader::new(payload);
        let version = r.u32()?;
        let attributes = r.u32()?;
        let mesh_name = r.fixed_name(16)?;
        let container_name = r.fixed_name(16)?;
        let num_tris = r.u32()?;
        let num_vertices = r.u32()?;
        let num_materials = r.u32()?;
        let num_damage_stages = r.u32()?;
        let sort_level = r.i32()?;
        let prelit_version = r.u32()?;
        r.skip(4)?;
        Ok(Self {
            version,
            attributes,
            mesh_name,
            container_name,
            num_tris,
            num_vertices,
            num_materials,
            num_damage_stages,
            sort_level,
            prelit_version,
            vertex_channels: r.u32()?,
            face_channels: r.u32()?,
            bound_box_min: Vec3::read(&mut r)?,
            bound_box_max: Vec3::read(&mut r)?,
            bound_sphere_center: Vec3::read(&mut r)?,
            bound_sphere_radius: r.f32()?,
        })
    }

    #[must_use]
    pub fn geometry_type(&self) -> u32 {
        self.attributes & Self::GEOMETRY_TYPE_MASK
    }

    #[must_use]
    pub fn is_skin(&self) -> bool {
        self.geometry_type() == Self::GEOMETRY_TYPE_SKIN
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.attributes & Self::HIDDEN != 0
    }

    #[must_use]
    pub fn is_two_sided(&self) -> bool {
        self.attributes & Self::TWO_SIDED != 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Triangle {
    pub indices: [u32; 3],
    pub attributes: u32,
    pub normal: Vec3,
    pub dist: f32,
}

impl Element for Triangle {
    const SIZE: usize = 32;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            indices: [r.u32()?, r.u32()?, r.u32()?],
            attributes: r.u32()?,
            normal: Vec3::read(r)?,
            dist: r.f32()?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VertexInfluence {
    pub bones: [u16; 2],
    pub weights: [u16; 2],
}

impl VertexInfluence {
    #[must_use]
    pub fn weights_percent(&self) -> [u16; 2] {
        match self.weights {
            [0, 0] => [100, 0],
            other => other,
        }
    }

    #[must_use]
    pub fn weights_fraction(&self) -> [f32; 2] {
        let [a, b] = self.weights_percent();
        [f32::from(a) / 100.0, f32::from(b) / 100.0]
    }
}

impl Element for VertexInfluence {
    const SIZE: usize = 8;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            bones: [r.u16()?, r.u16()?],
            weights: [r.u16()?, r.u16()?],
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Mesh<'a> {
    pub header: MeshHeader3,
    pub user_text: Option<String>,

    pub positions: Array<'a, Vec3>,
    pub normals: Array<'a, Vec3>,
    pub triangles: Array<'a, Triangle>,
    pub influences: Array<'a, VertexInfluence>,
    pub shade_indices: Array<'a, u32>,

    pub tangents: Array<'a, Vec3>,
    pub bitangents: Array<'a, Vec3>,
    pub positions2: Array<'a, Vec3>,
    pub normals2: Array<'a, Vec3>,

    pub material_info: Option<MaterialInfo>,
    pub textures: Vec<Texture>,
    pub vertex_materials: Vec<VertexMaterial>,
    pub shaders: Array<'a, Shader>,
    pub passes: Vec<MaterialPass<'a>>,
    pub shader_materials: Vec<ShaderMaterial>,
}

impl<'a> Mesh<'a> {
    pub fn parse(chunk: &Chunk<'a>) -> Result<Self> {
        let mut mesh = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::MESH_HEADER3 => mesh.header = MeshHeader3::parse(kid.payload())?,
                ChunkType::MESH_USER_TEXT => mesh.user_text = Some(kid.text()),
                ChunkType::VERTICES => mesh.positions = kid.array("VERTICES")?,
                ChunkType::VERTEX_NORMALS => mesh.normals = kid.array("VERTEX_NORMALS")?,
                ChunkType::TRIANGLES => mesh.triangles = kid.array("TRIANGLES")?,
                ChunkType::VERTEX_INFLUENCES => mesh.influences = kid.array("VERTEX_INFLUENCES")?,
                ChunkType::VERTEX_SHADE_INDICES => {
                    mesh.shade_indices = kid.array("VERTEX_SHADE_INDICES")?;
                }
                ChunkType::MATERIAL_INFO => {
                    mesh.material_info = Some(MaterialInfo::parse(kid.payload())?);
                }
                ChunkType::TEXTURES => mesh.textures = parse_textures(&kid)?,
                ChunkType::VERTEX_MATERIALS => {
                    mesh.vertex_materials = parse_vertex_materials(&kid)?;
                }
                ChunkType::SHADERS => mesh.shaders = kid.array("SHADERS")?,
                ChunkType::MATERIAL_PASS => mesh.passes.push(MaterialPass::parse(&kid)?),
                ChunkType::SHADER_MATERIALS => {
                    mesh.shader_materials = parse_shader_materials(&kid)?;
                }
                ChunkType::TANGENTS => mesh.tangents = kid.array("TANGENTS")?,
                ChunkType::BITANGENTS => mesh.bitangents = kid.array("BITANGENTS")?,
                ChunkType::VERTICES_2 => mesh.positions2 = kid.array("VERTICES_2")?,
                ChunkType::VERTEX_NORMALS_2 => mesh.normals2 = kid.array("VERTEX_NORMALS_2")?,
                _ => {}
            }
        }
        Ok(mesh)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.header.mesh_name
    }

    #[must_use]
    pub fn is_pick_proxy(&self) -> bool {
        self.name()
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("PICK"))
    }

    #[must_use]
    pub fn uv_tangents(&self) -> Array<'a, Vec3> {
        self.bitangents
    }

    #[must_use]
    pub fn uv_bitangents(&self) -> Array<'a, Vec3> {
        self.tangents
    }

    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    #[must_use]
    pub fn counts_agree(&self) -> bool {
        usize::try_from(self.header.num_vertices).is_ok_and(|n| n == self.vertex_count())
            && usize::try_from(self.header.num_tris).is_ok_and(|n| n == self.triangle_count())
    }

    #[must_use]
    pub fn texcoords(&self) -> Array<'a, Vec2> {
        let Some(pass) = self.passes.first() else {
            return Array::default();
        };
        match pass.texture_stages.first() {
            Some(stage) if !stage.texcoords.is_empty() => stage.texcoords,
            _ => pass.texcoords,
        }
    }

    #[must_use]
    pub fn diffuse_colors(&self) -> Array<'a, Rgba> {
        self.passes
            .first()
            .map_or(Array::default(), |pass| pass.diffuse_colors)
    }

    #[must_use]
    pub fn stage2_texcoords(&self) -> Array<'a, Vec2> {
        let Some(pass) = self.passes.first() else {
            return Array::default();
        };
        match pass.texture_stages.get(1) {
            Some(stage) if !stage.texcoords.is_empty() => stage.texcoords,
            _ => self.texcoords(),
        }
    }

    #[must_use]
    pub fn texture_name(&self) -> Option<&str> {
        self.legacy_texture_name().or_else(|| {
            self.shader_materials
                .iter()
                .find_map(ShaderMaterial::diffuse_texture)
        })
    }

    #[must_use]
    pub fn normal_map_name(&self) -> Option<&str> {
        self.shader_materials
            .iter()
            .find_map(ShaderMaterial::normal_map)
    }

    fn legacy_texture_name(&self) -> Option<&str> {
        Some(&self.legacy_texture()?.name)
    }

    #[must_use]
    pub fn texture_info(&self) -> Option<&TextureInfo> {
        self.legacy_texture()?.info.as_ref()
    }

    fn legacy_texture(&self) -> Option<&Texture> {
        let id = self
            .passes
            .first()?
            .texture_stages
            .first()?
            .texture_ids
            .as_ref()?
            .first()?;
        self.textures.get(usize::try_from(id).ok()?)
    }

    #[must_use]
    pub fn stage2_texture_name(&self) -> Option<&str> {
        Some(&self.stage2_texture()?.name)
    }

    #[must_use]
    pub fn stage2_texture_info(&self) -> Option<&TextureInfo> {
        self.stage2_texture()?.info.as_ref()
    }

    fn stage2_texture(&self) -> Option<&Texture> {
        let id = self
            .passes
            .first()?
            .texture_stages
            .get(1)?
            .texture_ids
            .as_ref()?
            .first()?;
        self.textures.get(usize::try_from(id).ok()?)
    }

    #[must_use]
    pub fn vertex_material(&self) -> Option<&VertexMaterial> {
        let id = self.passes.first()?.vertex_material_ids.as_ref()?.first()?;
        self.vertex_materials.get(usize::try_from(id).ok()?)
    }

    #[must_use]
    pub fn drawn_shaders(&self) -> Vec<Shader> {
        let all = || self.shaders.iter().flatten().collect::<Vec<_>>();
        let Some(ids) = self
            .passes
            .first()
            .and_then(|pass| pass.shader_ids.as_ref())
        else {
            return all();
        };
        let at = |id: u32| {
            usize::try_from(id)
                .ok()
                .and_then(|i| self.shaders.get(i))
                .and_then(Result::ok)
        };
        let selected: Vec<Shader> = match ids {
            IndexTable::Shared(id) => at(*id).into_iter().collect(),
            IndexTable::PerElement(array) => {
                let mut seen: Vec<u32> = Vec::new();
                let mut out = Vec::new();
                for id in array.iter().flatten() {
                    if id == u32::MAX || seen.contains(&id) {
                        continue;
                    }
                    seen.push(id);
                    if let Some(shader) = at(id) {
                        out.push(shader);
                    }
                }
                out
            }
        };
        if selected.is_empty() {
            return all();
        }
        selected
    }
}

#[cfg(test)]
mod tests;
