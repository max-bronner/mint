use mint_render::{
    Aabb, AddressMode, Blend, DetailAlphaCombine, DetailCombine, EnvMap, Mat4, MeshData,
    Modulate2x, Vec3, Vertex,
};
use mint_sage_asset::Transform as PoseTransform;
use mint_sage_w3d::{
    Array, Hierarchy, IndexTable, MapperArgs, Mesh, Model, Quat, Shader, ShaderMaterialValue,
    Triangle, UvMapperType, Vec2, VertexInfluence, VertexMaterial, VertexMaterialInfo,
};

use crate::Result;

#[derive(Debug, Clone)]
pub struct SkinSource {
    pub bone_space: MeshData,
    pub second_bone_space: Vec<SecondBoneVertex>,
    pub influences: Vec<VertexInfluence>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SecondBoneVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub name: String,
    pub mesh: MeshData,
    pub texture: Option<String>,
    pub normal_map: Option<String>,
    pub texture2: Option<String>,
    pub house_color: bool,
    pub detail_combine: DetailCombine,
    pub detail_alpha_combine: DetailAlphaCombine,
    pub address2_u: AddressMode,
    pub address2_v: AddressMode,
    pub transform: Mat4,
    pub alpha_test: bool,
    pub blend: Blend,
    pub fog_disabled: bool,
    pub skinned: bool,
    pub skin: Option<SkinSource>,
    pub pivot: Option<usize>,
    pub pick_proxy: bool,
    pub hidden: bool,
    pub cull: bool,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub dropped_triangles: usize,
    pub material_ambient: [f32; 3],
    pub material_diffuse: [f32; 3],
    pub material_emissive: [f32; 3],
    pub material_opacity: f32,
    pub uv_mapper: Option<UvMapperState>,
    pub env_map0: Option<EnvMap>,
    pub env_map1: Option<EnvMap>,
    pub bump_offset: bool,
    pub sort_level: Option<u8>,
    pub unlit: bool,
    pub ambient_from_vertex_color: bool,
    pub diffuse_from_vertex_color: bool,
    pub dcg_lighting_disabled: bool,
    pub dcg_multiplies_result: bool,
    pub modulate2x: Modulate2x,
    pub effect_lit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UvMapperState {
    pub info: VertexMaterialInfo,
    pub args0: MapperArgs,
    pub args1: MapperArgs,
}

#[derive(Debug, Clone, Default)]
pub struct ConvertedModel {
    pub parts: Vec<Part>,
    pub bounds: Aabb,
    pub notes: Vec<String>,
}

impl ConvertedModel {
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.parts.iter().map(|part| part.mesh.vertices.len()).sum()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.mesh.triangle_count())
            .sum()
    }
}

