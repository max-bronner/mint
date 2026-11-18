use mint_render::{
    Aabb, Blend, Camera, CustomShaderHandle, Instance, Mat4, MeshData, Vec3, Vertex,
};
use mint_sage_ini::{Flipbook, ParticleKind, ParticleRotation, ParticleShader, RandomRange};

use crate::rng::ClientRng;
use crate::world::{add, scale, sub, Particle};

pub(crate) const VERTICES_PER_QUAD: usize = 4;
const INDICES_PER_QUAD: usize = 6;

pub(crate) const SIZE_TO_HALF_EXTENT: f32 = 0.5;

const ALPHA_TEST_REFERENCE: f32 = 96.0 / 255.0;

pub(crate) fn shader_draw_state(shader: ParticleShader) -> (Blend, f32) {
    match shader {
        ParticleShader::AlphaTest => (Blend::Opaque, ALPHA_TEST_REFERENCE),
        ParticleShader::Additive => (Blend::Additive, 0.0),
        _ => (Blend::AlphaBlend, 0.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BillboardMode {
    #[default]
    Data,
    Screen,
    Ground,
}

impl BillboardMode {
    pub(crate) fn flat(self, ground_aligned: bool) -> bool {
        match self {
            Self::Data => ground_aligned,
            Self::Screen => false,
            Self::Ground => true,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Data => Self::Screen,
            Self::Screen => Self::Ground,
            Self::Ground => Self::Data,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Screen => "screen",
            Self::Ground => "ground",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "data" => Self::Data,
            "screen" => Self::Screen,
            "ground" => Self::Ground,
            _ => return None,
        })
    }
}

const SUBMISSION_SORT_LEVEL: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    None,
    Level,
    #[default]
    Depth,
}

impl SortMode {
    pub(crate) fn level_for(self, authored: Option<u8>) -> Option<u8> {
        match self {
            Self::None => Some(SUBMISSION_SORT_LEVEL),
            Self::Level => authored,
            Self::Depth => None,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Level,
            Self::Level => Self::Depth,
            Self::Depth => Self::None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Level => "level",
            Self::Depth => "depth",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "none" => Self::None,
            "level" => Self::Level,
            "depth" => Self::Depth,
            _ => return None,
        })
    }
}

pub(crate) fn billboard_axes(camera: &Camera) -> (Vec3, Vec3) {
    let forward = sub(camera.target, camera.eye);
    if !forward.is_finite() || forward.length_squared() <= 0.0 {
        return (Vec3::X, Vec3::Y);
    }
    let forward = forward.normalize();
    let right = forward.cross(camera.up);
    if right.length_squared() <= 1e-12 {
        return (Vec3::X, Vec3::Y);
    }
    let right = right.normalize();
    (right, right.cross(forward).normalize())
}

pub(crate) const FLAT_RIGHT: Vec3 = Vec3::new(-2.0, 0.0, 0.0);
pub(crate) const FLAT_UP: Vec3 = Vec3::new(0.0, 2.0, 0.0);

pub(crate) fn quad_axes(
    mode: BillboardMode,
    ground_aligned: bool,
    camera_right: Vec3,
    camera_up: Vec3,
) -> (Vec3, Vec3) {
    if mode.flat(ground_aligned) {
        (FLAT_RIGHT, FLAT_UP)
    } else {
        (camera_right, camera_up)
    }
}

pub(crate) fn quad_batch_mesh(capacity: usize) -> MeshData {
    let vertices = capacity.saturating_mul(VERTICES_PER_QUAD);
    let mut mesh = MeshData {
        vertices: vec![Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]); vertices],
        indices: Vec::with_capacity(capacity.saturating_mul(INDICES_PER_QUAD)),
    };
    for quad in 0..capacity {
        let base = u32::try_from(quad.saturating_mul(VERTICES_PER_QUAD)).unwrap_or(0);
        for offset in [0, 1, 2, 0, 2, 3] {
            mesh.indices.push(base.saturating_add(offset));
        }
    }
    mesh
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawShape {
    #[default]
    Sprite,
    Model,
    Streak,
    Volume,
    Smudge,
}

