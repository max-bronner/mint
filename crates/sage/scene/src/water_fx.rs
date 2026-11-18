use std::collections::BTreeMap;

use mint_render::{
    Aabb, Blend, Camera, CustomMaterialHandle, CustomShader, CustomShaderHandle, DepthTest,
    Instance, Mat4, MeshData, MeshHandle, Renderer, TextureData, TextureHandle, Vertex,
};
use mint_sage_asset::Resolver;
use mint_sage_fx::effect::{op, Resource, State};
use mint_sage_fx::shader::Shader;
use mint_sage_fx::wgsl::{self, Translation};
use mint_sage_fx::{Effect, Preshader};
use mint_sage_map::{HeightMapData, StandingWaterArea, MAP_XY_FACTOR};
use mint_sage_terrain::{RiverNetwork, RiverRibbon, WaterSurface};
use mint_vfs::{Vfs, VfsPath};

use crate::TerrainRig;

const EFFECT_DIR: &str = "shaders/compiled/";

pub type Params = BTreeMap<String, Vec<f32>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FxDetail {
    #[default]
    Ultra,
    High,
    Medium,
    Low,
}

impl FxDetail {
    pub const ALL: [Self; 4] = [Self::Ultra, Self::High, Self::Medium, Self::Low];

    fn bit(self) -> u8 {
        1 << self as u8
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Ultra => "_U",
            Self::High => "",
            Self::Medium => "_M",
            Self::Low => "_L",
        }
    }

    fn material_variant(self, name: &str) -> Option<String> {
        let letter = match self {
            Self::Medium => "M",
            Self::Low => "L",
            _ => return None,
        };
        let (stem, ext) = name.rsplit_once('.')?;
        Some(format!("{stem}{letter}.{ext}"))
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Ultra => Self::High,
            Self::High => Self::Medium,
            Self::Medium => Self::Low,
            Self::Low => Self::Ultra,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ultra => "ultra",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

fn vertex_input(usage: u8, index: u8) -> Option<String> {
    Some(match (usage, index) {
        (0, 0) => "vec4<f32>(vin.position, 1.0)".to_owned(),
        (3, 0) => "vec4<f32>(vin.normal, 1.0)".to_owned(),
        (5, 0) => "vec4<f32>(vin.uv, 0.0, 1.0)".to_owned(),
        (5, 1) => "vec4<f32>(vin.uv2, 0.0, 1.0)".to_owned(),
        (6, 0) => "vin.tangent".to_owned(),
        (10, 0) => "vin.color".to_owned(),
        _ => return None,
    })
}

#[derive(Debug, Clone)]
enum StateValue {
    Constant(u32),
    Preshader(Preshader),
}

pub struct FxProgram {
    pub label: String,
    effect: Effect,
    vs: Shader,
    ps: Shader,
    translation: Translation,
    pub shader: CustomShaderHandle,
    pub slot_textures: Vec<String>,
    src_blend: u32,
    dest_blend: StateValue,
    pub writes_depth: bool,
}

fn state_u32(state: &State) -> u32 {
    state.value.words.first().copied().unwrap_or(0)
}

impl FxProgram {
    pub fn build(
        renderer: &mut Renderer,
        effect: &Effect,
        file: &str,
        technique: &str,
        lookup: &dyn Fn(&str) -> Option<Vec<f32>>,
    ) -> Result<Self, String> {
        let pass = effect
            .technique(technique)
            .and_then(|t| t.passes.first())
            .ok_or_else(|| format!("{file}: no technique {technique}"))?;
        let mut vs_blob = None;
        let mut ps_blob = None;
        let mut src_blend = 5;
        let mut dest_blend = StateValue::Constant(6);
        let mut writes_depth = false;
        let mut writes_alpha = true;
        for state in &pass.states {
            match state.operation {
                op::VERTEXSHADER | op::PIXELSHADER => {
                    let element = match &state.resource {
                        Some(Resource::ArraySelector { preshader, .. }) => {
                            select_element(preshader, lookup)?
                        }
                        _ => 0,
                    };
                    let blob = effect.state_shader(state, element).ok_or_else(|| {
                        format!("{file} {technique}: shader state has no bytecode")
                    })?;
                    if state.operation == op::VERTEXSHADER {
                        vs_blob = Some(blob);
                    } else {
                        ps_blob = Some(blob);
                    }
                }
                op::SRCBLEND => src_blend = state_u32(state),
                op::DESTBLEND => {
                    dest_blend = match &state.resource {
                        Some(Resource::Data(blob)) => StateValue::Preshader(
                            Preshader::parse(blob).map_err(|e| format!("{file} DESTBLEND: {e}"))?,
                        ),
                        _ => StateValue::Constant(state_u32(state)),
                    };
                }
                op::ZWRITEENABLE => writes_depth = state_u32(state) != 0,
                op::COLORWRITEENABLE => writes_alpha = state_u32(state) & 8 != 0,
                _ => {}
            }
        }
        let vs =
            Shader::parse(vs_blob.ok_or_else(|| format!("{file} {technique}: no vertex shader"))?)
                .map_err(|e| format!("{file} {technique} vs: {e}"))?;
        let ps =
            Shader::parse(ps_blob.ok_or_else(|| format!("{file} {technique}: no pixel shader"))?)
                .map_err(|e| format!("{file} {technique} ps: {e}"))?;
        let translation = wgsl::translate(
            &vs,
            &ps,
            &wgsl::Target {
                vertex_type: "MintVertex",
                vertex_input: &vertex_input,
                group: 1,
                uniform_binding: 0,
                first_texture_binding: 1,
            },
        )
        .map_err(|e| {
            let name = match &e {
                mint_sage_fx::Error::Unsupported {
                    what: "shader opcode",
                    value,
                    ..
                } => mint_sage_fx::shader::opcode_name(*value),
                _ => "",
            };
            format!("{file} {technique}: {e} {name}")
        })?;
        let mut slot_textures = vec![String::new(); translation.texture_slots as usize];
        for constant in ps.constants.iter().filter(|c| c.register_set == 3) {
            let texture = effect
                .parameter(&constant.name)
                .and_then(|p| p.samplers.first())
                .and_then(|states| states.iter().find(|s| s.operation == op::TEXTURE))
                .and_then(|s| match &s.resource {
                    Some(Resource::Parameter(name)) => Some(name.clone()),
                    _ => None,
                });
            if let (Some(slot), Some(texture)) = (
                slot_textures.get_mut(usize::from(constant.register_index)),
                texture,
            ) {
                *slot = texture;
            }
        }
        let label = format!("{file} {technique}");
        let shader = renderer
            .register_custom_shader(&CustomShader {
                label: &label,
                source: &translation.source,
                texture_slots: translation.texture_slots,
                uniform_size: translation.uniform_size(),
                writes_alpha,
                ..CustomShader::default()
            })
            .map_err(|e| format!("{label}: {e}"))?;
        Ok(Self {
            label,
            effect: effect.clone(),
            vs,
            ps,
            translation,
            shader,
            slot_textures,
            src_blend,
            dest_blend,
            writes_depth,
        })
    }

    fn default_value(&self, name: &str) -> Option<Vec<f32>> {
        self.effect
            .parameter(name)
            .map(mint_sage_fx::Parameter::floats)
    }

    fn blend(&self, lookup: &dyn Fn(&str) -> Option<Vec<f32>>) -> Blend {
        let dest = match &self.dest_blend {
            StateValue::Constant(v) => *v,
            StateValue::Preshader(pre) => {
                let mut out = [0.0f32; 4];
                let inputs = preshader_inputs(pre, lookup);
                match pre.run(&inputs, &mut out) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    Ok(()) => out.first().copied().unwrap_or(0.0).round() as u32,
                    Err(_) => 6,
                }
            }
        };
        match (self.src_blend, dest) {
            (5, 2) => Blend::AlphaAdditive,
            (_, 2) => Blend::Additive,
            _ if self.writes_depth => Blend::AlphaBlendWritesDepth,
            _ => Blend::AlphaBlend,
        }
    }

    fn block(&self, lookup: &dyn Fn(&str) -> Option<Vec<f32>>, width: u32, height: u32) -> Vec<u8> {
        let vs = registers(&self.vs, self.translation.vs_registers, lookup);
        let ps = registers(&self.ps, self.translation.ps_registers, lookup);
        let mut vsb = vec![0.0f32; self.translation.vs_bools as usize];
        let mut psb = vec![0.0f32; self.translation.ps_bools as usize];
        mint_sage_fx::constants::fill_bools(&self.vs.constants, lookup, &mut vsb);
        mint_sage_fx::constants::fill_bools(&self.ps.constants, lookup, &mut psb);
        self.translation.block(&vs, &ps, &vsb, &psb, width, height)
    }
}

