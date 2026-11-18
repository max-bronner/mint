use mint_render::{
    Blend, Camera, Instance, Mat4, MeshData, MeshHandle, Renderer, TextureHandle, Vec3, Vertex,
};
use mint_sage_ini::SnowSetting;
use mint_sage_model::Resolver;
use mint_vfs::Vfs;

pub const SNOW_NOISE: usize = 64;

pub const MAX_FLAKES_PER_TILE: usize = 16_384;

const MAX_TILES: i64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpellRain {
    phase: RainPhase,
    cached_rainy: bool,
    timer: f32,
    carry: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RainPhase {
    #[default]
    Idle,
    RampIn,
    Hold,
    RampOut,
}

impl SpellRain {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.phase != RainPhase::Idle
    }

    pub fn update(&mut self, setting: &SnowSetting, raining: bool, dt: f32) {
        let steps = crate::client_clock::client_frames_elapsed(&mut self.carry, dt);
        if !setting.spell_enabled {
            return;
        }
        for _ in 0..steps {
            self.step(setting, raining);
        }
    }

    fn step(&mut self, setting: &SnowSetting, raining: bool) {
        let duration = spell_frames(setting);
        if self.phase == RainPhase::Idle {
            if raining && !self.cached_rainy {
                self.cached_rainy = true;
                self.phase = RainPhase::RampIn;
                self.timer = duration;
            }
            return;
        }
        if matches!(self.phase, RainPhase::RampIn | RainPhase::RampOut) {
            self.timer -= 1.0;
        }
        self.phase = match self.elapsed(duration) {
            Some(t) if t > setting.ramp_control[1] => RainPhase::RampOut,
            Some(t) if t > setting.ramp_control[0] => RainPhase::Hold,
            Some(_) => RainPhase::RampIn,
            None => {
                self.timer = 0.0;
                self.cached_rainy = raining;
                RainPhase::Idle
            }
        };
        if self.cached_rainy && !raining {
            self.phase = RainPhase::RampOut;
            self.timer = fade_out_frames(duration, setting.ramp_control[1]);
            self.cached_rainy = false;
        }
    }

    fn elapsed(&self, duration: f32) -> Option<f32> {
        if duration <= 0.0 {
            return None;
        }
        let t = (duration - self.timer) / duration;
        (t > 0.0 && t <= 1.0).then_some(t)
    }

    #[must_use]
    pub fn current(&self, setting: &SnowSetting) -> Option<(f32, f32)> {
        if !self.active() {
            return None;
        }
        let t = self.elapsed(spell_frames(setting)).unwrap_or(0.0);
        ramp_step(
            t,
            setting.ramp_control,
            setting.ramp_spacing,
            setting.ramp_speed,
        )
    }
}

fn spell_frames(setting: &SnowSetting) -> f32 {
    whole_frames(setting.spell_duration)
}

fn fade_out_frames(duration: f32, b: f32) -> f32 {
    whole_frames((1.0 - b) * duration)
}

fn whole_frames(frames: f32) -> f32 {
    if frames.is_finite() && frames > 0.0 {
        frames.trunc()
    } else {
        0.0
    }
}

fn ramp_step(
    t: f32,
    [a, b]: [f32; 2],
    [s0, s1]: [f32; 2],
    [v0, v1]: [f32; 2],
) -> Option<(f32, f32)> {
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    if t > b {
        let span = 1.0 - b;
        let f = if span <= 1.0e-5 { 1.0 } else { (t - b) / span };
        Some((s1 - (s1 - s0) * f, v1 - (v1 - v0) * f))
    } else if t > a {
        Some((s1, v1))
    } else {
        let f = if a <= 1.0e-5 { 1.0 } else { t / a };
        Some((s0 + (s1 - s0) * f, v0 + (v1 - v0) * f))
    }
}

pub struct SnowScene {
    setting: SnowSetting,
    texture: Option<TextureHandle>,
    mesh: Option<MeshHandle>,
    capacity: usize,
    heights: Vec<f32>,
    scratch: MeshData,
    positions: Vec<Vec3>,
    visible: bool,
    drawn: usize,
    spell: SpellRain,
    last_seconds: Option<f32>,
    clock: SnowClock,
    wanted: bool,
}