impl DrawShape {
    #[must_use]
    pub fn of(kind: ParticleKind, draw_module: Option<&str>, particle_name: Option<&str>) -> Self {
        let model = kind == ParticleKind::Drawable
            || draw_module.is_some_and(|module| module.eq_ignore_ascii_case("RenderObjectDraw"));
        if model {
            return Self::Model;
        }
        if particle_name.is_some_and(|name| name.as_bytes().starts_with(b"SMUD")) {
            return Self::Smudge;
        }
        if let Some(module) = draw_module {
            if module.eq_ignore_ascii_case("StreakDraw")
                || module.eq_ignore_ascii_case("LightningDraw")
            {
                return Self::Streak;
            }
        }
        match kind {
            ParticleKind::Streak => Self::Streak,
            ParticleKind::VolumeParticle => Self::Volume,
            ParticleKind::Smudge => Self::Smudge,
            _ => Self::Sprite,
        }
    }

    #[must_use]
    pub fn batch(self) -> Option<BatchShape> {
        match self {
            Self::Sprite | Self::Volume | Self::Smudge => Some(BatchShape::Quads),
            Self::Streak => Some(BatchShape::Ribbon),
            Self::Model => None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sprite => "sprite",
            Self::Model => "model",
            Self::Streak => "streak",
            Self::Volume => "volume",
            Self::Smudge => "smudge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeMode {
    #[default]
    Data,
    Sprite,
}

impl ShapeMode {
    #[must_use]
    pub fn shape(self, authored: DrawShape) -> DrawShape {
        match self {
            Self::Data => authored,
            Self::Sprite => DrawShape::Sprite,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Data => Self::Sprite,
            Self::Sprite => Self::Data,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Sprite => "sprite",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "data" => Self::Data,
            "sprite" => Self::Sprite,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchShape {
    Quads,
    Ribbon,
}

pub const VOLUME_PARTICLE_COPIES: usize = 6;

pub const VOLUME_PARTICLE_DEPTH: f32 = 1.0;

pub const VOLUME_PARTICLE_ATTENUATION: f32 = 1.0 / 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlipbookMode {
    Off,
    Life,
    LifeBlend,
    Frame,
    #[default]
    Blend,
    Retail,
}

impl FlipbookMode {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Life,
        Self::LifeBlend,
        Self::Frame,
        Self::Blend,
        Self::Retail,
    ];

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Life,
            Self::Life => Self::LifeBlend,
            Self::LifeBlend => Self::Frame,
            Self::Frame => Self::Blend,
            Self::Blend => Self::Retail,
            Self::Retail => Self::Off,
        }
    }

    #[must_use]
    pub fn rung(self) -> usize {
        Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .map_or(0, |index| index.saturating_add(1))
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Life => "life",
            Self::LifeBlend => "life-blend",
            Self::Frame => "frame",
            Self::Blend => "blend",
            Self::Retail => "retail",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.label() == text)
    }

    #[must_use]
    pub fn needs_shader(self) -> bool {
        matches!(self, Self::LifeBlend | Self::Blend | Self::Retail)
    }

    #[must_use]
    pub fn cell(self, book: Flipbook, age: f32, lifetime: u32, start: u32) -> f32 {
        let age = if age.is_finite() { age.max(0.0) } else { 0.0 };
        let total_frames = book.total_frames.max(1);
        let total = f32::from(u16::try_from(total_frames).unwrap_or(u16::MAX));
        let speed = if book.speed_multiplier.is_finite() {
            book.speed_multiplier
        } else {
            1.0
        };
        match self {
            Self::Off => 0.0,
            Self::Life | Self::LifeBlend => {
                #[allow(clippy::cast_precision_loss)]
                let life = lifetime.max(1) as f32;
                age / life * total * speed
            }
            Self::Frame | Self::Blend | Self::Retail => {
                let first = start.checked_rem(total_frames).unwrap_or(0);
                f32::from(u16::try_from(first).unwrap_or(0)) + age * speed
            }
        }
    }
}

#[must_use]
pub(crate) fn retail_cell_uv(book: Flipbook, n: u32) -> ([f32; 2], [f32; 2]) {
    let total = book.total_frames.max(1);
    let per_row = book.frames_per_row.max(1);
    #[allow(clippy::cast_precision_loss)]
    let (t, f, n) = (
        total as f32,
        per_row as f32,
        n.checked_rem(total).unwrap_or(0) as f32,
    );
    let a = (n / t).fract() * (t - 1.0);
    let b = (a / f).fract();
    let u = (b * (f - 1.0)).floor() / f;
    let v = (a / f).floor() / f;
    ([u, v], [1.0 / f, 1.0 / f])
}

#[must_use]
pub(crate) fn cell_uv(book: Flipbook, cell: u32) -> ([f32; 2], [f32; 2]) {
    let per_row = book.frames_per_row.max(1);
    let rows = book.rows();
    let cell = cell.checked_rem(book.total_frames.max(1)).unwrap_or(0);
    let column = cell.checked_rem(per_row).unwrap_or(0);
    let row = cell
        .checked_div(per_row)
        .and_then(|row| row.checked_rem(rows))
        .unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let size = [1.0 / per_row as f32, 1.0 / rows as f32];
    #[allow(clippy::cast_precision_loss)]
    let origin = [column as f32 * size[0], row as f32 * size[1]];
    (origin, size)
}

#[must_use]
pub fn ribbon_mesh(capacity: usize) -> MeshData {
    let vertices = capacity.saturating_mul(2);
    let segments = capacity.saturating_sub(1);
    let mut mesh = MeshData {
        vertices: vec![Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]); vertices],
        indices: Vec::with_capacity(segments.saturating_mul(INDICES_PER_QUAD)),
    };
    for segment in 0..segments {
        let base = u32::try_from(segment.saturating_mul(2)).unwrap_or(0);
        for offset in [0, 1, 3, 0, 3, 2] {
            mesh.indices.push(base.saturating_add(offset));
        }
    }
    mesh
}