fn select_element(blob: &[u8], lookup: &dyn Fn(&str) -> Option<Vec<f32>>) -> Result<usize, String> {
    let pre = Preshader::parse(blob).map_err(|e| format!("array selector: {e}"))?;
    let mut out = [0.0f32; 4];
    pre.run(&preshader_inputs(&pre, lookup), &mut out)
        .map_err(|e| format!("array selector: {e}"))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = out.first().copied().unwrap_or(0.0).max(0.0) as usize;
    Ok(index)
}

fn preshader_inputs(pre: &Preshader, lookup: &dyn Fn(&str) -> Option<Vec<f32>>) -> Vec<f64> {
    let size = pre
        .constants
        .iter()
        .map(|c| (usize::from(c.register_index) + usize::from(c.register_count)) * 4)
        .max()
        .unwrap_or(0);
    let mut table = vec![0.0f32; size];
    mint_sage_fx::constants::fill(&pre.constants, lookup, &mut table);
    table.into_iter().map(f64::from).collect()
}

fn registers(shader: &Shader, count: u32, lookup: &dyn Fn(&str) -> Option<Vec<f32>>) -> Vec<f32> {
    let mut out = vec![0.0f32; count as usize * 4];
    mint_sage_fx::constants::fill(&shader.constants, lookup, &mut out);
    if let Some(pre) = &shader.preshader {
        if let Err(error) = pre.run(&preshader_inputs(pre, lookup), &mut out) {
            eprintln!("  fx preshader: {error}");
        }
    }
    out
}

pub struct FxSurface {
    pub program: usize,
    pub mesh: MeshHandle,
    pub material: Option<CustomMaterialHandle>,
    pub params: Params,
    pub textures: Vec<Option<TextureHandle>>,
    river: Option<RiverStrip>,
    pca: Option<PcaBasis>,
    details: u8,
    shrouded: Option<CustomMaterialHandle>,
    material_name: String,
    reach: Option<Vec<Aabb>>,
}

pub struct PcaBasis {
    frames: usize,
    speed: f32,
    rows: Vec<Vec<f32>>,
}

impl PcaBasis {
    fn parse(text: &str) -> Option<Self> {
        let mut frames = 0usize;
        let mut speed = 1.0f32;
        let mut named: BTreeMap<String, Vec<f32>> = BTreeMap::new();
        for line in text.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            match key.as_str() {
                "numframes" => frames = rest.trim().parse().ok()?,
                "speed" => speed = rest.trim().parse().ok()?,
                _ => {
                    let values: Option<Vec<f32>> =
                        rest.split_whitespace().map(|t| t.parse().ok()).collect();
                    named.insert(key, values?);
                }
            }
        }
        if frames == 0 {
            return None;
        }
        let mut rows = Vec::with_capacity(13);
        for name in std::iter::once("mean".to_owned()).chain((0..12).map(|k| format!("basis{k}"))) {
            let row = named.remove(&name)?;
            if row.len() < frames.checked_mul(3)? {
                return None;
            }
            rows.push(row);
        }
        Some(Self {
            frames,
            speed,
            rows,
        })
    }

    fn at(&self, seconds: f32, rate: PcaRate) -> Vec<f32> {
        let position =
            (seconds * rate.frames_per_second(self.speed)).rem_euclid(self.frames as f32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let f0 = (position as usize).min(self.frames.saturating_sub(1));
        let f1 = (f0.saturating_add(1)) % self.frames;
        let t = position - f0 as f32;
        let component = |row: usize, axis: usize| -> f32 {
            let block = [0usize, 2, 1].get(axis).copied().unwrap_or(0);
            let Some(values) = self.rows.get(row) else {
                return 0.0;
            };
            let a = values.get(block * self.frames + f0).copied().unwrap_or(0.0);
            let b = values.get(block * self.frames + f1).copied().unwrap_or(0.0);
            (a + (b - a) * t) / 255.0
        };
        let mut out = Vec::with_capacity(39);
        for axis in 0..3 {
            out.push(component(0, axis));
        }
        for group in 0..3 {
            for axis in 0..3 {
                for k in 0..4 {
                    out.push(component(1 + group * 4 + k, axis));
                }
            }
        }
        out
    }
}

struct RiverStrip {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    v_scale: f32,
    scroll: f32,
}