impl SnowScene {
    pub fn new(
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        setting: SnowSetting,
    ) -> Self {
        let heights = start_heights(setting.box_height);
        let mut scene = Self {
            setting,
            texture: None,
            mesh: None,
            capacity: 0,
            heights,
            scratch: MeshData::new(),
            positions: Vec::new(),
            visible: true,
            drawn: 0,
            spell: SpellRain::new(),
            last_seconds: None,
            clock: SnowClock::default(),
            wanted: false,
        };
        if !scene.setting.enabled || !(scene.setting.snowing || scene.setting.spell_enabled) {
            return scene;
        }
        let compressed_ok = renderer.supports_block_compression();
        scene.texture =
            mint_sage_model::load_texture(vfs, resolver, &scene.setting.texture, compressed_ok)
                .ok()
                .and_then(|loaded| renderer.upload_texture(&loaded.data).ok());
        scene.wanted = true;
        scene
    }

    #[must_use]
    pub fn setting(&self) -> &SnowSetting {
        &self.setting
    }

    #[must_use]
    pub fn falls(&self) -> bool {
        self.setting.falls()
    }

    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    #[must_use]
    pub fn textured(&self) -> bool {
        self.texture.is_some()
    }

    #[must_use]
    pub fn drawn(&self) -> usize {
        self.drawn
    }

    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera,
        seconds: f32,
        raining: bool,
        extent: [f32; 2],
        out: &mut Vec<Instance>,
    ) -> mint_render::Result<()> {
        self.drawn = 0;
        let dt = self
            .last_seconds
            .map_or(0.0, |last| (seconds - last).max(0.0));
        self.last_seconds = Some(seconds);
        self.spell.update(&self.setting, raining, dt);
        let spell = self.spell.current(&self.setting);
        let (spacing, speed) = spell.unwrap_or((self.setting.spacing, self.setting.speed));
        self.clock
            .advance(dt, cycle_seconds(&self.setting), speed, self.setting.drift);
        let falling = self.setting.falls() || spell.is_some();
        if self.mesh.is_none() && self.wanted {
            self.wanted = false;
            self.capacity = snow_tiling(&self.setting, extent, densest_spacing(&self.setting))
                .map_or(0, |tiling| tiling.count);
            let batch = quad_batch(self.capacity);
            self.mesh = renderer.upload_mesh(&batch).ok();
            self.scratch = batch;
        }
        let Some(mesh) = self.mesh.filter(|_| self.visible && falling) else {
            return Ok(());
        };
        let Some(tiling) = snow_tiling(&self.setting, extent, spacing) else {
            return Ok(());
        };
        let axes = camera_axes(camera);
        tile_flakes(
            &self.setting,
            &self.heights,
            &tiling,
            &self.clock,
            speed,
            &mut self.positions,
        );
        fill_flakes(
            &mut self.scratch,
            &self.positions,
            self.capacity,
            axes,
            self.setting.quad_size,
        );
        renderer.update_mesh_vertices(mesh, &self.scratch)?;
        let per_tile = self.positions.len().min(self.capacity);
        for (column, row) in tiling.offsets() {
            let mut flakes = Instance::new(mesh);
            flakes.transform = Mat4::from_translation(Vec3::new(column, row, 0.0));
            flakes.texture = self.texture;
            flakes.blend = Blend::AlphaBlend;
            flakes.unlit = true;
            out.push(flakes);
            self.drawn = self.drawn.saturating_add(per_tile);
        }
        Ok(())
    }
}

pub const WEATHER_INI: &str = "data/ini/weather.ini";

