use std::collections::BTreeMap;
use std::sync::Arc;

use mint_render::{
    Aabb, AddressMode, Blend, Instance, Mat4, MeshHandle, Renderer, TextureHandle, Vec3,
};
use mint_sage_asset::Resolver;
use mint_sage_ini::HouseColorTable;
use mint_sage_w3d::Hierarchy;
use mint_vfs::Vfs;

use crate::convert::{Part, SkinSource};
use crate::textures::{
    load_bump_map, load_normal_map, load_texture, missing_texture, LoadedTexture,
};
use crate::{load_model, Result};

pub const ALPHA_CUTOFF: f32 = 96.0 / 255.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartTextures {
    pub texture: Option<TextureHandle>,
    pub normal_map: Option<TextureHandle>,
    pub texture2: Option<TextureHandle>,
}

#[must_use]
pub fn material_alpha(part: &Part) -> f32 {
    if part.blend == Blend::AlphaBlend && !part.diffuse_from_vertex_color {
        part.material_opacity
    } else {
        1.0
    }
}

pub const OBJECT_LIGHT_SET: u8 = 1;

pub const INFANTRY_LIGHT_SET: u8 = 2;

const FIXED_FUNCTION_SET_OFFSET: u8 = 2;

#[must_use]
pub fn part_light_set(rig: u8, effect_lit: bool) -> u8 {
    if effect_lit {
        rig
    } else {
        rig.saturating_add(FIXED_FUNCTION_SET_OFFSET)
    }
}

#[must_use]
pub fn instance_effect_lit(instance: &Instance) -> bool {
    instance.light_set <= INFANTRY_LIGHT_SET
}

#[must_use]
pub fn part_instance(part: &Part, mesh: MeshHandle, textures: PartTextures) -> Instance {
    Instance {
        mesh,
        texture: textures.texture,
        normal_map: textures.normal_map,
        transform: part.transform,
        tint: [1.0, 1.0, 1.0, material_alpha(part)],
        alpha_cutoff: if part.alpha_test { ALPHA_CUTOFF } else { 0.0 },
        blend: part.blend,
        fog: !part.fog_disabled,
        light_set: part_light_set(OBJECT_LIGHT_SET, part.effect_lit),
        cull: part.cull,
        texture2: textures.texture2,
        detail_combine: part.detail_combine,
        detail_alpha_combine: part.detail_alpha_combine,
        material_ambient: part.material_ambient,
        material_diffuse: part.material_diffuse,
        material_emissive: part.material_emissive,
        env_map0: part.env_map0,
        env_map1: part.env_map1,
        bump_offset: part.bump_offset.then_some([[1.0, 0.0], [0.0, 1.0]]),
        sort_level: part.sort_level,
        unlit: part.unlit,
        ambient_from_vertex_color: part.ambient_from_vertex_color,
        diffuse_from_vertex_color: part.diffuse_from_vertex_color,
        dcg_lighting_disabled: part.dcg_lighting_disabled,
        dcg_multiplies_result: part.dcg_multiplies_result,
        modulate2x: part.modulate2x,
        bounds: Some(part.mesh.bounds()),
        ..Instance::new(mesh)
    }
}

#[derive(Debug, Clone)]
pub struct CachedPart {
    pub name: String,
    pub instance: Instance,
    pub skin: Option<SkinSource>,
    pub pivot: Option<usize>,
    pub house_mask: Option<TextureHandle>,
    pub uv_mapper: Option<crate::convert::UvMapperState>,
    pub texture_name: Option<String>,
    pub texture_address: (AddressMode, AddressMode),
    pub hidden: bool,
}

#[must_use]
pub fn texture_stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

#[derive(Debug, Clone)]
pub struct CachedModel {
    pub name: String,
    pub hierarchy: Option<Hierarchy>,
    pub parts: Vec<CachedPart>,
    pub bounds: Aabb,
    pub vertices: usize,
    pub triangles: usize,
}

impl CachedModel {
    #[must_use]
    pub fn pivot_named(&self, bone: &str) -> Option<usize> {
        self.hierarchy.as_ref()?.pivots.iter().position(|pivot| {
            pivot
                .name
                .trim_end_matches(char::from(0))
                .eq_ignore_ascii_case(bone)
        })
    }