#[derive(Default)]
pub struct FxWater {
    pub programs: Vec<FxProgram>,
    pub surfaces: Vec<FxSurface>,
    pub report: Vec<String>,
    failed: std::collections::BTreeSet<String>,
    white: Option<TextureHandle>,
    black: Option<TextureHandle>,
    pub detail: FxDetail,
    shroud_uv: [f32; 4],
    pca_waves: Vec<PcaWave>,
    standing_meshes: Vec<(MeshHandle, MeshData)>,
    pub gate: PcaGate,
    pub gate_wanted: PcaGate,
    pub pca_rate: PcaRate,
    pub fog: Option<mint_render::DistanceFog>,
    view_proj: Option<Mat4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PcaRate {
    #[default]
    PerLogicFrame,
    PerSecond,
}

impl PcaRate {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::PerLogicFrame => Self::PerSecond,
            Self::PerSecond => Self::PerLogicFrame,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::PerLogicFrame => "per-logic-frame",
            Self::PerSecond => "per-second",
        }
    }

    fn frames_per_second(self, speed: f32) -> f32 {
        match self {
            Self::PerLogicFrame => 30.0 * speed,
            Self::PerSecond => speed,
        }
    }
}

fn read_effect(vfs: &Vfs, name: &str) -> Result<Effect, String> {
    let bytes = vfs
        .read_path(&VfsPath::new(&format!("{EFFECT_DIR}{name}")))
        .map_err(|e| format!("{name}: {e}"))?;
    Effect::parse(&bytes).map_err(|e| format!("{name}: {e}"))
}

fn material_params(vfs: &Vfs, resolver: &Resolver, name: &str) -> (Params, Option<u32>) {
    let mut params = Params::new();
    let Some(bytes) = resolver
        .resolve(vfs, name)
        .ok()
        .and_then(|path| vfs.read_path(&path).ok())
    else {
        return (params, None);
    };
    let Ok(model) = mint_sage_w3d::Model::parse(&bytes) else {
        return (params, None);
    };
    let mut technique = None;
    for material in model.meshes.iter().flat_map(|m| m.shader_materials.iter()) {
        technique.get_or_insert(material.unknown);
        for property in &material.properties {
            use mint_sage_w3d::ShaderMaterialValue as V;
            let value = match &property.value {
                V::Float(v) => vec![*v],
                V::Vec2(v) => v.to_vec(),
                V::Vec3(v) => v.to_vec(),
                V::Vec4(v) => v.to_vec(),
                V::Int(v) => vec![*v as f32],
                V::Bool(v) => vec![f32::from(u8::from(*v))],
                V::Unknown { .. } => continue,
                V::Texture(t) => {
                    params
                        .entry(format!("{}#texture", property.name.to_ascii_lowercase()))
                        .or_insert_with(|| t.bytes().map(f32::from).collect());
                    continue;
                }
            };
            params
                .entry(property.name.to_ascii_lowercase())
                .or_insert(value);
        }
    }
    (params, technique)
}

fn texture_name(params: &Params, parameter: &str) -> Option<String> {
    let bytes = params.get(&format!("{}#texture", parameter.to_ascii_lowercase()))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let name: String = bytes.iter().map(|b| char::from(*b as u8)).collect();
    (!name.is_empty() && !name.eq_ignore_ascii_case("none")).then_some(name)
}

fn solid(renderer: &mut Renderer, rgba: [u8; 4]) -> Option<TextureHandle> {
    let data = TextureData::rgba8(1, 1, rgba.to_vec())?;
    renderer.upload_texture(&data).ok()
}

const MAX_GRID_CELLS_PER_AXIS: usize = 1024;

fn standing_mesh(
    area: &StandingWaterArea,
    height_map: &HeightMapData,
    vertical_scale: f32,
    waves: &[PcaWave],
    density: MeshDensity,
) -> MeshData {
    let height = area.water_height as f32;
    let mut mesh = MeshData::default();
    let points = &area.points;
    if points.len() < 3
        || points
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return mesh;
    }
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in points {
        for ((l, h), &v) in lo.iter_mut().zip(hi.iter_mut()).zip(p.iter()) {
            *l = l.min(v);
            *h = h.max(v);
        }
    }
    let (wanted, _) = density.cell_and_cap();
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let cell = wanted.max(span / MAX_GRID_CELLS_PER_AXIS as f32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let nx = (((hi[0] - lo[0]) / cell).ceil() as usize).clamp(1, MAX_GRID_CELLS_PER_AXIS);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ny = (((hi[1] - lo[1]) / cell).ceil() as usize).clamp(1, MAX_GRID_CELLS_PER_AXIS);
    let mut index: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut vertex = |mesh: &mut MeshData, x: f32, y: f32| -> u32 {
        *index.entry((x.to_bits(), y.to_bits())).or_insert_with(|| {
            let bed = mint_sage_terrain::surface_height(height_map, vertical_scale, x, y)
                .unwrap_or(-1.0e4);
            let (reference, direction, gate) = pca_gate(waves, [x, y], PcaGate::default());
            mesh.vertices.push(
                Vertex::new([x, y, height], [reference[0], reference[1], bed], direction)
                    .with_uv2(gate),
            );
            u32::try_from(mesh.vertices.len().saturating_sub(1)).unwrap_or(u32::MAX)
        })
    };
    let full = cell * cell;
    for j in 0..ny {
        let y0 = lo[1] + j as f32 * cell;
        let y1 = lo[1] + (j + 1) as f32 * cell;
        for i in 0..nx {
            let x0 = lo[0] + i as f32 * cell;
            let x1 = lo[0] + (i + 1) as f32 * cell;
            let piece = clip_to_rect(points, [x0, y0], [x1, y1]);
            if piece.len() < 3 {
                continue;
            }
            let area2 = double_area(&piece).abs();
            if area2 * 0.5 >= full * 0.9999 {
                let a = vertex(&mut mesh, x0, y0);
                let b = vertex(&mut mesh, x1, y0);
                let c = vertex(&mut mesh, x1, y1);
                let d = vertex(&mut mesh, x0, y1);
                mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
            } else if area2 > 1.0e-3 {
                let ids: Vec<u32> = piece
                    .iter()
                    .map(|p| vertex(&mut mesh, p[0], p[1]))
                    .collect();
                for tri in mint_sage_terrain::triangulate(&piece).triangles {
                    for k in tri {
                        if let Some(id) = ids.get(k as usize) {
                            mesh.indices.push(*id);
                        }
                    }
                }
            }
        }
    }
    mesh.indices.truncate(mesh.indices.len() / 3 * 3);
    if PcaGate::default() == PcaGate::Open {
        open_depth_limits(&mut mesh);
    }
    mesh
}