pub(crate) fn batch_mesh(shape: BatchShape, capacity: usize) -> MeshData {
    match shape {
        BatchShape::Quads => quad_batch_mesh(capacity),
        BatchShape::Ribbon => ribbon_mesh(capacity),
    }
}

fn vertex_color(particle: &Particle, keys: bool, blend: f32) -> [f32; 4] {
    if keys {
        let (color, alpha) = particle.drawn_color(blend);
        [color[0], color[1], color[2], alpha]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QuadFill {
    pub(crate) right_axis: Vec3,
    pub(crate) up_axis: Vec3,
    pub(crate) keys: bool,
    pub(crate) flipbook: Option<(Flipbook, FlipbookMode)>,
    pub(crate) smudge_seed: Option<u64>,
    pub(crate) blend: f32,
}

pub(crate) fn fill_quads(
    scratch: &mut MeshData,
    particles: &[Particle],
    origin: Vec3,
    slots: usize,
    style: &QuadFill,
) -> Aabb {
    let QuadFill {
        right_axis,
        up_axis,
        keys,
        flipbook,
        smudge_seed,
        blend,
    } = *style;
    let mut smudge_rng = smudge_seed.map(ClientRng::new);
    let vertices = slots.saturating_mul(VERTICES_PER_QUAD);
    scratch.vertices.clear();
    scratch.vertices.resize(
        vertices,
        Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]).with_color([0.0; 4]),
    );

    let normal = right_axis.cross(up_axis).normalize().to_array();
    let corner_uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let mut bounds = Aabb::EMPTY;
    for (slot, particle) in particles.iter().take(slots).enumerate() {
        let (drawn_position, drawn_size, drawn_angle) = particle.drawn(blend);
        let half = drawn_size * SIZE_TO_HALF_EXTENT;
        let (sin, cos) = if smudge_rng.is_some() {
            (0.0, 1.0)
        } else {
            mint_math::f32::sin_cos(drawn_angle)
        };
        let right = add(scale(right_axis, cos), scale(up_axis, sin));
        let up = add(scale(up_axis, cos), scale(right_axis, -sin));
        let color = vertex_color(particle, keys, blend);
        let (this_cell, next_cell, weight) = cells_for(flipbook, particle, blend);
        let shift = smudge_rng
            .as_mut()
            .map_or(0.0, |rng| (rng.unit() * 2.0 - 1.0) * SMUDGE_MAX_OFFSET);
        let corners = [
            add(scale(right, -half), scale(up, -half)),
            add(scale(right, half), scale(up, -half)),
            add(scale(right, half), scale(up, half)),
            add(scale(right, -half), scale(up, half)),
        ];
        for corner in 0..VERTICES_PER_QUAD {
            let Some(index) = slot
                .checked_mul(VERTICES_PER_QUAD)
                .and_then(|base| base.checked_add(corner))
            else {
                continue;
            };
            let (Some(vertex), Some(offset), Some(uv)) = (
                scratch.vertices.get_mut(index),
                corners.get(corner),
                corner_uvs.get(corner),
            ) else {
                continue;
            };
            let position = add(drawn_position, *offset);
            bounds.insert(position);
            *vertex = Vertex::new(position.to_array(), normal, cell_at(this_cell, *uv))
                .with_color(color)
                .with_uv2(cell_at(next_cell, *uv))
                .with_tangent([weight, shift, 0.0, 1.0]);
        }
    }
    if bounds.is_empty() {
        bounds.insert(origin);
    }
    bounds
}