pub fn convert_model(model: &Model<'_>, hierarchy: Option<&Hierarchy>) -> Result<ConvertedModel> {
    let mut out = ConvertedModel::default();
    let bones = hierarchy.map(pivot_world_transforms).unwrap_or_default();

    for mesh in &model.meshes {
        let name = mesh.name().to_owned();
        let skinned = mesh.header.is_skin();
        let (transform, pivot) = if skinned {
            (Mat4::IDENTITY, None)
        } else {
            bone_binding(model, &name, &bones)
        };

        let (mut data, dropped) = convert_mesh(mesh)?;
        let mut unskinned = false;
        let mut skin_source = None;
        if skinned {
            let influences = mesh.influences.to_vec()?;
            if bones.is_empty() || influences.len() != data.vertices.len() {
                unskinned = true;
            } else {
                let second = second_bone_space(mesh, data.vertices.len())?;
                if second.is_empty() && influences.iter().any(|i| i.weights_percent()[1] > 0) {
                    out.notes.push(format!(
                        "{name}: two-bone skin weights but no VERTICES_2, so the second bone \
                         reuses the first bone's copy of each vertex - limbs will be pulled \
                         out of place"
                    ));
                }
                skin_source = Some(SkinSource {
                    bone_space: data.clone(),
                    second_bone_space: second.clone(),
                    influences: influences.clone(),
                });
                skin_in_place(&mut data, &influences, &second, &bones);
            }
        }
        if data.vertices.is_empty() {
            out.notes.push(format!("{name}: no vertices, skipped"));
            continue;
        }
        if dropped > 0 {
            out.notes.push(format!(
                "{name}: dropped {dropped} triangle(s) whose indices point outside the \
                 {}-vertex array",
                data.vertices.len()
            ));
        }
        if mesh.normals.is_empty() {
            out.notes
                .push(format!("{name}: no VERTEX_NORMALS, lit as fully ambient"));
        }
        let uv_count = mesh.texcoords().len();
        if uv_count != data.vertices.len() {
            out.notes.push(format!(
                "{name}: {uv_count} texture coordinate(s) for {} vertices; the shortfall is \
                 filled with (0, 0)",
                data.vertices.len()
            ));
        }
        if skinned && unskinned {
            out.notes.push(format!(
                "{name}: skinned mesh with no usable skeleton, so its vertices are drawn in \
                 bone space - it will look scattered"
            ));
        } else if skinned {
            out.notes.push(format!(
                "{name}: skinned mesh posed at its bind pose; pass --animation to move it"
            ));
        }

        let pick_proxy = mesh.is_pick_proxy();
        if pick_proxy {
            out.notes.push(format!(
                "{name}: a picking proxy, converted but not drawn - see \
                 docs/research/w3d-format.md"
            ));
        }

        let hidden = mesh.header.is_hidden();
        if hidden {
            out.notes.push(format!(
                "{name}: HIDDEN by default, converted but not drawn - see \
                 docs/research/w3d-unused-fields.md finding 2"
            ));
        }

        let mut part_bounds = data.bounds();
        if !part_bounds.is_empty() && !pick_proxy && !hidden {
            part_bounds = Aabb::from_corners(
                transform.transform_point(part_bounds.min()),
                transform.transform_point(part_bounds.max()),
            );
            out.bounds.union(&part_bounds);
        }

        let (material_ambient, material_diffuse, material_emissive, material_opacity, uv_mapper) =
            material_state(mesh);

        let dcg = dcg_sources(
            !mesh.diffuse_colors().is_empty(),
            wants_effect_path(mesh),
            material_ambient,
            material_diffuse,
            material_emissive,
        );
        if let Some(scale) = dcg.premultiply {
            for vertex in &mut data.vertices {
                vertex.color[0] *= scale[0];
                vertex.color[1] *= scale[1];
                vertex.color[2] *= scale[2];
                vertex.color[3] *= material_opacity;
            }
        }

        let (env_map0, env_map1) = match &uv_mapper {
            Some(state) => (
                env_map_for(state.info.stage0_mapper()),
                env_map_for(state.info.stage1_mapper()),
            ),
            None => (None, None),
        };
        if let Some(state) = &uv_mapper {
            for (stage, mapper) in [
                (0, state.info.stage0_mapper()),
                (1, state.info.stage1_mapper()),
            ] {
                if !mapper.is_implemented() {
                    out.notes.push(format!(
                        "{name}: stage {stage} names UV mapper {mapper:?}, which mint does not \
                         animate yet - drawn with its static UVs"
                    ));
                }
            }
        }

        let texture2 = mesh.stage2_texture_name().map(str::to_owned);
        let (detail_combine, detail_alpha_combine) = if texture2.is_some() {
            (mesh_detail_combine(mesh), mesh_detail_alpha_combine(mesh))
        } else {
            (DetailCombine::Disable, DetailAlphaCombine::Disable)
        };
        if let Some(raw) = mesh
            .shaders
            .iter()
            .flatten()
            .next()
            .map(|shader| shader.detail_color_func)
        {
            if texture2.is_some() && raw > MAX_KNOWN_DETAIL_COLOR_FUNC {
                out.notes.push(format!(
                    "{name}: second stage's detail_color_func is {raw}, outside the range \
                     GeneralsMD/Zero Hour's source documents (0-12) - treated as Disable \
                     (see docs/research/w3d-shading.md section 3.2)"
                ));
            }
        }

        let sort_level = mesh_sort_level(mesh);
        let unlit = wants_unlit(mesh);

        let (address_u, address_v) = clamp_addressing(mesh.texture_info());
        let (address2_u, address2_v) = clamp_addressing(mesh.stage2_texture_info());

        let part = Part {
            name,
            mesh: data,
            texture: mesh.texture_name().map(str::to_owned),
            normal_map: mesh.normal_map_name().map(str::to_owned),
            texture2,
            house_color: wants_house_color(mesh),
            detail_combine,
            detail_alpha_combine,
            address2_u,
            address2_v,
            alpha_test: wants_alpha_test(mesh),
            blend: wants_blend(mesh),
            fog_disabled: fog_disabled(mesh),
            transform,
            skinned,
            skin: skin_source,
            pivot,
            pick_proxy,
            hidden,
            cull: !mesh.header.is_two_sided(),
            address_u,
            address_v,
            dropped_triangles: dropped,
            material_ambient,
            material_diffuse,
            material_emissive,
            material_opacity,
            uv_mapper,
            env_map0,
            env_map1,
            bump_offset: wants_bump_offset(mesh),
            sort_level,
            unlit,
            ambient_from_vertex_color: dcg.ambient_from_vertex,
            diffuse_from_vertex_color: dcg.diffuse_from_vertex,
            dcg_lighting_disabled: dcg.lighting_disabled,
            dcg_multiplies_result: dcg.dcg_multiplies_result,
            modulate2x: wants_modulate2x(mesh),
            effect_lit: !mesh.shader_materials.is_empty() || wants_effect_path(mesh),
        };

        match material_split(mesh, part.mesh.vertices.len())? {
            None => out.parts.push(part),
            Some(split) => {
                out.notes.push(format!(
                    "{}: names a texture or shader per triangle; drawn as {} parts",
                    part.name,
                    split.groups.len()
                ));
                push_material_groups(&mut out.parts, &part, mesh, &split);
            }
        }
    }

    Ok(out)
}