    #[must_use]
    pub fn bind_bone(&self, pivot: usize) -> Option<Mat4> {
        let hierarchy = self.hierarchy.as_ref()?;
        let pose = mint_sage_asset::SampledPose::bind(hierarchy);
        crate::convert::pose_to_matrices(&pose.world)
            .get(pivot)
            .copied()
    }

    pub fn place(&self, placement: Mat4, out: &mut Vec<Instance>) {
        self.place_many(&[placement], out);
    }

    pub fn place_many(&self, placements: &[Mat4], out: &mut Vec<Instance>) {
        out.reserve(self.parts.len().saturating_mul(placements.len()));
        for part in &self.parts {
            for placement in placements {
                out.push(Instance {
                    transform: placed_transform(*placement, part.instance.transform),
                    ..part.instance
                });
            }
        }
    }

    #[must_use]
    pub fn has_uv_mappers(&self) -> bool {
        self.parts.iter().any(|part| part.uv_mapper.is_some())
    }

    pub fn apply_uv_mappers(&self, placements: usize, out: &mut [Instance], seconds: f32) -> bool {
        let mut wrote = false;
        for (index, part) in self.parts.iter().enumerate() {
            let Some(state) = &part.uv_mapper else {
                continue;
            };
            let ((uv_linear, uv_offset), (uv2_linear, uv2_offset), bump) =
                crate::convert::sample_uv_transforms_for(state, seconds);
            let start = index.saturating_mul(placements);
            let Some(slice) = out.get_mut(start..start.saturating_add(placements)) else {
                continue;
            };
            for instance in slice {
                instance.uv_linear = uv_linear;
                instance.uv_offset = uv_offset;
                instance.uv2_linear = uv2_linear;
                instance.uv2_offset = uv2_offset;
                if instance.bump_offset.is_some() {
                    instance.bump_offset = Some(bump);
                }
            }
            wrote = true;
        }
        wrote
    }
}