fn double_area(points: &[[f32; 2]]) -> f32 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let a = points.get(i).copied().unwrap_or_default();
            let b = points.get((i + 1) % n).copied().unwrap_or_default();
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn clip_to_rect(polygon: &[[f32; 2]], lo: [f32; 2], hi: [f32; 2]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = polygon.to_vec();
    for (axis, bound, above) in [
        (0, lo[0], true),
        (0, hi[0], false),
        (1, lo[1], true),
        (1, hi[1], false),
    ] {
        let input = std::mem::take(&mut out);
        let n = input.len();
        if n == 0 {
            break;
        }
        let inside = |p: [f32; 2]| {
            let c = p.get(axis).copied().unwrap_or(0.0);
            if above {
                c >= bound
            } else {
                c <= bound
            }
        };
        for i in 0..n {
            let cur = input.get(i).copied().unwrap_or_default();
            let prev = input.get((i + n - 1) % n).copied().unwrap_or_default();
            let cross = || {
                let prev_axis = prev.get(axis).copied().unwrap_or(0.0);
                let cur_axis = cur.get(axis).copied().unwrap_or(0.0);
                let t = (bound - prev_axis) / (cur_axis - prev_axis);
                let mut p = [
                    prev[0] + (cur[0] - prev[0]) * t,
                    prev[1] + (cur[1] - prev[1]) * t,
                ];
                if let Some(slot) = p.get_mut(axis) {
                    *slot = bound;
                }
                p
            };
            match (inside(prev), inside(cur)) {
                (true, true) => out.push(cur),
                (true, false) => out.push(cross()),
                (false, true) => {
                    out.push(cross());
                    out.push(cur);
                }
                (false, false) => {}
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshDensity {
    Retail,
    Fine,
}

impl MeshDensity {
    fn cell_and_cap(self) -> (f32, usize) {
        match self {
            Self::Retail => (8.0 * MAP_XY_FACTOR, 50),
            Self::Fine => (2.0 * MAP_XY_FACTOR, 600),
        }
    }
}

pub struct PcaWave {
    points: Vec<[f32; 2]>,
    seaward: [f32; 2],
    radius: f32,
    depth_limit: f32,
}

impl PcaWave {
    fn new(
        area: &mint_sage_map::StandingWaveArea,
        height_map: &HeightMapData,
        vertical_scale: f32,
    ) -> Option<Self> {
        if area.enable_pca_wave != Some(1) || area.points.len() < 2 {
            return None;
        }
        let first = *area.points.first()?;
        let last = *area.points.last()?;
        let mut length = 0.0;
        for pair in area.points.windows(2) {
            if let [a, b] = pair {
                length += (mint_math::f32::powi(b[0] - a[0], 2)
                    + mint_math::f32::powi(b[1] - a[1], 2))
                .sqrt();
            }
        }
        let along = [last[0] - first[0], last[1] - first[1]];
        let norm = (along[0] * along[0] + along[1] * along[1]).sqrt().max(1e-3);
        let mut normal = [-along[1] / norm, along[0] / norm];
        let mid = [(first[0] + last[0]) * 0.5, (first[1] + last[1]) * 0.5];
        let bed = |d: f32| {
            mint_sage_terrain::surface_height(
                height_map,
                vertical_scale,
                mid[0] + normal[0] * d,
                mid[1] + normal[1] * d,
            )
            .unwrap_or(0.0)
        };
        if bed(60.0) > bed(-60.0) {
            normal = [-normal[0], -normal[1]];
        }
        Some(Self {
            points: area.points.clone(),
            seaward: normal,
            radius: length * 0.5,
            depth_limit: area.distance_from_shore.max(1) as f32,
        })
    }
}

const PCA_DEPTH_LIMIT_OFF_WAVE: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PcaGate {
    #[default]
    Line,
    Band,
    Open,
}

impl PcaGate {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Line => Self::Band,
            Self::Band => Self::Open,
            Self::Open => Self::Line,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Band => "band",
            Self::Open => "open",
        }
    }
}

const PCA_DEPTH_LIMIT_OPEN: f32 = 1.0e4;

const MAX_REAL_DEPTH: f32 = 1.0e3;

fn open_depth_limits(mesh: &mut MeshData) {
    let deepest = mesh
        .vertices
        .iter()
        .map(|v| v.position[2] - v.normal[2])
        .filter(|d| *d < MAX_REAL_DEPTH)
        .fold(0.0f32, f32::max);
    for v in &mut mesh.vertices {
        if v.uv2[1] > 0.0 {
            v.uv2[0] = deepest + 1.0;
        }
    }
}

fn pca_gate(waves: &[PcaWave], at: [f32; 2], mode: PcaGate) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let closed = (at, [1.0, 0.0], [PCA_DEPTH_LIMIT_OFF_WAVE, 0.0]);
    let mut best: Option<(f32, [f32; 2], &PcaWave)> = None;
    let mut blend = [0.0f32; 2];
    for wave in waves {
        for pair in wave.points.windows(2) {
            let [a, b] = pair else { continue };
            let ab = [b[0] - a[0], b[1] - a[1]];
            let len2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6);
            let t = (((at[0] - a[0]) * ab[0] + (at[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
            let p = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            let d = (mint_math::f32::powi(at[0] - p[0], 2) + mint_math::f32::powi(at[1] - p[1], 2))
                .sqrt();
            let within = mode != PcaGate::Line || d < wave.radius;
            if within && best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, p, wave));
            }
            let w = 1.0 / (d * d + 1.0);
            blend[0] += wave.seaward[0] * w;
            blend[1] += wave.seaward[1] * w;
        }
    }
    let Some((_, p, wave)) = best else {
        return closed;
    };
    match mode {
        PcaGate::Line => (p, wave.seaward, [wave.depth_limit, wave.radius]),
        PcaGate::Band | PcaGate::Open => {
            let n = (blend[0] * blend[0] + blend[1] * blend[1]).sqrt();
            let dir = if n > 1e-6 {
                [blend[0] / n, blend[1] / n]
            } else {
                wave.seaward
            };
            let limit = if mode == PcaGate::Open {
                PCA_DEPTH_LIMIT_OPEN
            } else {
                wave.depth_limit
            };
            (at, dir, [limit, 1.0])
        }
    }
}

fn river_colour(ribbon: &RiverRibbon, rig: &TerrainRig, water_diffuse: [f32; 4]) -> [f32; 4] {
    let [r, g, b, a] = ribbon.tint;
    if (r, g, b) != (1.0, 1.0, 1.0) {
        return [r, g, b, a];
    }
    let mut shade = rig.ambient;
    for light in &rig.lights {
        let down = -light.direction.z;
        if down > 0.0 {
            for (s, c) in shade.iter_mut().zip(light.color) {
                *s += down * c;
            }
        }
    }
    [
        (shade[0] * water_diffuse[0]).min(1.0),
        (shade[1] * water_diffuse[1]).min(1.0),
        (shade[2] * water_diffuse[2]).min(1.0),
        a,
    ]
}

fn river_strip(ribbon: &RiverRibbon, rig: &TerrainRig, water_diffuse: [f32; 4]) -> RiverStrip {
    let stations = ribbon.mesh.vertices.len() / 2;
    let mut vertices = Vec::with_capacity(stations * 2);
    let colour = river_colour(ribbon, rig, water_diffuse);
    let pos = |i: usize| {
        ribbon
            .mesh
            .vertices
            .get(i)
            .map_or([0.0, 0.0], |v| [v.position[0], v.position[1]])
    };
    let dist = |a: [f32; 2], b: [f32; 2]| {
        (mint_math::f32::powi(a[0] - b[0], 2) + mint_math::f32::powi(a[1] - b[1], 2)).sqrt()
    };
    let mut banks = 0.0;
    for i in 1..stations {
        banks += dist(pos(2 * i), pos(2 * i - 2)) + dist(pos(2 * i + 1), pos(2 * i - 1));
    }
    let end_len = dist(pos(0), pos(1)).max(1.0);
    let last = stations.saturating_sub(1);
    let perimeter = banks + end_len + dist(pos(2 * last), pos(2 * last + 1));
    let length = perimeter / 2.0 - end_len;
    let rectangles = stations.saturating_sub(1).max(1) as f32;
    let v_scale = (length / end_len) / rectangles;
    for i in 0..stations {
        for (side, (u1, u2)) in [(0.5f32, 1.0f32), (0.0, 0.0)].into_iter().enumerate() {
            let p = ribbon
                .mesh
                .vertices
                .get(2 * i + side)
                .map_or([0.0; 3], |v| v.position);
            vertices.push(
                Vertex::new(p, [0.0, 0.0, 1.0], [u1, 0.0])
                    .with_uv2([u2, 0.0])
                    .with_color(colour),
            );
        }
    }
    RiverStrip {
        vertices,
        indices: ribbon.mesh.indices.clone(),
        v_scale,
        scroll: ribbon.uv_scroll_speed,
    }
}

impl RiverStrip {
    fn animate(&mut self, seconds: f32) {
        let origin = self.scroll * seconds;
        for (n, vertex) in self.vertices.iter_mut().enumerate() {
            let i = (n / 2) as f32;
            let v = -origin
                + self.v_scale * i
                + mint_math::f32::sin(2.0 * std::f32::consts::PI * self.v_scale * i - 3.0 * origin)
                    / 22.0;
            vertex.uv[1] = v;
            vertex.uv2[1] = v;
        }
    }
}

impl FxWater {
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        water: &mint_sage_map::Water,
        surface: &WaterSurface,
        rivers: &RiverNetwork,
        height_map: &HeightMapData,
        vertical_scale: f32,
        rig: &TerrainRig,
        transparency: mint_sage_terrain::WaterTransparency,
        planes: &[(f32, mint_render::RenderTargetHandle)],
        water_diffuse: [f32; 4],
        density: MeshDensity,
    ) -> Self {
        let mut fx = Self {
            white: solid(renderer, [255, 255, 255, 255]),
            black: solid(renderer, [0, 0, 0, 255]),
            shroud_uv: [1.0, 1.0, 0.0, 0.0],
            ..Self::default()
        };
        let compressed = renderer.supports_block_compression();
        let mut uploaded: BTreeMap<String, Option<TextureHandle>> = BTreeMap::new();
        let mut texture = |renderer: &mut Renderer, name: &str, report: &mut Vec<String>| {
            let key = name.to_ascii_lowercase();
            *uploaded.entry(key).or_insert_with(|| {
                match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
                    Ok(loaded) => renderer.upload_texture(&loaded.data).ok(),
                    Err(error) => {
                        report.push(format!("texture {name}: {error}"));
                        None
                    }
                }
            })
        };

        match read_effect(vfs, "watershader.fxo") {
            Err(error) => fx.report.push(error),
            Ok(effect) => {
                let areas = water
                    .standing
                    .as_ref()
                    .map_or(&[][..], |w| w.areas.as_slice());
                let pca_waves = pca_waves_of(water, height_map, vertical_scale);
                fx.report.push(format!(
                    "  {} PCA wave area(s) feed the vertex gate ({}); mesh density {density:?}",
                    pca_waves.len(),
                    PcaGate::default().label()
                ));
                for body in &surface.bodies {
                    let Some(area) = areas.iter().find(|a| a.unique_id == body.unique_id) else {
                        continue;
                    };
                    let mesh = standing_mesh(area, height_map, vertical_scale, &pca_waves, density);
                    if mesh.indices.is_empty() {
                        continue;
                    }
                    let Ok(handle) = renderer.upload_mesh(&mesh) else {
                        continue;
                    };
                    fx.standing_meshes.push((handle, mesh.clone()));
                    let first_surface = fx.surfaces.len();
                    for detail in FxDetail::ALL {
                        let (material, (mut params, index)) = if body.fx_shader.is_empty() {
                            (String::new(), (Params::new(), None))
                        } else {
                            let variant = detail.material_variant(&body.fx_shader);
                            match variant
                                .as_deref()
                                .map(|name| (name, material_params(vfs, resolver, name)))
                            {
                                Some((name, found @ (_, Some(_)))) => (name.to_owned(), found),
                                _ => (
                                    body.fx_shader.clone(),
                                    material_params(vfs, resolver, &body.fx_shader),
                                ),
                            }
                        };
                        params.insert(
                            "maxalphadepth".into(),
                            vec![transparency.map_depth.unwrap_or(transparency.depth)],
                        );
                        params.insert(
                            "deepwateralpha".into(),
                            vec![transparency.map_alpha.unwrap_or(transparency.min_opacity)],
                        );
                        params.insert("numshadows".into(), vec![0.0]);
                        params.insert("#height".into(), vec![body.height]);
                        let Some(program) = select_technique(
                            renderer,
                            &effect,
                            "watershader.fxo",
                            index,
                            detail,
                            &params,
                            &mut fx,
                        ) else {
                            continue;
                        };
                        if let Some(existing) = fx.surfaces.get_mut(first_surface..).and_then(|s| {
                            s.iter_mut()
                                .find(|s| s.program == program && s.material_name == material)
                        }) {
                            existing.details |= detail.bit();
                            continue;
                        }
                        fx.report.push(format!(
                            "  standing '{}' {:?}: {} ({}) - {} vertices, material word {:?}, height {}, bed {:.1}..{:.1}, map depth {:?} alpha {:?}",
                            body.name,
                            detail,
                            fx.programs.get(program).map_or("", |p| p.label.as_str()),
                            material,
                            mesh.vertices.len(),
                            index,
                            body.height,
                            mesh.vertices.iter().map(|v| v.normal[2]).fold(f32::MAX, f32::min),
                            mesh.vertices.iter().map(|v| v.normal[2]).fold(f32::MIN, f32::max),
                            transparency.map_depth,
                            transparency.map_alpha,
                        ));
                        let reach = surface_reach(&mesh, fx.programs.get(program), &params);
                        fx.report.push(reach_line(reach.as_deref()));
                        fx.surfaces.push(FxSurface {
                            program,
                            mesh: handle,
                            material: None,
                            params,
                            textures: Vec::new(),
                            river: None,
                            pca: None,
                            details: detail.bit(),
                            shrouded: None,
                            material_name: material,
                            reach,
                        });
                    }
                }
            }
        }

        fx.pca_waves = pca_waves_of(water, height_map, vertical_scale);

        match read_effect(vfs, "riverwater.fxo") {
            Err(error) => fx.report.push(error),
            Ok(effect) => {
                for ribbon in &rivers.ribbons {
                    let mut params = Params::new();
                    for (parameter, name) in [
                        ("RiverTexture", &ribbon.river_texture),
                        ("SparklesTexture", &ribbon.sparkle_texture),
                        ("NoiseTexture", &ribbon.noise_texture),
                        ("OpacityTexture", &ribbon.alpha_edge_texture),
                    ] {
                        params.insert(
                            format!("{}#texture", parameter.to_ascii_lowercase()),
                            name.bytes().map(f32::from).collect(),
                        );
                    }
                    params.insert(
                        "uvscrollpersecond".into(),
                        vec![ribbon.uv_scroll_speed, ribbon.uv_scroll_speed],
                    );
                    params.insert("#height".into(), vec![ribbon.height]);
                    params.insert(
                        "isadditiveblended".into(),
                        vec![f32::from(u8::from(ribbon.additive_blend))],
                    );
                    let mut strip = river_strip(ribbon, rig, water_diffuse);
                    strip.animate(0.0);
                    let Ok(handle) = renderer.upload_mesh(&MeshData {
                        vertices: strip.vertices.clone(),
                        indices: strip.indices.clone(),
                    }) else {
                        continue;
                    };
                    let first_surface = fx.surfaces.len();
                    for detail in FxDetail::ALL {
                        let wanted = if detail == FxDetail::Low {
                            "Default_L"
                        } else {
                            "Default_M"
                        };
                        let program = find_or_build(
                            renderer,
                            &effect,
                            "riverwater.fxo",
                            wanted,
                            &params,
                            &mut fx,
                        )
                        .or_else(|| {
                            find_or_build(
                                renderer,
                                &effect,
                                "riverwater.fxo",
                                "Default_M",
                                &params,
                                &mut fx,
                            )
                        });
                        let Some(program) = program else { continue };
                        if let Some(existing) = fx
                            .surfaces
                            .get_mut(first_surface..)
                            .and_then(|s| s.iter_mut().find(|s| s.program == program))
                        {
                            existing.details |= detail.bit();
                            continue;
                        }
                        fx.report.push(format!(
                            "  river '{}' {:?}: {} additive={} scroll={} vScale={:.3}",
                            ribbon.name,
                            detail,
                            fx.programs.get(program).map_or("", |p| p.label.as_str()),
                            ribbon.additive_blend,
                            ribbon.uv_scroll_speed,
                            strip.v_scale
                        ));
                        let reach = surface_reach(
                            &MeshData {
                                vertices: strip.vertices.clone(),
                                indices: strip.indices.clone(),
                            },
                            fx.programs.get(program),
                            &params,
                        );
                        fx.report.push(reach_line(reach.as_deref()));
                        fx.surfaces.push(FxSurface {
                            program,
                            mesh: handle,
                            material: None,
                            params: params.clone(),
                            textures: Vec::new(),
                            river: Some(RiverStrip {
                                vertices: strip.vertices.clone(),
                                indices: strip.indices.clone(),
                                v_scale: strip.v_scale,
                                scroll: strip.scroll,
                            }),
                            pca: None,
                            details: detail.bit(),
                            shrouded: None,
                            material_name: String::new(),
                            reach,
                        });
                    }
                }
            }
        }

        for index in 0..fx.surfaces.len() {
            let Some(surface) = fx.surfaces.get(index) else {
                continue;
            };
            let Some(program) = fx.programs.get(surface.program) else {
                continue;
            };
            let names: Vec<(String, Option<String>)> = program
                .slot_textures
                .iter()
                .map(|parameter| (parameter.clone(), texture_name(&surface.params, parameter)))
                .collect();
            let shader = program.shader;
            let pca = names
                .iter()
                .find(|(parameter, _)| parameter.eq_ignore_ascii_case("WaterPCATexture1"))
                .and_then(|(_, name)| name.as_deref())
                .map(|name| {
                    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
                    format!("art/textures/{stem}.basis")
                });
            if let Some(path) = pca {
                let basis = vfs
                    .read_path(&VfsPath::new(&path))
                    .ok()
                    .and_then(|bytes| PcaBasis::parse(&String::from_utf8_lossy(&bytes)));
                fx.report.push(format!(
                    "  PCA basis {path}: {}",
                    basis.as_ref().map_or(
                        "MISSING or malformed - struct left at the effect default".to_owned(),
                        |b| format!("{} frames, speed {}", b.frames, b.speed)
                    )
                ));
                if let Some(surface) = fx.surfaces.get_mut(index) {
                    surface.pca = basis;
                }
            }
            let mut textures = Vec::with_capacity(names.len());
            for (parameter, name) in names {
                let handle = if parameter.eq_ignore_ascii_case("ShroudTexture") {
                    fx.white
                } else if parameter.eq_ignore_ascii_case("WaterReflectionTexture") {
                    let height = surface_height_of(renderer, fx.surfaces.get(index));
                    planes
                        .iter()
                        .min_by(|a, b| (a.0 - height).abs().total_cmp(&(b.0 - height).abs()))
                        .and_then(|(_, target)| renderer.render_target_texture(*target))
                } else {
                    match name {
                        Some(name) => texture(renderer, &name, &mut fx.report).or(fx.black),
                        None => fx.black,
                    }
                };
                textures.push(handle);
            }
            let block = vec![
                0u8;
                fx.programs
                    .get(fx.surfaces.get(index).map_or(0, |s| s.program))
                    .map_or(16, |p| p.translation.uniform_size() as usize)
            ];
            let slots: Vec<Option<TextureHandle>> =
                textures.iter().map(|t| t.or(fx.black)).collect();
            let material = renderer.create_custom_material(shader, &slots, &block);
            if let Some(surface) = fx.surfaces.get_mut(index) {
                surface.textures = textures;
                match material {
                    Ok(handle) => surface.material = Some(handle),
                    Err(error) => fx.report.push(format!("fx material: {error}")),
                }
            }
        }
        fx
    }

    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        self.surfaces
            .iter()
            .map(|s| {
                self.programs
                    .get(s.program)
                    .map_or_else(String::new, |p| p.label.clone())
            })
            .collect()
    }

    #[must_use]
    pub fn visibility(&self) -> Vec<bool> {
        self.surfaces
            .iter()
            .map(|s| s.details & self.detail.bit() != 0)
            .collect()
    }

    #[must_use]
    pub fn materials(&self, shroud: bool) -> Vec<Option<CustomMaterialHandle>> {
        self.surfaces
            .iter()
            .map(|s| {
                if shroud {
                    s.shrouded.or(s.material)
                } else {
                    s.material
                }
            })
            .collect()
    }

    pub fn bind_shroud(
        &mut self,
        renderer: &mut Renderer,
        texture: TextureHandle,
        extent: [f32; 4],
    ) -> Result<(), String> {
        self.shroud_uv = [
            1.0 / extent[2].max(1.0),
            1.0 / extent[3].max(1.0),
            -extent[0],
            -extent[1],
        ];
        for surface in &mut self.surfaces {
            let Some(program) = self.programs.get(surface.program) else {
                continue;
            };
            let mut slots: Vec<Option<TextureHandle>> =
                surface.textures.iter().map(|t| t.or(self.black)).collect();
            let mut changed = false;
            for (slot, parameter) in program.slot_textures.iter().enumerate() {
                if parameter.eq_ignore_ascii_case("ShroudTexture") {
                    if let Some(t) = slots.get_mut(slot) {
                        *t = Some(texture);
                        changed = true;
                    }
                }
            }
            if changed {
                let block = vec![0u8; program.translation.uniform_size() as usize];
                surface.shrouded = Some(
                    renderer
                        .create_custom_material(program.shader, &slots, &block)
                        .map_err(|e| format!("fx shroud material: {e}"))?,
                );
            }
        }
        Ok(())
    }

    pub fn set_gate(&mut self, renderer: &mut Renderer, gate: PcaGate) -> Result<(), String> {
        self.gate = gate;
        for (handle, mesh) in &mut self.standing_meshes {
            for v in &mut mesh.vertices {
                let (reference, direction, limits) =
                    pca_gate(&self.pca_waves, [v.position[0], v.position[1]], gate);
                v.normal[0] = reference[0];
                v.normal[1] = reference[1];
                v.uv = direction;
                v.uv2 = limits;
            }
            if gate == PcaGate::Open {
                open_depth_limits(mesh);
            }
            renderer
                .update_mesh_vertices(*handle, mesh)
                .map_err(|e| format!("fx gate: {e}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn surface_reach(&self, index: usize) -> Option<&[Aabb]> {
        self.surfaces.get(index).and_then(|s| s.reach.as_deref())
    }

    #[must_use]
    pub fn view_frustum(&self) -> Option<mint_render::Frustum> {
        self.view_proj.map(mint_render::Frustum::from_view_proj)
    }

    #[must_use]
    pub fn instances(&self) -> Vec<Instance> {
        self.surfaces
            .iter()
            .map(|surface| {
                let program = self.programs.get(surface.program);
                let lookup = |name: &str| lookup_in(&surface.params, program, name, &Params::new());
                Instance {
                    cull: false,
                    custom_material: surface.material,
                    blend: program.map_or(Blend::AlphaBlend, |p| p.blend(&lookup)),
                    depth_test: DepthTest::NearerOrEqual,
                    visible: false,
                    ..Instance::new(surface.mesh)
                }
            })
            .collect()
    }

    pub fn push(
        &mut self,
        renderer: &mut Renderer,
        camera: Camera,
        rig: &TerrainRig,
        seconds: f32,
    ) {
        if self.gate_wanted != self.gate {
            if let Err(error) = self.set_gate(renderer, self.gate_wanted) {
                eprintln!("  {error}");
            }
        }
        let (width, height) = renderer.size();
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let projection = camera.projection_with_depth(renderer.depth_direction(), aspect);
        let view = camera.view();
        let view_proj = projection.mul_mat4(view);
        self.view_proj = Some(view_proj);
        let mut engine = Params::new();
        engine.insert(
            "worldviewprojection".into(),
            view_proj.to_cols_array().to_vec(),
        );
        engine.insert("view".into(), view.to_cols_array().to_vec());
        engine.insert("projection".into(), projection.to_cols_array().to_vec());
        let mut identity = vec![0.0f32; 16];
        for i in 0..4 {
            if let Some(slot) = identity.get_mut(i * 5) {
                *slot = 1.0;
            }
        }
        engine.insert("world".into(), identity.clone());
        engine.insert("worldview".into(), view.to_cols_array().to_vec());
        let mut view_i = identity;
        for j in 0..3 {
            for i in 0..3 {
                if let Some(slot) = view_i.get_mut(j * 4 + i) {
                    *slot = view.get(j, i);
                }
            }
        }
        if let Some(slot) = view_i.get_mut(12) {
            *slot = camera.eye.x;
        }
        if let Some(slot) = view_i.get_mut(13) {
            *slot = camera.eye.y;
        }
        if let Some(slot) = view_i.get_mut(14) {
            *slot = camera.eye.z;
        }
        engine.insert("viewi".into(), view_i);
        engine.insert("time".into(), vec![seconds]);
        let mut lights = Vec::new();
        let mut sorted: Vec<&mint_render::DirectionalLight> = rig.lights.iter().collect();
        sorted.sort_by(|a, b| {
            let power =
                |l: &mint_render::DirectionalLight| l.color.iter().map(|c| c * c).sum::<f32>();
            power(b).total_cmp(&power(a))
        });
        for light in sorted.into_iter().take(3) {
            lights.extend_from_slice(&light.color);
            lights.extend_from_slice(&[-light.direction.x, -light.direction.y, -light.direction.z]);
        }
        engine.insert("directionallight".into(), lights);
        let fog = self
            .fog
            .map_or(vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1000.0], |fog| {
                let [r, g, b] = fog.color;
                vec![1.0, r, g, b, 1.0, fog.start, fog.end]
            });
        engine.insert("fog".into(), fog);
        engine.insert("shroud".into(), self.shroud_uv.to_vec());

        for surface in &mut self.surfaces {
            let (Some(program), Some(material)) =
                (self.programs.get(surface.program), surface.material)
            else {
                continue;
            };
            if let Some(strip) = surface.river.as_mut() {
                strip.animate(seconds);
                let data = MeshData {
                    vertices: strip.vertices.clone(),
                    indices: strip.indices.clone(),
                };
                if let Err(error) = renderer.update_mesh_vertices(surface.mesh, &data) {
                    crate::once::report_once(&format!("  fx river mesh: {error}"));
                }
            }
            let params = &surface.params;
            let pca = surface.pca.as_ref().map(|b| b.at(seconds, self.pca_rate));
            let lookup = |name: &str| {
                if name.eq_ignore_ascii_case("PCA") || name.eq_ignore_ascii_case("PCASetup_M") {
                    if let Some(values) = &pca {
                        return Some(values.clone());
                    }
                }
                lookup_in(params, Some(program), name, &engine)
            };
            let block = program.block(&lookup, width, height);
            for material in std::iter::once(material).chain(surface.shrouded) {
                if let Err(error) = renderer.update_custom_material(material, &block) {
                    crate::once::report_once(&format!("  fx material update: {error}"));
                }
            }
        }
    }
}

fn pca_waves_of(
    water: &mint_sage_map::Water,
    height_map: &HeightMapData,
    vertical_scale: f32,
) -> Vec<PcaWave> {
    water
        .waves
        .as_ref()
        .map_or(&[][..], |w| w.areas.as_slice())
        .iter()
        .filter_map(|a| PcaWave::new(a, height_map, vertical_scale))
        .collect()
}

fn gerstner_reach(lookup: &dyn Fn(&str) -> Option<Vec<f32>>) -> Option<(f32, f32)> {
    let (mut across, mut up) = (0.0f32, 0.0f32);
    for name in [
        "GWAmpFreqPhaseSteepness1",
        "GWAmpFreqPhaseSteepness2",
        "GWAmpFreqPhaseSteepness3",
    ] {
        let Some(wave) = lookup(name) else { continue };
        let (Some(amplitude), Some(frequency), Some(steepness)) =
            (wave.first(), wave.get(1), wave.get(3))
        else {
            continue;
        };
        up += (amplitude * 0.075).abs();
        let q = (steepness * 0.33).abs();
        if q > 0.0 {
            if frequency.is_nan() || *frequency <= 0.0 {
                return None;
            }
            across += 0.75 * q / frequency;
        }
    }
    (across.is_finite() && up.is_finite()).then_some((across, up))
}

const REACH_MARGIN: f32 = 2.0;

const REACH_TILE: f32 = 256.0;

fn surface_reach(
    mesh: &MeshData,
    program: Option<&FxProgram>,
    params: &Params,
) -> Option<Vec<Aabb>> {
    let lookup = |name: &str| lookup_in(params, program, name, &Params::new());
    let (across, up) = gerstner_reach(&lookup)?;
    let (across, up) = (across * REACH_MARGIN, up * REACH_MARGIN);
    let mut bins: BTreeMap<(i64, i64), Aabb> = BTreeMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let corners: Vec<mint_render::Vec3> = triangle
            .iter()
            .filter_map(|i| mesh.vertices.get(*i as usize))
            .map(|v| mint_render::Vec3::from_array(v.position))
            .collect();
        let Some(first) = corners.first() else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            (first.x / REACH_TILE).floor() as i64,
            (first.y / REACH_TILE).floor() as i64,
        );
        let bin = bins.entry(key).or_insert(Aabb::EMPTY);
        for corner in corners {
            bin.insert(corner);
        }
    }
    Some(
        bins.into_values()
            .filter(|b| !b.is_empty())
            .map(|b| {
                let (min, max) = (b.min(), b.max());
                Aabb::from_corners(
                    mint_render::Vec3::new(min.x - across, min.y - across, min.z - up),
                    mint_render::Vec3::new(max.x + across, max.y + across, max.z + up),
                )
            })
            .collect(),
    )
}