struct MaterialGroup {
    stage0: Option<u32>,
    stage1: Option<u32>,
    shader: Option<u32>,
    vertex_material: Option<u32>,
    indices: Vec<u32>,
}

struct MaterialSplit {
    stage0_per_face: bool,
    stage1_per_face: bool,
    shader_per_face: bool,
    vertex_material_per_face: bool,
    groups: Vec<MaterialGroup>,
}

fn material_split(mesh: &Mesh<'_>, vertices: usize) -> Result<Option<MaterialSplit>> {
    let Some(pass) = mesh.passes.first() else {
        return Ok(None);
    };
    fn per_face<'a, 'b>(ids: Option<&'b IndexTable<'a>>) -> Option<&'b Array<'a, u32>> {
        match ids {
            Some(IndexTable::PerElement(ids)) if ids.len() > 1 => Some(ids),
            _ => None,
        }
    }
    let stage0 = per_face(
        pass.texture_stages
            .first()
            .and_then(|stage| stage.texture_ids.as_ref()),
    );
    let stage1 = per_face(
        pass.texture_stages
            .get(1)
            .and_then(|stage| stage.texture_ids.as_ref()),
    );
    let shader = per_face(pass.shader_ids.as_ref());
    let vertex_material = per_face(pass.vertex_material_ids.as_ref());
    if stage0.is_none() && stage1.is_none() && shader.is_none() && vertex_material.is_none() {
        return Ok(None);
    }

    let id_at = |ids: Option<&Array<'_, u32>>, face: usize| {
        ids?.get(face)?.ok().filter(|id| *id != u32::MAX)
    };
    let vm_id_at = |ids: Option<&Array<'_, u32>>, triangle: &Triangle| {
        let ids = ids?;
        let first = *triangle.indices.first()?;
        ids.get(usize::try_from(first).ok()?)?
            .ok()
            .filter(|id| *id != u32::MAX)
    };

    let limit = u32::try_from(vertices).unwrap_or(u32::MAX);
    let mut groups: Vec<MaterialGroup> = Vec::new();
    for (face, triangle) in mesh.triangles.iter().enumerate() {
        let triangle = triangle?;
        if triangle.indices.iter().any(|&i| i >= limit) {
            continue;
        }
        let (id0, id1, ids, vm) = (
            id_at(stage0, face),
            id_at(stage1, face),
            id_at(shader, face),
            vm_id_at(vertex_material, &triangle),
        );
        let group = match groups.iter_mut().find(|group| {
            group.stage0 == id0
                && group.stage1 == id1
                && group.shader == ids
                && group.vertex_material == vm
        }) {
            Some(group) => group,
            None => {
                groups.push(MaterialGroup {
                    stage0: id0,
                    stage1: id1,
                    shader: ids,
                    vertex_material: vm,
                    indices: Vec::new(),
                });
                match groups.last_mut() {
                    Some(group) => group,
                    None => continue,
                }
            }
        };
        group.indices.extend_from_slice(&triangle.indices);
    }

    if groups.len() < 2 {
        return Ok(None);
    }
    Ok(Some(MaterialSplit {
        stage0_per_face: stage0.is_some(),
        stage1_per_face: stage1.is_some(),
        shader_per_face: shader.is_some(),
        vertex_material_per_face: vertex_material.is_some(),
        groups,
    }))
}

fn push_material_groups(
    parts: &mut Vec<Part>,
    part: &Part,
    mesh: &Mesh<'_>,
    split: &MaterialSplit,
) {
    for group in &split.groups {
        let mut drawn = part.clone();
        drawn.mesh.indices.clone_from(&group.indices);
        if split.stage0_per_face {
            let texture = group
                .stage0
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| mesh.textures.get(id));
            drawn.texture = texture.map(|texture| texture.name.clone());
            (drawn.address_u, drawn.address_v) =
                clamp_addressing(texture.and_then(|texture| texture.info.as_ref()));
        }
        if split.stage1_per_face {
            let texture = group
                .stage1
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| mesh.textures.get(id));
            drawn.texture2 = texture.map(|texture| texture.name.clone());
            (drawn.address2_u, drawn.address2_v) =
                clamp_addressing(texture.and_then(|texture| texture.info.as_ref()));
            if drawn.texture2.is_none() {
                drawn.detail_combine = DetailCombine::Disable;
                drawn.detail_alpha_combine = DetailAlphaCombine::Disable;
            }
        }
        if split.shader_per_face {
            if let Some(shader) = group
                .shader
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| mesh.shaders.get(id))
                .and_then(|shader| shader.ok())
            {
                drawn.alpha_test = shader.alpha_test != 0;
                drawn.blend = shader_blend(&shader);
                if drawn.texture2.is_some() {
                    drawn.detail_combine = detail_combine_from_byte(shader.detail_color_func);
                    drawn.detail_alpha_combine =
                        detail_alpha_combine_from_byte(shader.detail_alpha_func);
                }
            }
        }
        if split.vertex_material_per_face {
            if let Some((ambient, diffuse, emissive, opacity, uv_mapper)) = group
                .vertex_material
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| mesh.vertex_materials.get(id))
                .and_then(vertex_material_state)
            {
                drawn.material_ambient = ambient;
                drawn.material_diffuse = diffuse;
                drawn.material_emissive = emissive;
                drawn.material_opacity = opacity;
                drawn.uv_mapper = uv_mapper;
            }
        }
        parts.push(drawn);
    }
}