#[must_use]
pub fn load_snow_setting(vfs: &Vfs, map_path: &str) -> (SnowSetting, String) {
    let mut setting = SnowSetting::default();
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let map_ini = format!("{folder}/map.ini");
    let mut read = Vec::new();
    for path in [WEATHER_INI, map_ini.as_str()] {
        let Ok(bytes) = vfs.read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let table = mint_sage_ini::BlockTable::corpus_path();
        let Ok(document) = mint_sage_ini::Document::parse(path, &text, &table) else {
            read.push(format!("{path} (did not parse)"));
            continue;
        };
        let mut blocks = 0usize;
        for block in document.roots_named("Weather") {
            setting.apply(block);
            blocks = blocks.saturating_add(1);
        }
        read.push(format!("{path} ({blocks} Weather block(s))"));
    }
    let line = format!(
        "snow: {} - SnowEnabled {}, IsSnowing {}, texture {}, box {} spacing {} ({:?} tiles a side), \
         speed {}, drift {:?}, quad {} [from {}]",
        if setting.falls() { "falling" } else { "none" },
        setting.enabled,
        setting.snowing,
        setting.texture,
        setting.box_height,
        setting.spacing,
        setting.tiles,
        setting.speed,
        setting.drift,
        setting.quad_size,
        if read.is_empty() {
            "no weather file".to_owned()
        } else {
            read.join(", ")
        }
    );
    (setting, line)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnowTiling {
    pub tiles: u32,
    pub tile: [f32; 2],
    pub spacing: f32,
    pub count: usize,
}

impl SnowTiling {
    pub fn offsets(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        (0..self.tiles).flat_map(move |row| {
            (0..self.tiles).map(move |column| {
                #[allow(clippy::cast_precision_loss)]
                let (column, row) = (column as f32, row as f32);
                (column * self.tile[0], row * self.tile[1])
            })
        })
    }
}

#[must_use]
pub fn snow_tiling(setting: &SnowSetting, extent: [f32; 2], spacing: f32) -> Option<SnowTiling> {
    #[allow(clippy::cast_possible_truncation)]
    let whole = |value: f32| {
        value
            .is_finite()
            .then(|| value.clamp(0.0, 1.0e6).trunc() as i64)
    };
    let tiles = setting
        .tiles
        .and_then(whole)
        .unwrap_or(1)
        .clamp(1, MAX_TILES);
    let tile = [whole(extent[0])? / tiles, whole(extent[1])? / tiles];
    if tile[0] <= 0 || tile[1] <= 0 {
        return None;
    }
    let spacing = whole(spacing).unwrap_or(1).max(1);
    let count = tile[0]
        .saturating_mul(tile[1])
        .checked_div(spacing.saturating_mul(spacing))
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or(0)
        .min(MAX_FLAKES_PER_TILE);
    #[allow(clippy::cast_precision_loss)]
    Some(SnowTiling {
        tiles: u32::try_from(tiles).unwrap_or(1),
        tile: [tile[0] as f32, tile[1] as f32],
        spacing: spacing as f32,
        count,
    })
}

fn densest_spacing(setting: &SnowSetting) -> f32 {
    let mut spacing = setting.spacing;
    if setting.spell_enabled {
        spacing = spacing
            .min(setting.ramp_spacing[0])
            .min(setting.ramp_spacing[1]);
    }
    spacing.max(mint_sage_ini::weather::MIN_SNOW_SPACING)
}

fn start_heights(box_height: f32) -> Vec<f32> {
    (0..SNOW_NOISE * SNOW_NOISE)
        .map(|slot| {
            let mut hash: u32 = 0x811c_9dc5;
            for byte in u32::try_from(slot).unwrap_or(0).to_le_bytes() {
                hash ^= u32::from(byte);
                hash = hash.wrapping_mul(0x0100_0193);
            }
            #[allow(clippy::cast_precision_loss)]
            let unit = (hash >> 8) as f32 / (1u32 << 24) as f32;
            unit * box_height.max(0.0)
        })
        .collect()
}

fn camera_axes(camera: &Camera) -> (Vec3, Vec3) {
    let forward = camera.forward();
    let right = forward.cross(Vec3::Z).normalize();
    let right = if right.length_squared() > 0.5 {
        right
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let up = right.cross(forward).normalize();
    (right, up)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SnowClock {
    time: f32,
    fallen: f32,
    drift: [f32; 2],
}

impl SnowClock {
    pub fn advance(&mut self, dt: f32, period: f32, speed: f32, drift: [f32; 2]) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let old = self.time;
        let time = old + dt;
        self.time = if period.is_finite() && period > 0.0 {
            time.rem_euclid(period)
        } else {
            time
        };
        let step = self.time - old;
        self.fallen += step * speed;
        self.drift[0] += step * drift[0];
        self.drift[1] += step * drift[1];
    }

    #[must_use]
    pub fn fallen(&self) -> f32 {
        self.fallen
    }

    #[must_use]
    pub fn drift(&self) -> [f32; 2] {
        self.drift
    }
}

#[must_use]
pub fn cycle_seconds(setting: &SnowSetting) -> f32 {
    if setting.speed > 0.0 {
        setting.box_height.max(1.0) / setting.speed
    } else {
        f32::INFINITY
    }
}

pub fn tile_flakes(
    setting: &SnowSetting,
    heights: &[f32],
    tiling: &SnowTiling,
    clock: &SnowClock,
    speed: f32,
    out: &mut Vec<Vec3>,
) {
    out.clear();
    let fallen = clock.fallen();
    if !fallen.is_finite() {
        return;
    }
    let depth = setting.box_height.max(1.0);
    let drift = clock.drift();
    let fall_seconds = if speed > 0.0 { depth / speed } else { 0.0 };
    let per_wrap = [
        setting.drift[0] * fall_seconds,
        setting.drift[1] * fall_seconds,
    ];
    let mut y = 0.0f32;
    while y < tiling.tile[1] && out.len() < tiling.count {
        let mut x = 0.0f32;
        while x < tiling.tile[0] && out.len() < tiling.count {
            let start = heights.get(noise_slot(x, y)).copied().unwrap_or(0.0);
            let travelled = start + fallen;
            let wraps = (travelled / depth).floor();
            let h = depth - (travelled - wraps * depth);
            let sway_x = setting.amplitude * mint_math::f32::sin(h * setting.frequency[0] + x);
            let sway_y = setting.amplitude * mint_math::f32::sin(h * setting.frequency[1] + y);
            out.push(Vec3::new(
                x + drift[0] - wraps * per_wrap[0] + sway_x,
                y + drift[1] - wraps * per_wrap[1] + sway_y,
                h,
            ));
            x += tiling.spacing;
        }
        y += tiling.spacing;
    }
}

fn noise_slot(x: f32, y: f32) -> usize {
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (x as i64, y as i64);
    let noise = i64::try_from(SNOW_NOISE).unwrap_or(64);
    let slot = (y - 32).rem_euclid(noise) * noise + (x - 32).rem_euclid(noise);
    usize::try_from(slot).unwrap_or(0)
}

fn quad_batch(capacity: usize) -> MeshData {
    let mut mesh = MeshData::new();
    mesh.vertices =
        vec![Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]); capacity.saturating_mul(4)];
    mesh.indices.reserve(capacity.saturating_mul(6));
    for flake in 0..capacity {
        let base = u32::try_from(flake.saturating_mul(4)).unwrap_or(0);
        for offset in [3u32, 0, 2, 2, 0, 1] {
            mesh.indices.push(base.saturating_add(offset));
        }
    }
    mesh
}