fn cells_for(
    flipbook: Option<(Flipbook, FlipbookMode)>,
    particle: &Particle,
    blend: f32,
) -> (([f32; 2], [f32; 2]), ([f32; 2], [f32; 2]), f32) {
    let whole = ([0.0, 0.0], [1.0, 1.0]);
    let Some((book, mode)) = flipbook else {
        return (whole, whole, 0.0);
    };
    let t = if blend.is_finite() {
        blend.clamp(0.0, 1.0)
    } else {
        1.0
    };
    #[allow(clippy::cast_precision_loss)]
    let age = (particle.age as f32 - 1.0 + t).max(0.0);
    let cell = mode
        .cell(book, age, particle.lifetime, particle.sheet_start)
        .max(0.0);
    let floor = cell.floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = floor.min(f32::from(u16::MAX)) as u32;
    let weight = if book.animates() { cell - floor } else { 0.0 };
    if mode == FlipbookMode::Retail {
        return (
            retail_cell_uv(book, index),
            retail_cell_uv(book, index.saturating_add(1)),
            weight,
        );
    }
    (
        cell_uv(book, index),
        cell_uv(book, index.saturating_add(1)),
        weight,
    )
}

fn cell_at((origin, size): ([f32; 2], [f32; 2]), corner: [f32; 2]) -> [f32; 2] {
    [
        origin[0] + corner[0] * size[0],
        origin[1] + corner[1] * size[1],
    ]
}