#[must_use]
pub fn flip_v(uv: Vec2) -> [f32; 2] {
    [uv.u, 1.0 - uv.v]
}

const UV_AREA_EPSILON: f32 = 1e-12;

pub fn convert_mesh(mesh: &Mesh<'_>) -> Result<(MeshData, usize)> {
    let positions = mesh.positions.to_vec()?;
    let normals = mesh.normals.to_vec()?;
    let texcoords = mesh.texcoords().to_vec()?;
    let texcoords2 = mesh.stage2_texcoords().to_vec()?;
    let diffuse_colors = mesh.diffuse_colors().to_vec()?;

    let mut vertices = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        let normal = normals.get(index).copied().unwrap_or_default();
        let uv = texcoords.get(index).copied().map_or([0.0, 0.0], flip_v);
        let uv2 = texcoords2.get(index).copied().map_or([0.0, 0.0], flip_v);
        let color = diffuse_colors
            .get(index)
            .copied()
            .map_or([1.0, 1.0, 1.0, 1.0], |rgba| {
                [unit(rgba.r), unit(rgba.g), unit(rgba.b), unit(rgba.a)]
            });
        vertices.push(
            Vertex::new(
                [position.x, position.y, position.z],
                [normal.x, normal.y, normal.z],
                uv,
            )
            .with_uv2(uv2)
            .with_color(color),
        );
    }

    let limit = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    let mut indices = Vec::with_capacity(mesh.triangle_count().saturating_mul(3));
    let mut dropped = 0usize;
    for triangle in mesh.triangles.iter() {
        let triangle = triangle?;
        if triangle.indices.iter().any(|&i| i >= limit) {
            dropped = dropped.saturating_add(1);
            continue;
        }
        indices.extend_from_slice(&triangle.indices);
    }

    let mut data = MeshData { vertices, indices };
    tangent_basis(mesh, &mut data)?;
    Ok((data, dropped))
}

fn second_bone_space(mesh: &Mesh<'_>, vertices: usize) -> Result<Vec<SecondBoneVertex>> {
    if mesh.positions2.len() != vertices || vertices == 0 {
        return Ok(Vec::new());
    }
    let positions = mesh.positions2.to_vec()?;
    let normals = mesh.normals2.to_vec()?;
    Ok(positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let normal = normals.get(index).copied().unwrap_or_default();
            SecondBoneVertex {
                position: [position.x, position.y, position.z],
                normal: [normal.x, normal.y, normal.z],
            }
        })
        .collect())
}

pub fn tangent_basis(mesh: &Mesh<'_>, data: &mut MeshData) -> Result<()> {
    let tangents = mesh.uv_tangents();
    let bitangents = mesh.uv_bitangents();
    if tangents.len() == data.vertices.len() && bitangents.len() == data.vertices.len() {
        let tangents = tangents.to_vec()?;
        let bitangents = bitangents.to_vec()?;
        for (index, vertex) in data.vertices.iter_mut().enumerate() {
            let (Some(tangent), Some(bitangent)) = (tangents.get(index), bitangents.get(index))
            else {
                continue;
            };
            vertex.tangent = packed_tangent(
                Vec3::new(tangent.x, tangent.y, tangent.z),
                Vec3::new(bitangent.x, bitangent.y, bitangent.z),
                Vec3::from_array(vertex.normal),
            );
        }
        return Ok(());
    }

    if mesh.normal_map_name().is_none() {
        return Ok(());
    }
    derive_tangent_basis(mesh, data)
}

fn packed_tangent(tangent: Vec3, bitangent: Vec3, normal: Vec3) -> [f32; 4] {
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    [tangent.x, tangent.y, tangent.z, handedness]
}