#[must_use]
pub fn placed_transform(placement: Mat4, part: Mat4) -> Mat4 {
    placement.mul_mat4(part)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelCacheStats {
    pub requests: usize,
    pub loaded: usize,
    pub failed: usize,
    pub meshes: usize,
    pub textures: usize,
    pub skipped_pick: usize,
    pub skipped_hidden: usize,
}

#[derive(Debug, Default)]
pub struct ModelCache {
    models: BTreeMap<String, Option<Arc<CachedModel>>>,
    textures: BTreeMap<(String, bool, bool), Option<TextureHandle>>,
    bump_maps: BTreeMap<(String, bool, bool), Option<TextureHandle>>,
    stats: ModelCacheStats,
    failures: Vec<(String, String)>,
    house_colors: Option<Arc<HouseColorTable>>,
    ahead: BTreeMap<String, Result<LoadedTexture>>,
}

impl ModelCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_house_colors(&mut self, table: Arc<HouseColorTable>) {
        self.house_colors = Some(table);
    }

    #[must_use]
    pub fn house_color_entries(&self) -> usize {
        self.house_colors.as_ref().map_or(0, |table| table.len())
    }

    pub fn get_or_load(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
    ) -> Option<Arc<CachedModel>> {
        self.stats.requests = self.stats.requests.saturating_add(1);
        let key = name.to_ascii_lowercase();
        if let Some(entry) = self.models.get(&key) {
            return entry.clone();
        }
        let entry = match self.load(renderer, vfs, resolver, name, false) {
            Ok(model) => {
                self.stats.loaded = self.stats.loaded.saturating_add(1);
                Some(Arc::new(model))
            }
            Err(error) => {
                self.stats.failed = self.stats.failed.saturating_add(1);
                self.failures.push((name.to_owned(), reason(&error)));
                None
            }
        };
        self.models.insert(key, entry.clone());
        entry
    }

    pub fn load_uncached(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
    ) -> Option<Arc<CachedModel>> {
        self.stats.requests = self.stats.requests.saturating_add(1);
        match self.load(renderer, vfs, resolver, name, false) {
            Ok(model) => {
                self.stats.loaded = self.stats.loaded.saturating_add(1);
                Some(Arc::new(model))
            }
            Err(error) => {
                self.stats.failed = self.stats.failed.saturating_add(1);
                self.failures.push((name.to_owned(), reason(&error)));
                None
            }
        }
    }

    pub fn load_with_hidden(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
    ) -> Result<CachedModel> {
        self.stats.requests = self.stats.requests.saturating_add(1);
        self.load(renderer, vfs, resolver, name, true)
    }

    fn load(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        keep_hidden: bool,
    ) -> Result<CachedModel> {
        let loaded = load_model(vfs, resolver, name)?;
        let compressed_ok = renderer.supports_block_compression();
        self.decode_ahead(vfs, resolver, &loaded.model.parts, compressed_ok);
        let mut parts = Vec::with_capacity(loaded.model.parts.len());
        let mut bounds = Aabb::EMPTY;
        let (mut vertices, mut triangles) = (0usize, 0usize);

        for part in &loaded.model.parts {
            if part.pick_proxy {
                self.stats.skipped_pick = self.stats.skipped_pick.saturating_add(1);
                continue;
            }
            if part.hidden && !keep_hidden {
                self.stats.skipped_hidden = self.stats.skipped_hidden.saturating_add(1);
                continue;
            }
            let mesh = renderer.upload_mesh(&part.mesh)?;
            self.stats.meshes = self.stats.meshes.saturating_add(1);
            vertices = vertices.saturating_add(part.mesh.vertices.len());
            triangles = triangles.saturating_add(part.mesh.triangle_count());
            let part_bounds = part.mesh.bounds();
            if !part_bounds.is_empty() {
                bounds.union(&transformed_bounds(&part_bounds, part.transform));
            }

            let texture = if part.bump_offset {
                self.bump_for(
                    renderer,
                    vfs,
                    resolver,
                    part.texture.as_deref(),
                    (part.address_u, part.address_v),
                )
            } else {
                self.diffuse_for(
                    renderer,
                    vfs,
                    resolver,
                    part.texture.as_deref(),
                    compressed_ok,
                    (part.address_u, part.address_v),
                )
            };
            let normal_map = self.normal_for(
                renderer,
                vfs,
                resolver,
                part.normal_map.as_deref(),
                compressed_ok,
            );
            let texture2 = self.diffuse_for(
                renderer,
                vfs,
                resolver,
                part.texture2.as_deref(),
                compressed_ok,
                (part.address2_u, part.address2_v),
            );
            let house_mask = self.house_color_mask(part).and_then(|name| {
                self.diffuse_for(
                    renderer,
                    vfs,
                    resolver,
                    Some(&name),
                    compressed_ok,
                    (part.address_u, part.address_v),
                )
            });
            let instance = part_instance(
                part,
                mesh,
                PartTextures {
                    texture,
                    normal_map,
                    texture2,
                },
            );
            parts.push(CachedPart {
                name: part.name.clone(),
                instance,
                skin: part.skin.clone(),
                pivot: part.pivot,
                uv_mapper: part.uv_mapper.clone(),
                house_mask,
                texture_name: part.texture.clone(),
                texture_address: (part.address_u, part.address_v),
                hidden: part.hidden,
            });
        }

        self.ahead.clear();
        Ok(CachedModel {
            name: name.to_owned(),
            hierarchy: loaded.hierarchy.clone(),
            parts,
            bounds,
            vertices,
            triangles,
        })
    }

    fn decode_ahead(
        &mut self,
        vfs: &Vfs,
        resolver: &Resolver,
        parts: &[Part],
        compressed_ok: bool,
    ) {
        self.ahead.clear();
        let mut names: Vec<String> = Vec::new();
        for part in parts.iter().filter(|part| !part.pick_proxy && !part.hidden) {
            let stage0 = part.texture.clone().filter(|_| !part.bump_offset);
            let wanted = [
                stage0.map(|name| (name, (part.address_u, part.address_v))),
                part.texture2
                    .clone()
                    .map(|name| (name, (part.address2_u, part.address2_v))),
                self.house_color_mask(part)
                    .map(|name| (name, (part.address_u, part.address_v))),
            ];
            for (name, address) in wanted.into_iter().flatten() {
                let key = (
                    name.to_ascii_lowercase(),
                    address.0 == AddressMode::Clamp,
                    address.1 == AddressMode::Clamp,
                );
                if !self.textures.contains_key(&key) && !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        let decoded = mint_pool::map_ordered(&names, |name| {
            load_texture(vfs, resolver, name, compressed_ok)
        });
        self.ahead = names.into_iter().zip(decoded).collect();
    }

    fn house_color_mask(&self, part: &Part) -> Option<String> {
        if !part.house_color {
            return None;
        }
        let table = self.house_colors.as_ref()?;
        table.mask_for(part.texture.as_deref()?).map(str::to_owned)
    }

    fn diffuse_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: Option<&str>,
        compressed_ok: bool,
        address: (AddressMode, AddressMode),
    ) -> Option<TextureHandle> {
        let name = name?;
        let key = (
            name.to_ascii_lowercase(),
            address.0 == AddressMode::Clamp,
            address.1 == AddressMode::Clamp,
        );
        if let Some(entry) = self.textures.get(&key) {
            return *entry;
        }
        let data = self
            .ahead
            .remove(name)
            .unwrap_or_else(|| load_texture(vfs, resolver, name, compressed_ok))
            .map_or_else(|_| missing_texture(), |loaded| loaded.data)
            .with_addressing(address.0, address.1);
        let handle = renderer.upload_texture(&data).ok();
        if handle.is_some() {
            self.stats.textures = self.stats.textures.saturating_add(1);
        }
        self.textures.insert(key, handle);
        handle
    }

    fn bump_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: Option<&str>,
        address: (AddressMode, AddressMode),
    ) -> Option<TextureHandle> {
        let name = name?;
        let key = (
            name.to_ascii_lowercase(),
            address.0 == AddressMode::Clamp,
            address.1 == AddressMode::Clamp,
        );
        if let Some(entry) = self.bump_maps.get(&key) {
            return *entry;
        }
        let handle = load_bump_map(vfs, resolver, name).ok().and_then(|map| {
            renderer
                .upload_texture(&map.data.with_addressing(address.0, address.1))
                .ok()
        });
        if handle.is_some() {
            self.stats.textures = self.stats.textures.saturating_add(1);
        }
        self.bump_maps.insert(key, handle);
        handle
    }

    fn normal_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: Option<&str>,
        compressed_ok: bool,
    ) -> Option<TextureHandle> {
        let name = name?;
        let key = (name.to_ascii_lowercase(), false, false);
        if let Some(entry) = self.textures.get(&key) {
            return *entry;
        }
        let handle = load_normal_map(vfs, resolver, name, compressed_ok)
            .ok()
            .and_then(|map| renderer.upload_texture(&map.data).ok());
        if handle.is_some() {
            self.stats.textures = self.stats.textures.saturating_add(1);
        }
        self.textures.insert(key, handle);
        handle
    }

    pub fn substitute(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        model: &CachedModel,
        swaps: &[(String, String)],
    ) -> Vec<Option<TextureHandle>> {
        let compressed_ok = renderer.supports_block_compression();
        model
            .parts
            .iter()
            .map(|part| {
                let own = part.texture_name.as_deref()?;
                let (_, to) = swaps
                    .iter()
                    .find(|(from, _)| texture_stem(from).eq_ignore_ascii_case(texture_stem(own)))?;
                self.diffuse_for(
                    renderer,
                    vfs,
                    resolver,
                    Some(to),
                    compressed_ok,
                    part.texture_address,
                )
            })
            .collect()
    }

    #[must_use]
    pub fn stats(&self) -> ModelCacheStats {
        self.stats
    }

    #[must_use]
    pub fn failures(&self) -> &[(String, String)] {
        &self.failures
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

fn reason(error: &crate::Error) -> String {
    let mut out = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(next) = source {
        out.push_str(": ");
        out.push_str(&next.to_string());
        source = next.source();
    }
    out
}

fn transformed_bounds(bounds: &Aabb, transform: Mat4) -> Aabb {
    let (min, max) = (bounds.min(), bounds.max());
    let mut out = Aabb::EMPTY;
    for index in 0..8u8 {
        let pick = |bit: u8, low: f32, high: f32| if index & bit == 0 { low } else { high };
        out.insert(transform.transform_point(Vec3::new(
            pick(1, min.x, max.x),
            pick(2, min.y, max.y),
            pick(4, min.z, max.z),
        )));
    }
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use mint_render::{DetailAlphaCombine, DetailCombine, MeshData};

    #[test]
    fn a_texture_substitution_matches_on_the_stem() {
        assert_eq!(texture_stem("GBFortress1.tga"), "GBFortress1");
        assert_eq!(
            texture_stem("art\\compiledtextures\\gb\\gbfortress1.dds"),
            "gbfortress1"
        );
        assert_eq!(texture_stem("a/b/NoExtension"), "NoExtension");
        assert!(
            texture_stem("GBFortress1.TGA").eq_ignore_ascii_case(texture_stem("gbfortress1.dds"))
        );
        assert_ne!(
            texture_stem("GBFortress1_Snow.tga"),
            texture_stem("GBFortress1.tga")
        );
    }

    fn skeleton() -> Hierarchy {
        use mint_sage_w3d::{HierarchyHeader, Pivot, Quat, Vec3 as W3dVec3};
        let pivot = |name: &str, parent, z| Pivot {
            name: format!("{name}\0\0"),
            parent,
            translation: W3dVec3 { x: 0.0, y: 0.0, z },
            euler_angles: W3dVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: Quat {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
        };
        Hierarchy {
            header: HierarchyHeader {
                name: "TEST".to_owned(),
                num_pivots: 2,
                center: W3dVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                version: 0,
            },
            pivots: vec![
                pivot("ROOTTRANSFORM", None, 3.0),
                pivot("FXBone01", Some(0), 10.0),
            ],
        }
    }

    fn model_with_skeleton() -> CachedModel {
        CachedModel {
            name: "test".to_owned(),
            hierarchy: Some(skeleton()),
            parts: Vec::new(),
            bounds: Aabb::EMPTY,
            vertices: 0,
            triangles: 0,
        }
    }

    #[test]
    fn a_bone_is_found_by_name_whatever_the_casing() {
        let model = model_with_skeleton();
        assert_eq!(model.pivot_named("FXBone01"), Some(1));
        assert_eq!(model.pivot_named("fxbone01"), Some(1), "casing is not data");
        assert_eq!(model.pivot_named("ROOTTRANSFORM"), Some(0));
        assert_eq!(model.pivot_named("NoSuchBone"), None);
        assert_eq!(
            CachedModel {
                hierarchy: None,
                ..model_with_skeleton()
            }
            .pivot_named("FXBone01"),
            None,
            "a model with no skeleton has no bones to find"
        );
    }

    #[test]
    fn the_bind_bone_is_the_pivots_world_transform() {
        let model = model_with_skeleton();
        let root = model.bind_bone(0).expect("a root");
        let child = model.bind_bone(1).expect("a child");
        assert_eq!(root.transform_point(Vec3::ZERO).to_array(), [0.0, 0.0, 3.0]);
        assert_eq!(
            child.transform_point(Vec3::ZERO).to_array(),
            [0.0, 0.0, 13.0]
        );
        assert!(model.bind_bone(2).is_none(), "past the end is not a bone");
    }

    fn part(blend: Blend, opacity: f32) -> Part {
        Part {
            blend,
            material_opacity: opacity,
            mesh: MeshData::new(),
            detail_combine: DetailCombine::Disable,
            detail_alpha_combine: DetailAlphaCombine::Disable,
            ..Part::default()
        }
    }

    #[test]
    fn only_an_alpha_blended_material_takes_its_own_opacity() {
        assert!((material_alpha(&part(Blend::AlphaBlend, 0.25)) - 0.25).abs() < 1e-6);
        assert!((material_alpha(&part(Blend::Additive, 0.0)) - 1.0).abs() < 1e-6);
        assert!((material_alpha(&part(Blend::Opaque, 0.0)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn placing_a_part_composes_the_placement_over_the_parts_own_transform() {
        let quarter = Mat4::from_cols([
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 0.0, 0.0, 1.0],
        ]);
        let part = Mat4::from_translation(Vec3::new(2.0, 0.0, 5.0));
        let at = placed_transform(quarter, part).transform_point(Vec3::ZERO);
        assert!((at.x - 10.0).abs() < 1e-5, "got {at:?}");
        assert!((at.y - 2.0).abs() < 1e-5, "got {at:?}");
        assert!((at.z - 5.0).abs() < 1e-5, "got {at:?}");
    }

    #[test]
    fn transformed_bounds_contain_every_rotated_corner() {
        let unit = Aabb::from_corners(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let c = core::f32::consts::FRAC_1_SQRT_2;
        let turn = Mat4::from_cols([
            [c, c, 0.0, 0.0],
            [-c, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let out = transformed_bounds(&unit, turn);
        let expected = core::f32::consts::SQRT_2;
        assert!((out.max().x - expected).abs() < 1e-5, "{:?}", out.max());
        assert!((out.min().y + expected).abs() < 1e-5, "{:?}", out.min());
    }
}