fn fill_flakes(
    mesh: &mut MeshData,
    positions: &[Vec3],
    capacity: usize,
    (right, up): (Vec3, Vec3),
    size: f32,
) {
    const CORNERS: [([f32; 2], [f32; 2]); 4] = [
        ([-0.5, 0.5], [0.0, 0.0]),
        ([-0.5, -0.5], [0.0, 1.0]),
        ([0.5, -0.5], [1.0, 1.0]),
        ([0.5, 0.5], [1.0, 0.0]),
    ];
    let normal = right.cross(up).to_array();
    for (slot, vertex) in mesh.vertices.iter_mut().enumerate() {
        let flake = slot / 4;
        let corner = slot % 4;
        let (Some(centre), Some((offset, uv))) = (
            positions.get(flake).filter(|_| flake < capacity),
            CORNERS.get(corner),
        ) else {
            *vertex = Vertex::new([0.0; 3], normal, [0.0; 2]);
            continue;
        };
        let (dx, dy) = (offset[0] * size, offset[1] * size);
        *vertex = Vertex::new(
            [
                centre.x + right.x * dx + up.x * dy,
                centre.y + right.y * dx + up.y * dy,
                centre.z + right.z * dx + up.z * dy,
            ],
            normal,
            *uv,
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;

    fn arnor_like() -> SnowSetting {
        SnowSetting {
            enabled: true,
            snowing: true,
            box_height: 500.0,
            spacing: 60.0,
            amplitude: 0.0,
            speed: 120.0,
            ..SnowSetting::default()
        }
    }

    const AXES: (Vec3, Vec3) = (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));

    fn tiled(tiles: f32) -> SnowSetting {
        SnowSetting {
            tiles: Some(tiles),
            ..arnor_like()
        }
    }

    #[test]
    fn the_extent_is_cut_into_number_tiles_whole_tiles() {
        let tiling = snow_tiling(&tiled(2.0), [1001.0, 700.0], 100.0).unwrap();
        assert_eq!(tiling.tile, [500.0, 350.0]);
        assert_eq!(tiling.count, 17);
        let offsets: Vec<(f32, f32)> = tiling.offsets().collect();
        assert_eq!(
            offsets,
            vec![(0.0, 0.0), (500.0, 0.0), (0.0, 350.0), (500.0, 350.0)]
        );
        let setting = tiled(2.0);
        let heights = start_heights(setting.box_height);
        let mut out = Vec::new();
        tile_flakes(
            &setting,
            &heights,
            &tiling,
            &SnowClock::default(),
            setting.speed,
            &mut out,
        );
        assert_eq!(out.len(), 17);
        assert!(out.iter().all(|f| f.x < 500.0 && f.y < 350.0));
        assert!(snow_tiling(&tiled(0.0), [1000.0, 700.0], 100.0).is_some_and(|t| t.tiles == 1));
        assert!(snow_tiling(&tiled(2.0), [1.0, 700.0], 100.0).is_none());
        assert!(snow_tiling(&tiled(2.0), [f32::NAN, 700.0], 100.0).is_none());
    }

    #[test]
    fn a_flake_falls_and_wraps_inside_the_box_from_the_ground_datum() {
        let setting = tiled(1.0);
        let heights = start_heights(setting.box_height);
        let tiling = snow_tiling(&setting, [600.0, 600.0], setting.spacing).unwrap();
        let mut before = Vec::new();
        let mut after = Vec::new();
        let at = |fallen: f32| SnowClock {
            fallen,
            ..SnowClock::default()
        };
        let speed = setting.speed;
        tile_flakes(&setting, &heights, &tiling, &at(1200.0), speed, &mut before);
        tile_flakes(&setting, &heights, &tiling, &at(1212.0), speed, &mut after);
        assert_eq!(before.len(), 100);
        assert!(before
            .iter()
            .all(|f| f.z > 0.0 && f.z <= setting.box_height));
        let fell: Vec<f32> = before.iter().zip(&after).map(|(a, b)| a.z - b.z).collect();
        assert!(fell.iter().any(|d| (d - 12.0).abs() < 1e-2));
        assert!(fell.iter().all(|d| (d - 12.0).abs() < 1e-2 || *d < -400.0));
        let mut zs: Vec<f32> = before.iter().map(|f| f.z).collect();
        zs.sort_by(f32::total_cmp);
        zs.dedup();
        assert!(zs.len() > 32, "the start table should scatter the heights");
        tile_flakes(
            &setting,
            &heights,
            &tiling,
            &at(f32::INFINITY),
            speed,
            &mut before,
        );
        assert!(before.is_empty());
    }

    #[test]
    fn the_drift_winds_back_with_the_fall_and_never_leaves_the_extent() {
        let setting = SnowSetting {
            drift: [40.0, 25.0],
            ..tiled(1.0)
        };
        let heights = start_heights(setting.box_height);
        let tiling = snow_tiling(&setting, [600.0, 600.0], setting.spacing).unwrap();
        let period = cycle_seconds(&setting);
        let reach = 40.0 * period + 1e-2;
        let mut clock = SnowClock::default();
        let mut flakes = Vec::new();
        for _ in 0..(600 * 30) {
            clock.advance(1.0 / 30.0, period, setting.speed, setting.drift);
        }
        tile_flakes(
            &setting,
            &heights,
            &tiling,
            &clock,
            setting.speed,
            &mut flakes,
        );
        assert!(flakes
            .iter()
            .all(|f| f.x > -reach && f.x < tiling.tile[0] + reach));
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut clock = SnowClock::default();
        clock.advance(period - 0.01, period, setting.speed, setting.drift);
        tile_flakes(
            &setting,
            &heights,
            &tiling,
            &clock,
            setting.speed,
            &mut before,
        );
        clock.advance(0.02, period, setting.speed, setting.drift);
        tile_flakes(
            &setting,
            &heights,
            &tiling,
            &clock,
            setting.speed,
            &mut after,
        );
        let smooth = before
            .iter()
            .zip(&after)
            .filter(|(a, b)| b.z < a.z)
            .all(|(a, b)| (b.x - a.x - 40.0 * 0.02).abs() < 1e-2);
        assert!(smooth);
    }

    #[test]
    fn spare_slots_in_the_batch_draw_nothing() {
        let mut mesh = quad_batch(4);
        assert_eq!(mesh.validate(), None);
        fill_flakes(&mut mesh, &[Vec3::new(5.0, 5.0, 5.0)], 4, AXES, 2.0);
        let first: Vec<[f32; 3]> = mesh.vertices[..4].iter().map(|v| v.position).collect();
        assert!(first.iter().any(|p| p[0] != 5.0));
        assert!(mesh.vertices[4..].iter().all(|v| v.position == [0.0; 3]));
    }

    #[test]
    fn the_ramp_thickens_holds_and_thins_across_the_spell() {
        let control = [0.2, 0.8];
        let spacing = [30.0, 20.0];
        let speed = [150.0, 250.0];
        let close = |got: Option<(f32, f32)>, want: (f32, f32)| {
            let (gs, gv) = got.unwrap();
            assert!(
                (gs - want.0).abs() < 1e-3 && (gv - want.1).abs() < 1e-3,
                "{got:?} != {want:?}"
            );
        };
        close(ramp_step(0.1, control, spacing, speed), (25.0, 200.0));
        assert_eq!(ramp_step(0.5, control, spacing, speed), Some((20.0, 250.0)));
        close(ramp_step(0.9, control, spacing, speed), (25.0, 200.0));
        assert_eq!(ramp_step(1.5, control, spacing, speed), None);
    }

    fn rainy_setting() -> SnowSetting {
        SnowSetting {
            spell_enabled: true,
            spell_duration: 100.0,
            ramp_control: [0.2, 0.8],
            ramp_spacing: [30.0, 20.0],
            ramp_speed: [150.0, 250.0],
            ..SnowSetting::default()
        }
    }

    fn frames(n: u16) -> f32 {
        f32::from(n) * crate::client_clock::CLIENT_FRAME_SECONDS
    }

    #[test]
    fn the_rain_holds_while_the_kind_stays_rainy() {
        let setting = rainy_setting();
        let mut spell = SpellRain::new();
        spell.update(&setting, true, frames(500));
        assert!(spell.active());
        assert_eq!(spell.current(&setting), Some((20.0, 250.0)));
    }

    #[test]
    fn leaving_rainy_fades_out_over_the_last_part_of_the_ramp() {
        let setting = rainy_setting();
        let mut spell = SpellRain::new();
        spell.update(&setting, true, frames(50));
        spell.update(&setting, false, frames(1));
        let (spacing, speed) = spell.current(&setting).unwrap();
        assert!(
            spacing < 21.0 && speed > 240.0,
            "fade starts from the hold: {spacing} {speed}"
        );
        spell.update(&setting, false, frames(10));
        let (spacing, speed) = spell.current(&setting).unwrap();
        assert!(
            spacing > 20.0 && speed < 250.0,
            "thinning: {spacing} {speed}"
        );
        spell.update(&setting, false, frames(12));
        assert!(!spell.active(), "the fade has run out");
        spell.update(&setting, false, frames(5));
        assert!(!spell.active());
    }

    #[test]
    fn a_kind_other_than_rainy_never_starts_the_ramp() {
        let setting = rainy_setting();
        let mut spell = SpellRain::new();
        spell.update(&setting, false, 10.0);
        assert!(!spell.active());
    }
}