fn derive_tangent_basis(mesh: &Mesh<'_>, data: &mut MeshData) -> Result<()> {
    let mut tangents = vec![Vec3::ZERO; data.vertices.len()];
    let mut bitangents = vec![Vec3::ZERO; data.vertices.len()];

    for triangle in mesh.triangles.iter() {
        let [a, b, c] = triangle?.indices;
        let (Some(a), Some(b), Some(c)) = (
            usize::try_from(a).ok(),
            usize::try_from(b).ok(),
            usize::try_from(c).ok(),
        ) else {
            continue;
        };
        let (Some(va), Some(vb), Some(vc)) = (
            data.vertices.get(a),
            data.vertices.get(b),
            data.vertices.get(c),
        ) else {
            continue;
        };

        let origin = Vec3::from_array(va.position);
        let edge1 = Vec3::from_array(vb.position) - origin;
        let edge2 = Vec3::from_array(vc.position) - origin;
        let (du1, dv1) = (vb.uv[0] - va.uv[0], vb.uv[1] - va.uv[1]);
        let (du2, dv2) = (vc.uv[0] - va.uv[0], vc.uv[1] - va.uv[1]);

        let determinant = du1 * dv2 - du2 * dv1;
        if !determinant.is_finite() || determinant.abs() < UV_AREA_EPSILON {
            continue;
        }
        let scale = 1.0 / determinant;
        let tangent = (edge1 * dv2 - edge2 * dv1) * scale;
        let bitangent = (edge2 * du1 - edge1 * du2) * scale;
        if !tangent.is_finite() || !bitangent.is_finite() {
            continue;
        }

        for index in [a, b, c] {
            if let Some(slot) = tangents.get_mut(index) {
                *slot = *slot + tangent;
            }
            if let Some(slot) = bitangents.get_mut(index) {
                *slot = *slot + bitangent;
            }
        }
    }

    for (index, vertex) in data.vertices.iter_mut().enumerate() {
        let (Some(tangent), Some(bitangent)) = (tangents.get(index), bitangents.get(index)) else {
            continue;
        };
        let normal = Vec3::from_array(vertex.normal);
        let orthogonal = *tangent - normal * normal.dot(*tangent);
        if !orthogonal.is_finite() || orthogonal.length() <= 0.0 {
            continue;
        }
        vertex.tangent = packed_tangent(orthogonal.normalize(), *bitangent, normal);
    }
    Ok(())
}