fn reach_line(reach: Option<&[Aabb]>) -> String {
    let Some(boxes) = reach else {
        return "    reach unbounded".to_owned();
    };
    let mut all = Aabb::EMPTY;
    for b in boxes {
        all.insert(b.min());
        all.insert(b.max());
    }
    let (min, max) = (all.min(), all.max());
    format!(
        "    reach {} box(es) over x {:.0}..{:.0} y {:.0}..{:.0} z {:.1}..{:.1}",
        boxes.len(),
        min.x,
        max.x,
        min.y,
        max.y,
        min.z,
        max.z
    )
}

fn surface_height_of(_renderer: &Renderer, surface: Option<&FxSurface>) -> f32 {
    surface
        .and_then(|s| s.params.get("#height"))
        .and_then(|v| v.first().copied())
        .unwrap_or(0.0)
}

fn lookup_in(
    params: &Params,
    program: Option<&FxProgram>,
    name: &str,
    engine: &Params,
) -> Option<Vec<f32>> {
    let key = name.to_ascii_lowercase();
    engine
        .get(&key)
        .or_else(|| params.get(&key))
        .cloned()
        .or_else(|| program.and_then(|p| p.default_value(name)))
}

fn find_or_build(
    renderer: &mut Renderer,
    effect: &Effect,
    file: &str,
    technique: &str,
    params: &Params,
    fx: &mut FxWater,
) -> Option<usize> {
    let label = format!("{file} {technique}");
    if let Some(index) = fx.programs.iter().position(|p| p.label == label) {
        return Some(index);
    }
    if fx.failed.contains(&label) {
        return None;
    }
    let lookup = |name: &str| {
        params
            .get(&name.to_ascii_lowercase())
            .cloned()
            .or_else(|| effect.parameter(name).map(mint_sage_fx::Parameter::floats))
    };
    match FxProgram::build(renderer, effect, file, technique, &lookup) {
        Ok(program) => {
            fx.report.push(format!("  translated {label}"));
            fx.programs.push(program);
            Some(fx.programs.len() - 1)
        }
        Err(error) => {
            fx.report.push(format!("  NOT TRANSLATED {error}"));
            fx.failed.insert(label);
            None
        }
    }
}

fn select_technique(
    renderer: &mut Renderer,
    effect: &Effect,
    file: &str,
    index: Option<u32>,
    detail: FxDetail,
    params: &Params,
    fx: &mut FxWater,
) -> Option<usize> {
    let named = index
        .and_then(|i| effect.techniques.get(i as usize))
        .map(|t| t.name.clone());
    let mut candidates = Vec::new();
    if let Some(name) = &named {
        let family = ["_U", "_M", "_L"]
            .iter()
            .find_map(|s| name.strip_suffix(s))
            .unwrap_or(name);
        let suffixed = format!("{family}{}", detail.suffix());
        if effect.technique(&suffixed).is_some() {
            candidates.push(suffixed);
        }
        candidates.push(name.clone());
    }
    candidates.push("DefaultTechnique".to_owned());
    candidates.dedup();
    let first = candidates.first().cloned().unwrap_or_default();
    for name in &candidates {
        if let Some(program) = find_or_build(renderer, effect, file, name, params, fx) {
            if *name != first {
                fx.report
                    .push(format!("  {detail:?}: {first} fell back to {name}"));
            }
            return Some(program);
        }
    }
    None
}