pub(crate) fn fill_ribbon(
    scratch: &mut MeshData,
    particles: &[Particle],
    origin: Vec3,
    slots: usize,
    view: Vec3,
    keys: bool,
    blend: f32,
    jitter: Option<&RibbonJitter>,
) -> Aabb {
    fill_ribbon_points(scratch, particles.len(), slots, origin, view, |slot| {
        let particle = particles.get(slot)?;
        let (drawn, size, _) = particle.drawn(blend);
        Some(RibbonPoint {
            position: jitter.map_or(drawn, |jitter| add(drawn, jitter.offset(slot))),
            half_width: size * SIZE_TO_HALF_EXTENT,
            color: if slot == 0 {
                [0.0; 4]
            } else {
                vertex_color(particle, keys, blend)
            },
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RibbonPoint {
    pub position: Vec3,
    pub half_width: f32,
    pub color: [f32; 4],
}

pub fn fill_ribbon_points(
    scratch: &mut MeshData,
    count: usize,
    slots: usize,
    origin: Vec3,
    view: Vec3,
    point: impl Fn(usize) -> Option<RibbonPoint>,
) -> Aabb {
    let vertices = slots.saturating_mul(2);
    scratch.vertices.clear();
    scratch.vertices.resize(
        vertices,
        Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]).with_color([0.0; 4]),
    );
    let mut bounds = Aabb::EMPTY;
    let live = count.min(slots);
    let normal = view.to_array();
    let mut last = (origin, origin);
    let at = |slot: usize| point(slot).map(|held| held.position);
    for slot in 0..live {
        let Some(here) = point(slot) else {
            continue;
        };
        let before = slot.checked_sub(1).and_then(at).unwrap_or(here.position);
        let after = Some(slot.saturating_add(1))
            .filter(|next| *next < live)
            .and_then(at)
            .unwrap_or(here.position);
        let across = sub(after, before).cross(view);
        let side = if across.length_squared() > 1e-12 {
            scale(across.normalize(), here.half_width)
        } else {
            Vec3::ZERO
        };
        #[allow(clippy::cast_precision_loss)]
        let v = slot as f32 / live.max(2).saturating_sub(1) as f32;
        let left = sub(here.position, side);
        let right = add(here.position, side);
        bounds.insert(left);
        bounds.insert(right);
        last = (left, right);
        if let Some(vertex) = scratch.vertices.get_mut(slot.saturating_mul(2)) {
            *vertex = Vertex::new(left.to_array(), normal, [0.0, v]).with_color(here.color);
        }
        if let Some(vertex) = scratch
            .vertices
            .get_mut(slot.saturating_mul(2).saturating_add(1))
        {
            *vertex = Vertex::new(right.to_array(), normal, [1.0, v]).with_color(here.color);
        }
    }
    for slot in live..slots {
        if let Some(vertex) = scratch.vertices.get_mut(slot.saturating_mul(2)) {
            *vertex = Vertex::new(last.0.to_array(), normal, [0.0, 1.0]).with_color([0.0; 4]);
        }
        if let Some(vertex) = scratch
            .vertices
            .get_mut(slot.saturating_mul(2).saturating_add(1))
        {
            *vertex = Vertex::new(last.1.to_array(), normal, [1.0, 1.0]).with_color([0.0; 4]);
        }
    }
    if bounds.is_empty() {
        bounds.insert(origin);
    }
    bounds
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RibbonJitter {
    pub(crate) ranges: [RandomRange; 3],
    pub(crate) frame: Option<Mat4>,
    pub(crate) seed: u64,
}

impl RibbonJitter {
    fn offset(&self, slot: usize) -> Vec3 {
        let mut rng = ClientRng::new(
            self.seed
                .wrapping_add((slot as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                | 1,
        );
        let [x, y, z] = self.ranges.map(|range| rng.range(range));
        let local = Vec3::new(x, y, z);
        self.frame
            .map_or(local, |frame| frame.transform_vector(local))
    }
}

#[must_use]
pub(crate) fn model_transform(
    particle: &Particle,
    blend: f32,
    frame: Option<Mat4>,
    rotation: ParticleRotation,
) -> Mat4 {
    let (position, drawn_size, angle) = particle.drawn(blend);
    let scale = match particle.drawn_render_size(blend) {
        Some([x, y, z]) => Vec3::new(x.max(0.0), y.max(0.0), z.max(0.0)),
        None => {
            let size = positive_or_unit(drawn_size);
            Vec3::new(size, size, size)
        }
    };
    let placement = Mat4::from_translation(position);
    let placement = match frame {
        Some(frame) => placement.mul_mat4(rotation_of(frame)),
        None => placement,
    };
    placement
        .mul_mat4(spin(rotation, angle, particle.velocity))
        .mul_mat4(Mat4::from_scale(scale))
}

fn spin(rotation: ParticleRotation, angle: f32, velocity: Vec3) -> Mat4 {
    let axis = match rotation {
        ParticleRotation::Off => return Mat4::IDENTITY,
        ParticleRotation::X => Vec3::X,
        ParticleRotation::Y => Vec3::Y,
        ParticleRotation::Z => Vec3::Z,
        ParticleRotation::Velocity => {
            let axis = Vec3::new(velocity.x, -velocity.y, velocity.z);
            if !axis.is_finite() || axis.length_squared() <= 0.0 {
                return Mat4::IDENTITY;
            }
            axis.normalize()
        }
    };
    let (sin, cos) = mint_math::f32::sin_cos(-angle);
    let turn = |v: Vec3| {
        let along = axis.dot(v) * (1.0 - cos);
        let across = axis.cross(v);
        Vec3::new(
            v.x * cos + across.x * sin + axis.x * along,
            v.y * cos + across.y * sin + axis.y * along,
            v.z * cos + across.z * sin + axis.z * along,
        )
    };
    let [x, y, z] = [turn(Vec3::X), turn(Vec3::Y), turn(Vec3::Z)];
    Mat4::from_cols([
        [x.x, x.y, x.z, 0.0],
        [y.x, y.y, y.z, 0.0],
        [z.x, z.y, z.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

fn rotation_of(frame: Mat4) -> Mat4 {
    let mut cols = frame.to_cols();
    if let Some(origin) = cols.get_mut(3) {
        *origin = [0.0, 0.0, 0.0, 1.0];
    }
    Mat4::from_cols(cols)
}

fn positive_or_unit(value: f32) -> f32 {
    if value > 0.0 {
        value
    } else {
        1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PartKeys {
    pub tint: [f32; 4],
    pub diffuse: [f32; 3],
    pub emissive: [f32; 3],
}

#[must_use]
pub(crate) fn part_keys(
    shader: Option<ParticleShader>,
    part: PartKeys,
    colour: [f32; 3],
    alpha: f32,
    alpha_in_colour: bool,
) -> PartKeys {
    let scale = if alpha_in_colour { alpha } else { 1.0 };
    let [r, g, b] = colour;
    let keyed = [r * scale, g * scale, b * scale];
    match shader {
        Some(ParticleShader::W3dEmissive) => PartKeys {
            emissive: keyed,
            ..part
        },
        Some(ParticleShader::W3dDiffuse) => PartKeys {
            diffuse: keyed,
            ..part
        },
        Some(ParticleShader::W3dAlpha) => {
            let [r, g, b, _] = part.tint;
            PartKeys {
                tint: [r, g, b, alpha],
                ..part
            }
        }
        _ => part,
    }
}

pub(crate) fn push_volume_copies(
    out: &mut Vec<Instance>,
    base: &Instance,
    toward_camera: Vec3,
    depth: f32,
) {
    for (offset, attenuation) in volume_copies(toward_camera, depth) {
        out.push(Instance {
            transform: Mat4::from_translation(offset),
            tint: [1.0, 1.0, 1.0, attenuation],
            ..*base
        });
    }
}

#[must_use]
pub(crate) fn volume_copies(
    toward_camera: Vec3,
    depth: f32,
) -> [(Vec3, f32); VOLUME_PARTICLE_COPIES] {
    let step = if depth.is_finite() {
        depth * VOLUME_PARTICLE_DEPTH / 6.0
    } else {
        0.0
    };
    let mut out = [(Vec3::ZERO, VOLUME_PARTICLE_ATTENUATION); VOLUME_PARTICLE_COPIES];
    for (copy, slot) in out.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let far = VOLUME_PARTICLE_COPIES
            .saturating_sub(1)
            .saturating_sub(copy) as f32;
        slot.0 = scale(toward_camera, far * step);
    }
    out
}

pub const FLIPBOOK_SHADER_WGSL: &str = include_str!("flipbook.wgsl");

pub const SMUDGE_SHADER_WGSL: &str = include_str!("smudge.wgsl");

pub const SMUDGE_MAX_OFFSET: f32 = 0.06;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlatSize {
    #[default]
    Double,
    Single,
}

impl FlatSize {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Double => Self::Single,
            Self::Single => Self::Double,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Double => "2x size (WW3D ground branch)",
            Self::Single => "1x size (billboard extent)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmudgeJitter {
    #[default]
    Step,
    Frame,
}

impl SmudgeJitter {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Step => Self::Frame,
            Self::Frame => Self::Step,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Step => "per 30 Hz step",
            Self::Frame => "per drawn frame",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ParticleShaders {
    pub(crate) flipbook: Option<CustomShaderHandle>,
    pub(crate) smudge: Option<CustomShaderHandle>,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp
)]
mod tests;