#[must_use]
fn unit(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DcgSources {
    ambient_from_vertex: bool,
    diffuse_from_vertex: bool,
    lighting_disabled: bool,
    premultiply: Option<[f32; 3]>,
    dcg_multiplies_result: bool,
}

impl DcgSources {
    const NO_DCG: Self = Self {
        ambient_from_vertex: false,
        diffuse_from_vertex: false,
        lighting_disabled: false,
        premultiply: None,
        dcg_multiplies_result: false,
    };

    const EFFECT_DCG: Self = Self {
        ambient_from_vertex: false,
        diffuse_from_vertex: false,
        lighting_disabled: false,
        premultiply: None,
        dcg_multiplies_result: true,
    };
}

#[must_use]
fn dcg_sources(
    has_dcg: bool,
    effect_path: bool,
    ambient: [f32; 3],
    diffuse: [f32; 3],
    emissive: [f32; 3],
) -> DcgSources {
    if !has_dcg {
        return DcgSources::NO_DCG;
    }
    if effect_path {
        return DcgSources::EFFECT_DCG;
    }
    let is_zero = |c: [f32; 3]| c == [0.0, 0.0, 0.0];
    let d = !is_zero(diffuse);
    let a = !is_zero(ambient);
    let e = !is_zero(emissive);
    match (d, a, e) {
        (true, false, false) => DcgSources {
            ambient_from_vertex: false,
            diffuse_from_vertex: true,
            lighting_disabled: false,
            premultiply: Some(diffuse),
            dcg_multiplies_result: false,
        },
        (true, true, false) => DcgSources {
            ambient_from_vertex: true,
            diffuse_from_vertex: true,
            lighting_disabled: false,
            premultiply: Some(diffuse),
            dcg_multiplies_result: false,
        },
        (false, true, false) => DcgSources {
            ambient_from_vertex: true,
            diffuse_from_vertex: false,
            lighting_disabled: false,
            premultiply: Some(ambient),
            dcg_multiplies_result: false,
        },
        (false, false, true) => DcgSources {
            ambient_from_vertex: false,
            diffuse_from_vertex: true,
            lighting_disabled: true,
            premultiply: Some(emissive),
            dcg_multiplies_result: false,
        },
        _ => DcgSources {
            ambient_from_vertex: false,
            diffuse_from_vertex: true,
            lighting_disabled: false,
            premultiply: None,
            dcg_multiplies_result: false,
        },
    }
}

#[must_use]
fn wants_effect_path(mesh: &Mesh<'_>) -> bool {
    if mesh.passes.len() != 1 || mesh.vertex_materials.len() != 1 {
        return false;
    }
    if mesh.stage2_texture_name().is_some() && mesh.texture_name().is_none() {
        return false;
    }
    let Some(shader) = mesh.shaders.iter().flatten().next() else {
        return false;
    };
    let blend_pair_ok = matches!(
        (shader.src_blend, shader.dest_blend),
        (1, 0) | (2, 5) | (1, 1)
    );
    blend_pair_ok
        && shader.detail_alpha_func == 0
        && shader.sec_gradient == 0
        && matches!(shader.pri_gradient, 0 | 1)
        && matches!(shader.detail_color_func, 0 | 2 | 8 | 11 | 12)
        && shader.depth_compare == 3
}

#[must_use]
fn wants_modulate2x(mesh: &Mesh<'_>) -> Modulate2x {
    if wants_effect_path(mesh) || mesh.texture_name().is_none() {
        return Modulate2x::Off;
    }
    match mesh.shaders.iter().flatten().next().map(|s| s.pri_gradient) {
        Some(5) => Modulate2x::On,
        Some(1) => Modulate2x::WithScene,
        _ => Modulate2x::Off,
    }
}

#[must_use]
fn material_state(mesh: &Mesh<'_>) -> ([f32; 3], [f32; 3], [f32; 3], f32, Option<UvMapperState>) {
    mesh.vertex_material()
        .and_then(vertex_material_state)
        .unwrap_or(NEUTRAL_MATERIAL)
}

const NEUTRAL_MATERIAL: ([f32; 3], [f32; 3], [f32; 3], f32, Option<UvMapperState>) =
    ([1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], 1.0, None);

#[must_use]
fn vertex_material_state(
    vm: &VertexMaterial,
) -> Option<([f32; 3], [f32; 3], [f32; 3], f32, Option<UvMapperState>)> {
    let info = vm.info?;

    let ambient = [
        unit(info.ambient.r),
        unit(info.ambient.g),
        unit(info.ambient.b),
    ];
    let diffuse = [
        unit(info.diffuse.r),
        unit(info.diffuse.g),
        unit(info.diffuse.b),
    ];
    let emissive = [
        unit(info.emissive.r),
        unit(info.emissive.g),
        unit(info.emissive.b),
    ];

    let uv_mapper =
        if info.stage0_mapper() == UvMapperType::Uv && info.stage1_mapper() == UvMapperType::Uv {
            None
        } else {
            Some(UvMapperState {
                info,
                args0: vm.mapper_args0_parsed(),
                args1: vm.mapper_args1_parsed(),
            })
        };

    Some((ambient, diffuse, emissive, info.opacity, uv_mapper))
}

#[must_use]
pub fn sample_uv_transforms_for(state: &UvMapperState, seconds: f32) -> SampledUvTransforms {
    let both = mint_sage_asset::sample_material_uv_transforms(
        &state.info,
        &state.args0,
        &state.args1,
        seconds,
    );
    (
        (both.stage0.linear, both.stage0.offset),
        (both.stage1.linear, both.stage1.offset),
        both.bump,
    )
}

pub type SampledUvTransforms = (
    ([[f32; 2]; 2], [f32; 2]),
    ([[f32; 2]; 2], [f32; 2]),
    [[f32; 2]; 2],
);

#[must_use]
fn wants_alpha_test(mesh: &Mesh<'_>) -> bool {
    if mesh
        .drawn_shaders()
        .iter()
        .any(|shader| shader.alpha_test != 0)
    {
        return true;
    }
    mesh.shader_materials.iter().any(|material| {
        matches!(
            material.property("AlphaTestEnable"),
            Some(ShaderMaterialValue::Bool(true))
        )
    })
}

#[must_use]
fn fog_disabled(mesh: &Mesh<'_>) -> bool {
    mesh.shader_materials.iter().any(|material| {
        matches!(
            material.property("FogEnable"),
            Some(ShaderMaterialValue::Bool(false))
        )
    })
}

#[must_use]
fn wants_blend(mesh: &Mesh<'_>) -> Blend {
    let mut best = Blend::Opaque;
    for shader in &mesh.drawn_shaders() {
        let this = shader_blend(shader);
        best = match (best, this) {
            (_, Blend::Additive) | (Blend::Additive, _) => Blend::Additive,
            (_, Blend::AlphaBlend) | (Blend::AlphaBlend, _) => Blend::AlphaBlend,
            (Blend::Opaque, other) => other,
            (kept, _) => kept,
        };
    }
    best
}

fn shader_blend(shader: &Shader) -> Blend {
    match shader.dest_blend {
        0 => return Blend::Opaque,
        1 => return Blend::Additive,
        _ => {}
    }
    if shader.alpha_test != 0 && shader.depth_mask != 0 {
        return Blend::Opaque;
    }
    match (shader.src_blend, shader.dest_blend) {
        (2, 5) => Blend::AlphaBlend,
        (1, 4) => Blend::AddScaledByAlpha,
        (1, 2) => Blend::AddScaledByColor,
        (1, 3) => Blend::AddInverseScaledByColor,
        (0, 2) => Blend::Multiply,
        (src, dest) => {
            warn_unknown_blend_pair_once(src, dest);
            Blend::AlphaBlend
        }
    }
}

fn warn_unknown_blend_pair_once(src: u8, dest: u8) {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(u8, u8)>>> =
        std::sync::OnceLock::new();
    let seen = SEEN.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let is_new = seen
        .lock()
        .map(|mut seen| seen.insert((src, dest)))
        .unwrap_or(true);
    if is_new {
        tracing::warn!(
            src_blend = src,
            dest_blend = dest,
            "SHADERS blend pair outside EA's documented table; falling back to alpha blend"
        );
    }
}

#[must_use]
fn env_map_for(mapper: UvMapperType) -> Option<EnvMap> {
    match mapper {
        UvMapperType::CheapEnvironment => Some(EnvMap::CheapEnvironment),
        UvMapperType::Environment => Some(EnvMap::Environment),
        UvMapperType::WsClassicEnv => Some(EnvMap::WsClassicEnvironment),
        UvMapperType::WsEnvironment => Some(EnvMap::WsEnvironment),
        _ => None,
    }
}

#[must_use]
fn mesh_sort_level(mesh: &Mesh<'_>) -> Option<u8> {
    u8::try_from(mesh.header.sort_level)
        .ok()
        .filter(|&level| (1..=32).contains(&level))
}

#[must_use]
fn wants_unlit(mesh: &Mesh<'_>) -> bool {
    if mesh
        .shader_materials
        .iter()
        .any(|material| material.shader_name.eq_ignore_ascii_case("Simple.fx"))
    {
        return true;
    }
    mesh.shaders
        .iter()
        .flatten()
        .next()
        .is_some_and(|shader| shader.pri_gradient == 0)
}

#[must_use]
fn wants_bump_offset(mesh: &Mesh<'_>) -> bool {
    mesh.shaders
        .iter()
        .flatten()
        .next()
        .is_some_and(|shader| shader.pri_gradient == 3)
}

#[must_use]
fn clamp_addressing(info: Option<&mint_sage_w3d::TextureInfo>) -> (AddressMode, AddressMode) {
    info.map_or((AddressMode::Repeat, AddressMode::Repeat), |info| {
        (
            if info.clamp_u() {
                AddressMode::Clamp
            } else {
                AddressMode::Repeat
            },
            if info.clamp_v() {
                AddressMode::Clamp
            } else {
                AddressMode::Repeat
            },
        )
    })
}

const MAX_KNOWN_DETAIL_COLOR_FUNC: u8 = 12;

#[must_use]
fn wants_house_color(mesh: &Mesh<'_>) -> bool {
    if mesh.normal_map_name().is_some() {
        return false;
    }
    if mesh.shader_materials.is_empty() {
        return mesh.passes.len() == 1;
    }
    mesh.shader_materials
        .iter()
        .any(|material| material.shader_name.eq_ignore_ascii_case("DefaultW3D.fx"))
}

#[must_use]
fn mesh_detail_combine(mesh: &Mesh<'_>) -> DetailCombine {
    mesh.shaders
        .iter()
        .flatten()
        .next()
        .map_or(DetailCombine::Disable, |shader| {
            detail_combine_from_byte(shader.detail_color_func)
        })
}

#[must_use]
fn detail_combine_from_byte(byte: u8) -> DetailCombine {
    match byte {
        0 => DetailCombine::Disable,
        1 => DetailCombine::Detail,
        2 => DetailCombine::Scale,
        3 => DetailCombine::InvScale,
        4 => DetailCombine::Add,
        5 => DetailCombine::Sub,
        6 => DetailCombine::Subr,
        7 => DetailCombine::Blend,
        8 => DetailCombine::DetailBlend,
        9 => DetailCombine::AddSigned,
        10 => DetailCombine::AddSigned2x,
        11 => DetailCombine::Scale2x,
        12 => DetailCombine::ModAlphaAddColor,
        _ => DetailCombine::Disable,
    }
}

#[must_use]
fn mesh_detail_alpha_combine(mesh: &Mesh<'_>) -> DetailAlphaCombine {
    mesh.shaders
        .iter()
        .flatten()
        .next()
        .map_or(DetailAlphaCombine::Disable, |shader| {
            detail_alpha_combine_from_byte(shader.detail_alpha_func)
        })
}

#[must_use]
fn detail_alpha_combine_from_byte(byte: u8) -> DetailAlphaCombine {
    match byte {
        1 => DetailAlphaCombine::Detail,
        2 => DetailAlphaCombine::Scale,
        3 => DetailAlphaCombine::InvScale,
        _ => DetailAlphaCombine::Disable,
    }
}

#[must_use]
pub fn skin_mesh(source: &SkinSource, bones: &[Mat4]) -> MeshData {
    let mut data = source.bone_space.clone();
    skin_in_place(
        &mut data,
        &source.influences,
        &source.second_bone_space,
        bones,
    );
    data
}

#[must_use]
pub fn pose_to_matrices(world: &[PoseTransform]) -> Vec<Mat4> {
    world.iter().map(pose_matrix).collect()
}

#[must_use]
pub fn pose_matrix(transform: &PoseTransform) -> Mat4 {
    let translation = Vec3::new(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    );
    Mat4::from_translation(translation).mul_mat4(rotation_matrix(transform.rotation))
}

#[must_use]
pub fn skin_vertices(source: &SkinSource, bone_count: usize) -> Vec<mint_render::SkinVertex> {
    let vertices = &source.bone_space.vertices;
    if source.influences.len() != vertices.len() {
        return vec![mint_render::SkinVertex::default(); vertices.len()];
    }
    vertices
        .iter()
        .zip(&source.influences)
        .enumerate()
        .map(|(index, (vertex, influence))| {
            let [wa, wb] = influence.weights_fraction();
            let total = wa + wb;
            let (wa, wb) = if total > 0.0 {
                (wa / total, wb / total)
            } else {
                (1.0, 0.0)
            };
            let usable = |bone: u16, weight: f32| {
                if usize::from(bone) < bone_count {
                    weight
                } else {
                    0.0
                }
            };
            let [b0, b1] = influence.bones;
            let (position2, normal2) = source
                .second_bone_space
                .get(index)
                .map_or((vertex.position, vertex.normal), |other| {
                    (other.position, other.normal)
                });
            mint_render::SkinVertex {
                bones: [b0, b1],
                weights: [usable(b0, wa), usable(b1, wb)],
                position2,
                normal2,
            }
        })
        .collect()
}

fn skin_in_place(
    data: &mut MeshData,
    influences: &[VertexInfluence],
    second: &[SecondBoneVertex],
    bones: &[Mat4],
) -> bool {
    if bones.is_empty() || influences.len() != data.vertices.len() {
        return false;
    }

    for (index, (vertex, influence)) in data.vertices.iter_mut().zip(influences.iter()).enumerate()
    {
        let [wa, wb] = influence.weights_fraction();
        let total = wa + wb;
        let (wa, wb) = if total > 0.0 {
            (wa / total, wb / total)
        } else {
            (1.0, 0.0)
        };

        let position = Vec3::from_array(vertex.position);
        let normal = Vec3::from_array(vertex.normal);
        let (position2, normal2) = second.get(index).map_or((position, normal), |other| {
            (
                Vec3::from_array(other.position),
                Vec3::from_array(other.normal),
            )
        });
        let tangent = Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
        let mut moved = Vec3::ZERO;
        let mut turned = Vec3::ZERO;
        let mut swung = Vec3::ZERO;
        let mut applied = 0.0f32;

        for ((bone, weight), (source, source_normal)) in influence
            .bones
            .iter()
            .zip([wa, wb])
            .zip([(position, normal), (position2, normal2)])
        {
            if weight <= 0.0 {
                continue;
            }
            let Some(transform) = bones.get(usize::from(*bone)) else {
                continue;
            };
            let p = transform.transform_point(source);
            let n = transform.transform_vector(source_normal);
            let t = transform.transform_vector(tangent);
            moved = Vec3::new(
                moved.x + p.x * weight,
                moved.y + p.y * weight,
                moved.z + p.z * weight,
            );
            turned = Vec3::new(
                turned.x + n.x * weight,
                turned.y + n.y * weight,
                turned.z + n.z * weight,
            );
            swung = Vec3::new(
                swung.x + t.x * weight,
                swung.y + t.y * weight,
                swung.z + t.z * weight,
            );
            applied += weight;
        }

        if applied > 0.0 {
            vertex.position =
                Vec3::new(moved.x / applied, moved.y / applied, moved.z / applied).to_array();
            vertex.normal = turned.normalize().to_array();
            let swung = swung.normalize();
            vertex.tangent = [swung.x, swung.y, swung.z, vertex.tangent[3]];
        }
    }
    true
}

#[must_use]
pub fn pivot_world_transforms(hierarchy: &Hierarchy) -> Vec<Mat4> {
    let mut world: Vec<Mat4> = Vec::with_capacity(hierarchy.len());
    for (index, pivot) in hierarchy.pivots.iter().enumerate() {
        let local = Mat4::from_translation(Vec3::new(
            pivot.translation.x,
            pivot.translation.y,
            pivot.translation.z,
        ))
        .mul_mat4(rotation_matrix(pivot.rotation));

        let parent = pivot
            .parent
            .and_then(|p| usize::try_from(p).ok())
            .filter(|&p| p < index)
            .and_then(|p| world.get(p).copied())
            .unwrap_or(Mat4::IDENTITY);

        world.push(parent.mul_mat4(local));
    }
    world
}

#[must_use]
fn rotation_matrix(q: Quat) -> Mat4 {
    let length = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if length <= 0.0 || !length.is_finite() {
        return Mat4::IDENTITY;
    }
    let (x, y, z, w) = (q.x / length, q.y / length, q.z / length, q.w / length);

    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);

    Mat4::from_cols([
        [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0],
        [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0],
        [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

fn bone_binding(model: &Model<'_>, mesh_name: &str, bones: &[Mat4]) -> (Mat4, Option<usize>) {
    for hlod in &model.hlods {
        let Some(lod) = hlod.finest_lod() else {
            continue;
        };
        for sub in &lod.sub_objects {
            let leaf = sub.name.rsplit('.').next().unwrap_or(&sub.name);
            if !(sub.name.eq_ignore_ascii_case(mesh_name) || leaf.eq_ignore_ascii_case(mesh_name)) {
                continue;
            }
            let Some(index) = usize::try_from(sub.bone_index).ok() else {
                return (Mat4::IDENTITY, None);
            };
            let index = if bones.is_empty() || index < bones.len() {
                index
            } else {
                0
            };
            let transform = bones.get(index).copied().unwrap_or(Mat4::IDENTITY);
            return (transform, Some(index));
        }
    }
    (Mat4::IDENTITY, None)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests;
