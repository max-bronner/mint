use std::collections::BTreeMap;

use mint_render::{
    Blend, Instance, Mat4, MeshData, MeshHandle, Renderer, TextureHandle, UiQuad, Vec3, Vertex,
};

use crate::render_state::{ProjectileView, UnitView};
use mint_sage_asset::PickType;

const SEGMENTS: usize = 16;

const COS_STEP: f32 = 0.923_879_5;
const SIN_STEP: f32 = 0.382_683_4;

const RING_INNER: f32 = 0.78;

const RING_LIFT: f32 = 0.6;

const MERGE_MASK_CUTOFF: f32 = 96.0 / 255.0;

pub(crate) const MOVE_HINT_LIFETIME_FRAMES: u32 = 40;

fn is_mobile(unit: &UnitView) -> bool {
    !unit.kind_of.contains(mint_sage_asset::KindOf::IMMOBILE) && !unit.held
}

pub(crate) fn shows_move_hint(units: &[UnitView], fogged: bool) -> bool {
    let selected: Vec<&UnitView> = units.iter().filter(|unit| unit.selected).collect();
    match selected.as_slice() {
        [] => false,
        [only] => fogged || is_mobile(only),
        _ => fogged || selected.iter().copied().any(is_mobile),
    }
}

const PROJECTILE_SIZE: f32 = 3.6;

pub(crate) const MAX_STREAK_POINTS: usize = 33;

#[derive(Debug, Clone, Copy)]
pub(crate) struct StreakView {
    pub(crate) points: [Vec3; MAX_STREAK_POINTS],
    pub(crate) count: usize,
    pub(crate) width: f32,
    pub(crate) tint: [f32; 4],
    pub(crate) texture: Option<mint_render::TextureHandle>,
    pub(crate) additive: bool,
}

pub(crate) struct UnitVisuals {
    body: MeshHandle,
    ring: MeshHandle,
    bar: MeshHandle,
    streak_meshes: Vec<MeshHandle>,
    streak_scratch: MeshData,
    pub(crate) ghosts: crate::ghosts::GhostMemory,
    pub(crate) bar_style: HealthBarStyle,
    pub(crate) bar_quads: Vec<UiQuad>,
    pub(crate) placement_ghost: Option<PlacementGhost>,
    pub(crate) span_ghosts: Vec<PlacementGhost>,
    pub(crate) formation_preview: Vec<crate::rules::FormationPreviewItem>,
    pub(crate) formation_arrow: Vec<crate::rules::ArrowPiece>,
    decal_textures: BTreeMap<String, Option<TextureHandle>>,
    ground_textures: BTreeMap<String, Option<TextureHandle>>,
    pub(crate) radius_cursor: Option<RadiusCursorDraw>,
    pub(crate) claim_ring: Option<RadiusCursorDraw>,
    pub(crate) claim_outline: (
        Vec<crate::game::claim_outline::ClaimCircle>,
        Vec<crate::game::claim_outline::Segment>,
    ),
    claim_disc: Option<FadedDisc>,
    pub(crate) selection_claims: usize,
    pub(crate) fading_decals: crate::game::decals::FadingDecals,
    pub(crate) shadow_decals: Option<bool>,
    pub(crate) claim_share: Option<ClaimShareLabel>,
    label_atlas: Option<mint_ui::GlyphAtlas>,
    label_texture: Option<TextureHandle>,
    label_page: Option<TextureHandle>,
    labelled: usize,
}

struct FadedDisc {
    texture: String,
    base: mint_render::MipLevel,
    handle: TextureHandle,
    drawn: Option<((f32, f32, f32), mint_sage_passability::claims::Fade)>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DecalView {
    pub(crate) centre: Vec3,
    pub(crate) half_x: f32,
    pub(crate) half_y: f32,
    pub(crate) facing: f32,
    pub(crate) texture: TextureHandle,
    pub(crate) tint: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RadiusCursor {
    pub(crate) texture: String,
    pub(crate) radius: f32,
    pub(crate) opacity_min: f32,
    pub(crate) opacity_max: f32,
    pub(crate) throb_ms: u32,
}

pub(crate) const RADIUS_CURSOR_VALID_RGB: [f32; 3] = [1.0, 1.0, 1.0];
pub(crate) const RADIUS_CURSOR_INVALID_RGB: [f32; 3] = [1.0, 20.0 / 255.0, 20.0 / 255.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RadiusCursorDraw {
    pub(crate) centre: Vec3,
    pub(crate) radius: f32,
    pub(crate) texture: Option<TextureHandle>,
    pub(crate) tint: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClaimShareLabel {
    pub(crate) at: Vec3,
    pub(crate) percent: u32,
}

impl ClaimShareLabel {
    pub(crate) const fn colour(self) -> [f32; 4] {
        if self.percent >= 75 {
            [0.0, 1.0, 0.0, 1.0]
        } else if self.percent >= 50 {
            [1.0, 1.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0, 1.0]
        }
    }
}

impl RadiusCursor {
    pub(crate) fn of(
        button: &str,
        buttons: &mint_sage_ini::CommandButtonIndex,
        powers: &mint_sage_ini::SpecialPowerIndex,
        templates: &mint_sage_ini::RadiusCursorIndex,
    ) -> Option<Self> {
        let def = buttons.get(button)?;
        let named = def.radius_cursor_type.as_deref()?;
        let template = templates.get(named)?;
        let texture = template.texture.clone()?;
        let radius = powers
            .get(def.special_power.as_deref()?)?
            .radius_cursor_radius?;
        (radius > 0.0).then_some(Self {
            texture,
            radius,
            opacity_min: template.opacity_min,
            opacity_max: template.opacity_max,
            throb_ms: template.opacity_throb_time_ms,
        })
    }

    pub(crate) fn opacity(&self, frame: u64, ticks: u32) -> f32 {
        throb_opacity(
            (self.opacity_min, self.opacity_max),
            self.throb_ms,
            frame,
            ticks,
        )
    }
}

pub(crate) fn throb_opacity((min, max): (f32, f32), throb_ms: u32, frame: u64, ticks: u32) -> f32 {
    let numerator = u64::from(throb_ms).saturating_mul(u64::from(ticks));
    let period = numerator
        .checked_add(999)
        .and_then(|rounded| rounded.checked_div(1000))
        .unwrap_or(1)
        .max(1);
    #[allow(clippy::cast_precision_loss)]
    let phase = (frame % period) as f32 / period as f32;
    let percent = 0.5 * (mint_math::f32::sin(std::f32::consts::TAU * phase) + 1.0);
    min + percent * (max - min)
}

pub(crate) fn decal_ring_alpha(
    outer: (&[u8], u32, u32),
    inner: Option<(&[u8], u32, u32)>,
) -> Vec<u8> {
    let (bytes, width, height) = outer;
    let (w, h) = (
        usize::try_from(width).unwrap_or(0),
        usize::try_from(height).unwrap_or(0),
    );
    let alpha_at = |data: &[u8], stride: usize, x: usize, y: usize| -> u32 {
        y.checked_mul(stride)
            .and_then(|row| row.checked_add(x))
            .and_then(|pixel| pixel.checked_mul(4))
            .and_then(|at| at.checked_add(3))
            .and_then(|at| data.get(at))
            .map_or(0, |a| u32::from(*a))
    };
    let mut out = Vec::with_capacity(w.saturating_mul(h).saturating_mul(4));
    for y in 0..h {
        for x in 0..w {
            let a = alpha_at(bytes, w, x, y);
            let cut = inner.map_or(0, |(data, iw, ih)| {
                let (iw, ih) = (
                    usize::try_from(iw).unwrap_or(0),
                    usize::try_from(ih).unwrap_or(0),
                );
                if iw == 0 || ih == 0 {
                    return 0;
                }
                let ix = x.saturating_mul(iw) / w.max(1);
                let iy = y.saturating_mul(ih) / h.max(1);
                alpha_at(data, iw, ix, iy)
            });
            let alpha = a.saturating_mul(255u32.saturating_sub(cut)) / 255;
            out.extend_from_slice(&[255, 255, 255, u8::try_from(alpha).unwrap_or(u8::MAX)]);
        }
    }
    out
}

fn decal_level(
    vfs: &mint_vfs::Vfs,
    resolver: &mint_sage_model::Resolver,
    name: &str,
) -> Option<mint_render::MipLevel> {
    let logical = if name.contains('.') {
        name.to_owned()
    } else {
        format!("{name}.tga")
    };
    let loaded = mint_sage_model::load_texture(vfs, resolver, &logical, false).ok()?;
    let level = loaded.data.levels.into_iter().next()?;
    (level.data.len()
        == usize::try_from(level.width)
            .ok()?
            .checked_mul(usize::try_from(level.height).ok()?)?
            .checked_mul(4)?)
    .then_some(level)
}

pub(crate) fn selection_decal_width(
    decal: &mint_sage_modules::SelectionDecal,
    count: u32,
) -> Option<f32> {
    if decal.texture.is_empty() {
        return None;
    }
    let min = decal.min_radius.filter(|r| *r > 0.0)?;
    let max = decal.max_radius.filter(|r| *r > 0.0)?;
    if count == 0 {
        return None;
    }
    let max_selected = decal.max_selected.unwrap_or(1.0);
    #[allow(clippy::cast_precision_loss)]
    let grown = min + (max - min) / max_selected * (count - 1) as f32;
    Some(grown.min(max) / 2.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacementGhost {
    pub(crate) kind: u16,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) facing: f32,
    pub(crate) code: u8,
    pub(crate) span_code: Option<u8>,
}

pub(crate) const GHOST_OPACITY: f32 = 0.45;

fn blink_on(frame: u64) -> bool {
    frame & 4 != 0
}

const ILLEGAL_RED: [f32; 3] = [1.0, 0.0, 0.0];

#[must_use]
pub(crate) fn ghost_tint(code: u8, frame: u64) -> Option<[f32; 3]> {
    match code {
        0 => None,
        9 => Some([0.0, 0.0, if blink_on(frame) { 1.7 } else { 0.5 }]),
        _ => Some(ILLEGAL_RED),
    }
}

#[must_use]
pub(crate) fn span_piece_tint(span: u8, piece: u8, frame: u64) -> Option<[f32; 3]> {
    if span == 0 {
        return None;
    }
    if piece == 0 {
        return Some(ILLEGAL_RED);
    }
    let on = blink_on(frame);
    Some(match span {
        7 => [0.0, if on { 1.0 } else { 0.0 }, 0.0],
        9 => [0.0, 0.0, if on { 1.7 } else { 0.0 }],
        10 => [0.5, if on { 1.3 } else { 0.5 }, 0.5],
        _ => [1.0, if on { 0.8 } else { 0.0 }, 0.0],
    })
}

pub(crate) fn ghost_look(ghost: &PlacementGhost, frame: u64) -> mint_sage_scene::GhostLook {
    let tint = match ghost.span_code {
        Some(span) => span_piece_tint(span, ghost.code, frame),
        None => ghost_tint(ghost.code, frame),
    };
    mint_sage_scene::GhostLook {
        opacity: GHOST_OPACITY,
        tint: [1.0, 1.0, 1.0],
        light_add: tint.unwrap_or([0.0, 0.0, 0.0]),
        state_particles: false,
    }
}

pub(crate) fn plan_look() -> mint_sage_scene::GhostLook {
    mint_sage_scene::GhostLook {
        opacity: GHOST_OPACITY,
        tint: [1.0, 1.0, 1.0],
        light_add: [0.0, 0.0, 0.0],
        state_particles: true,
    }
}

pub(crate) const PLACEMENT_CONDITION: &str = "BUILD_PLACEMENT_CURSOR";

pub(crate) fn placement_look(scene: &mut mint_sage_scene::UnitScene) -> mint_sage_scene::LookKey {
    scene.intern_look(mint_sage_scene::UnitLook {
        conditions: vec![PLACEMENT_CONDITION.into()],
        ..mint_sage_scene::UnitLook::default()
    })
}

pub(crate) const RALLY_MARKER_TEMPLATE: &str = "RallyPointMarker";

pub(crate) const RALLY_FLAG_ID: u32 = u32::MAX - (2 << 20);

pub(crate) const PLACEMENT_GHOST_ID: u32 = u32::MAX;

pub(crate) const PLANTED_TREE_ID: u32 = u32::MAX - (1 << 20);

impl UnitVisuals {
    pub(crate) fn upload(renderer: &mut Renderer) -> Result<Self, String> {
        Ok(Self {
            body: renderer
                .upload_mesh(&body_mesh())
                .map_err(|e| e.to_string())?,
            ring: renderer
                .upload_mesh(&ring_mesh())
                .map_err(|e| e.to_string())?,
            bar: renderer
                .upload_mesh(&quad_mesh())
                .map_err(|e| e.to_string())?,
            streak_meshes: Vec::new(),
            streak_scratch: MeshData {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            ghosts: crate::ghosts::GhostMemory::default(),
            bar_style: HealthBarStyle::default(),
            bar_quads: Vec::new(),
            placement_ghost: None,
            span_ghosts: Vec::new(),
            formation_preview: Vec::new(),
            formation_arrow: Vec::new(),
            decal_textures: BTreeMap::new(),
            ground_textures: BTreeMap::new(),
            radius_cursor: None,
            claim_ring: None,
            claim_outline: (Vec::new(), Vec::new()),
            claim_disc: None,
            selection_claims: 0,
            fading_decals: crate::game::decals::FadingDecals::default(),
            shadow_decals: None,
            claim_share: None,
            label_atlas: mint_ui::GlyphAtlas::new(mint_ui::GlyphAtlasConfig::default_config()).ok(),
            label_texture: mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
                .and_then(|data| renderer.upload_texture(&data).ok()),
            label_page: None,
            labelled: 0,
        })
    }

    pub(crate) fn decal_texture(
        &mut self,
        renderer: &mut Renderer,
        vfs: &mint_vfs::Vfs,
        resolver: &mint_sage_model::Resolver,
        texture: &str,
        cut_out: Option<&str>,
    ) -> Option<TextureHandle> {
        let key = format!(
            "{}|{}",
            texture.to_ascii_lowercase(),
            cut_out.unwrap_or_default().to_ascii_lowercase()
        );
        if let Some(held) = self.decal_textures.get(&key) {
            return *held;
        }
        let rgba = |name: &str| decal_level(vfs, resolver, name);
        let handle = rgba(texture).and_then(|outer| {
            let inner = cut_out.and_then(rgba);
            let pixels = decal_ring_alpha(
                (&outer.data, outer.width, outer.height),
                inner.as_ref().map(|i| (&i.data[..], i.width, i.height)),
            );
            let mut data = mint_render::TextureData::rgba8(outer.width, outer.height, pixels)?
                .with_addressing(
                    mint_render::AddressMode::Clamp,
                    mint_render::AddressMode::Clamp,
                );
            data.generate_mipmaps(16);
            renderer.upload_texture(&data).ok()
        });
        println!(
            "render: selection decal {texture}{} {}",
            cut_out.map(|c| format!(" less {c}")).unwrap_or_default(),
            if handle.is_some() {
                "loaded"
            } else {
                "did not load - the ring stands in"
            }
        );
        self.decal_textures.insert(key, handle);
        handle
    }

    pub(crate) fn ground_texture(
        &mut self,
        renderer: &mut Renderer,
        vfs: &mint_vfs::Vfs,
        resolver: &mint_sage_model::Resolver,
        texture: &str,
    ) -> Option<TextureHandle> {
        if let Some(held) = self.ground_textures.get(texture) {
            return *held;
        }
        let key = format!("ground|{}", texture.to_ascii_lowercase());
        if let Some(held) = self.decal_textures.get(&key).copied() {
            self.ground_textures.insert(texture.to_owned(), held);
            return held;
        }
        let handle = decal_level(vfs, resolver, texture).and_then(|level| {
            let mut data = mint_render::TextureData::rgba8(level.width, level.height, level.data)?
                .with_addressing(
                    mint_render::AddressMode::Clamp,
                    mint_render::AddressMode::Clamp,
                );
            data.generate_mipmaps(16);
            renderer.upload_texture(&data).ok()
        });
        println!(
            "render: ground decal {texture} {}",
            if handle.is_some() {
                "loaded"
            } else {
                "did not load - nothing is drawn"
            }
        );
        self.decal_textures.insert(key, handle);
        self.ground_textures.insert(texture.to_owned(), handle);
        handle
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn faded_claim_texture(
        &mut self,
        renderer: &mut Renderer,
        vfs: &mint_vfs::Vfs,
        resolver: &mint_sage_model::Resolver,
        texture: &str,
        centre: mint_movement::Vec2,
        radius: f32,
        fade: &mint_sage_passability::claims::Fade,
    ) -> Option<TextureHandle> {
        if self
            .claim_disc
            .as_ref()
            .is_none_or(|disc| !disc.texture.eq_ignore_ascii_case(texture))
        {
            let base = decal_level(vfs, resolver, texture)?;
            let data = mint_render::TextureData::rgba8(base.width, base.height, base.data.clone())?
                .with_addressing(
                    mint_render::AddressMode::Clamp,
                    mint_render::AddressMode::Clamp,
                );
            let handle = renderer.upload_texture(&data).ok()?;
            self.claim_disc = Some(FadedDisc {
                texture: texture.to_owned(),
                base,
                handle,
                drawn: None,
            });
        }
        let disc = self.claim_disc.as_mut()?;
        let key = (centre.x, centre.y, radius);
        if disc
            .drawn
            .as_ref()
            .is_some_and(|(held, faded)| *held == key && faded == fade)
        {
            return Some(disc.handle);
        }
        let (width, height) = (disc.base.width, disc.base.height);
        let mut pixels = disc.base.data.clone();
        let columns = usize::try_from(width).ok()?;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (width as f32, height as f32);
        for (i, texel) in pixels.chunks_exact_mut(4).enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let (col, row) = ((i % columns) as f32, (i / columns) as f32);
            let at = mint_movement::Vec2::new(
                centre.x - radius + (col + 0.5) / w * 2.0 * radius,
                centre.y - radius + (row + 0.5) / h * 2.0 * radius,
            );
            if let Some(alpha) = texel.get_mut(3) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let faded = (f32::from(*alpha) * fade.at(at)).round().clamp(0.0, 255.0) as u8;
                *alpha = faded;
            }
        }
        let data = mint_render::TextureData::rgba8(width, height, pixels)?;
        renderer.update_texture(disc.handle, &data).ok()?;
        disc.drawn = Some((key, fade.clone()));
        Some(disc.handle)
    }

    pub(crate) fn push_decals(&self, decals: &[DecalView], out: &mut Vec<Instance>) {
        self.push_decals_as(decals, Blend::AlphaBlend, out);
    }

    pub(crate) fn push_merge_masks(&self, decals: &[DecalView], out: &mut Vec<Instance>) {
        let first = out.len();
        self.push_decals_as(decals, Blend::MergeMask, out);
        for mask in out.iter_mut().skip(first) {
            mask.alpha_cutoff = MERGE_MASK_CUTOFF;
        }
    }

    pub(crate) fn push_decals_as(
        &self,
        decals: &[DecalView],
        blend: Blend,
        out: &mut Vec<Instance>,
    ) {
        for decal in decals {
            let mut quad = Instance::new(self.bar);
            quad.transform = oriented_quad_placement(
                decal.centre.x,
                decal.centre.y,
                decal.centre.z + RING_LIFT,
                decal.facing,
                decal.half_x * 2.0,
                decal.half_y * 2.0,
                1.0,
            );
            quad.texture = Some(decal.texture);
            quad.tint = decal.tint;
            quad.unlit = true;
            quad.blend = blend;
            out.push(quad);
        }
    }

    pub(crate) fn push(
        &self,
        units: &[UnitView],
        projectiles: &[ProjectileView],
        has_model: &dyn Fn(&UnitView) -> bool,
        out: &mut Vec<Instance>,
    ) {
        for unit in units {
            let masked = crate::game::shroud_masked(unit.visibility);
            if !has_model(unit) {
                let mut body = Instance::new(self.body);
                body.transform = placement(
                    unit.position.x,
                    unit.position.y,
                    unit.ground,
                    unit.facing,
                    unit.radius,
                    unit.height,
                );
                let tint = tint_for(unit);
                body.tint = tint;
                body.visibility_mask = masked;
                out.push(body);
            }
        }
        for shot in projectiles {
            let mut arrow = Instance::new(self.body);
            arrow.transform = placement(
                shot.x,
                shot.y,
                shot.z,
                0.0,
                PROJECTILE_SIZE,
                PROJECTILE_SIZE,
            );
            arrow.tint = [1.0, 0.95, 0.7, 1.0];
            arrow.unlit = true;
            out.push(arrow);
        }
    }

    pub(crate) fn push_move_hint(
        &self,
        model: Option<&std::sync::Arc<mint_sage_model::CachedModel>>,
        x: f32,
        y: f32,
        z: f32,
        seconds: f32,
        out: &mut Vec<Instance>,
    ) {
        match model {
            Some(model) => {
                let start = out.len();
                model.place(placement(x, y, z, 0.0, 1.0, 1.0), out);
                if let Some(placed) = out.get_mut(start..) {
                    model.apply_uv_mappers(1, placed, seconds);
                }
            }
            None => {
                let mut marker = Instance::new(self.ring);
                marker.transform = placement(x, y, z + RING_LIFT, 0.0, 12.0, 1.0);
                marker.tint = [1.0, 0.85, 0.2, 1.0];
                marker.unlit = true;
                out.push(marker);
            }
        }
    }
}

impl UnitVisuals {
    pub(crate) fn push_streaks(
        &mut self,
        renderer: &mut Renderer,
        streaks: &[StreakView],
        eye: Vec3,
        out: &mut Vec<Instance>,
    ) {
        let mut used = 0usize;
        for streak in streaks {
            let points = streak.points.get(..streak.count).unwrap_or(&[]);
            let Some(&head) = points.last() else {
                continue;
            };
            if points.len() < 2 {
                continue;
            }
            let mesh = match self.streak_meshes.get(used) {
                Some(&held) => held,
                None => match renderer
                    .upload_mesh(&mint_sage_particle::ribbon_mesh(MAX_STREAK_POINTS))
                {
                    Ok(mesh) => {
                        self.streak_meshes.push(mesh);
                        mesh
                    }
                    Err(_) => return,
                },
            };
            let to_eye = eye - head;
            let view = if to_eye.length_squared() > 1.0e-8 {
                to_eye.normalize()
            } else {
                Vec3::new(0.0, 0.0, 1.0)
            };
            let half_width = streak.width * 0.5;
            let bounds = mint_sage_particle::fill_ribbon_points(
                &mut self.streak_scratch,
                points.len(),
                MAX_STREAK_POINTS,
                head,
                view,
                |slot| {
                    points
                        .get(slot)
                        .map(|&position| mint_sage_particle::RibbonPoint {
                            position,
                            half_width,
                            color: [1.0; 4],
                        })
                },
            );
            if renderer
                .update_mesh_vertex_prefix(mesh, &self.streak_scratch)
                .is_err()
            {
                continue;
            }
            used = used.saturating_add(1);
            let mut ribbon = Instance::new(mesh);
            ribbon.texture = streak.texture;
            ribbon.tint = streak.tint;
            ribbon.unlit = true;
            ribbon.cull = false;
            ribbon.bounds = Some(bounds);
            ribbon.blend = if streak.additive {
                Blend::Additive
            } else {
                Blend::AlphaBlend
            };
            out.push(ribbon);
        }
    }

    pub(crate) fn push_obstacle_overlay(
        &self,
        static_cells: &[Vec3],
        dynamic_cells: &[Vec3],
        cell_size: f32,
        out: &mut Vec<Instance>,
    ) {
        let mut push = |cells: &[Vec3], tint: [f32; 4]| {
            for cell in cells {
                let mut quad = Instance::new(self.bar);
                quad.transform = bar_placement(
                    cell.x,
                    cell.y,
                    cell.z + OVERLAY_LIFT,
                    cell_size,
                    cell_size,
                    1.0,
                );
                quad.tint = tint;
                quad.unlit = true;
                quad.blend = Blend::AlphaBlend;
                out.push(quad);
            }
        };
        push(static_cells, [1.0, 0.6, 0.0, 0.35]);
        push(dynamic_cells, [1.0, 0.05, 0.05, 0.55]);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_construction_labels(
        &mut self,
        renderer: &mut Renderer,
        units: &[UnitView],
        boxes: &dyn Fn(&UnitView) -> HealthBox,
        text_for: &dyn Fn(&UnitView) -> Option<String>,
        font: Option<&mint_ui::Font>,
        size_px: u32,
        view_proj: Mat4,
        viewport: (u32, u32),
        out: &mut Vec<UiQuad>,
    ) {
        let (Some(atlas), Some(texture)) = (self.label_atlas.as_mut(), self.label_texture) else {
            return;
        };
        let start = out.len();
        let mut labelled = 0usize;
        #[allow(clippy::cast_precision_loss)]
        let ascent = font
            .and_then(|font| font.metrics(size_px).ok())
            .map_or(size_px as f32 * 0.8, |metrics| metrics.win_ascent);
        for unit in units {
            if !is_actively_being_constructed(unit.conditions) {
                continue;
            }
            let Some(text) = text_for(unit) else {
                continue;
            };
            let lift = boxes(unit).lift;
            let point = Vec3::new(
                unit.position.x,
                unit.position.y,
                unit.ground + lift - lift * CONSTRUCTION_LABEL_DROP,
            );
            let Some((cx, cy)) = project_to_pixel(view_proj, point, viewport) else {
                continue;
            };
            labelled = labelled.saturating_add(1);
            let width = mint_ui::measure_text(&text, font, size_px);
            let x = cx - (width * 0.5).trunc();
            let y = cy + ascent;
            for (dx, dy, colour) in [
                (1.0, 1.0, [0.0, 0.0, 0.0, 1.0]),
                (0.0, 0.0, [1.0, 1.0, 1.0, 1.0]),
            ] {
                let quads = mint_ui::layout_text(
                    &text,
                    [x + dx, y + dy],
                    colour,
                    LABEL_FONT_ID,
                    font,
                    atlas,
                    size_px,
                    texture,
                );
                out.extend(quads);
            }
        }
        if labelled != self.labelled {
            self.labelled = labelled;
            crate::diag::diag!("construction: {labelled} site(s) labelled");
        }
        self.upload_label_atlas(renderer, out, start, texture);
    }

    pub(crate) fn push_floating_texts(
        &mut self,
        renderer: &mut Renderer,
        labels: &[crate::floating_text::Label<'_>],
        font: Option<&mint_ui::Font>,
        view_proj: Mat4,
        viewport: (u32, u32),
        out: &mut Vec<UiQuad>,
    ) {
        let (Some(atlas), Some(texture)) = (self.label_atlas.as_mut(), self.label_texture) else {
            return;
        };
        let start = out.len();
        for label in labels {
            let Some((cx, cy)) = project_to_pixel(view_proj, label.at, viewport) else {
                continue;
            };
            let width = mint_ui::measure_text(label.text, font, LABEL_SIZE_PX);
            let (x, y) = (cx - width * 0.5, cy - label.rise_px);
            let shadow = [0.0, 0.0, 0.0, label.colour[3]];
            for (dx, dy, colour) in [(1.0, 1.0, shadow), (0.0, 0.0, label.colour)] {
                out.extend(mint_ui::layout_text(
                    label.text,
                    [x + dx, y + dy],
                    colour,
                    LABEL_FONT_ID,
                    font,
                    atlas,
                    LABEL_SIZE_PX,
                    texture,
                ));
            }
        }
        self.upload_label_atlas(renderer, out, start, texture);
    }

    pub(crate) fn push_caption(
        &mut self,
        renderer: &mut Renderer,
        caption: &crate::military_caption::Caption,
        style: &crate::military_caption::CaptionStyle,
        font: Option<&mint_ui::Font>,
        viewport: (u32, u32),
        out: &mut Vec<UiQuad>,
    ) {
        let (Some(atlas), Some(texture)) = (self.label_atlas.as_mut(), self.label_texture) else {
            return;
        };
        let start = out.len();
        let size_px = caption_text_px(style.points, viewport.0);
        #[allow(clippy::cast_precision_loss)]
        let (width, height, line_height) =
            (viewport.0 as f32, viewport.1 as f32, size_px as f32 * 1.2);
        let [origin_x, mut y] = crate::military_caption::origin(style, width, height);
        let fade = caption.fade();
        let colour = |alpha: f32| {
            [
                style.colour[0],
                style.colour[1],
                style.colour[2],
                style.colour[3] * alpha,
            ]
        };
        let full = caption.full_lines();
        let mut draw = |text: &str, x: f32, y: f32, alpha: f32| {
            let shadow = [0.0, 0.0, 0.0, style.colour[3] * alpha];
            for (dx, dy, colour) in [(1.0, 1.0, shadow), (0.0, 0.0, colour(alpha))] {
                out.extend(mint_ui::layout_text(
                    text,
                    [x + dx, y + dy],
                    colour,
                    LABEL_FONT_ID,
                    font,
                    atlas,
                    size_px,
                    texture,
                ));
            }
        };
        let mut last_x = origin_x;
        for (index, line) in caption.lines().iter().enumerate() {
            let offset = if style.centered {
                -mint_ui::measure_text(full.get(index).map_or("", String::as_str), font, size_px)
                    * 0.5
            } else {
                0.0
            };
            last_x = origin_x + offset;
            if !line.is_empty() {
                draw(line, last_x, y, fade);
            }
            if index + 1 < caption.lines().len() {
                y += line_height;
            }
        }
        if let (Some((ch, progress)), Some(line)) = (caption.staged(), caption.lines().last()) {
            let x = last_x
                + mint_ui::measure_text(line, font, size_px)
                + crate::military_caption::Caption::slide_offset(progress);
            draw(&ch.to_string(), x, y, fade * progress);
        }
        self.upload_label_atlas(renderer, out, start, texture);
    }

    pub(crate) fn push_named_timers(
        &mut self,
        renderer: &mut Renderer,
        lines: &[(String, bool)],
        style: &crate::named_timer::TimerStyle,
        font: Option<&mint_ui::Font>,
        viewport: (u32, u32),
        out: &mut Vec<UiQuad>,
    ) {
        let (Some(atlas), Some(texture)) = (self.label_atlas.as_mut(), self.label_texture) else {
            return;
        };
        let start = out.len();
        let size_px = caption_text_px(style.points, viewport.0);
        #[allow(clippy::cast_precision_loss)]
        let (width, height, line_height) =
            (viewport.0 as f32, viewport.1 as f32, size_px as f32 * 1.2);
        let right_aligned = style.position[0] >= 0.5;
        let mut y = (height * style.position[1]).floor();
        for (text, ready) in lines {
            let colour = if *ready { style.ready } else { style.normal };
            let mut x = (width * style.position[0]).floor();
            if right_aligned {
                x -= mint_ui::measure_text(text, font, size_px);
            }
            for (dx, dy, colour) in [(1.0, 1.0, [0.0, 0.0, 0.0, 1.0]), (0.0, 0.0, colour)] {
                out.extend(mint_ui::layout_text(
                    text,
                    [x + dx, y + dy],
                    colour,
                    LABEL_FONT_ID,
                    font,
                    atlas,
                    size_px,
                    texture,
                ));
            }
            y -= line_height;
        }
        self.upload_label_atlas(renderer, out, start, texture);
    }

    fn upload_label_atlas(
        &mut self,
        renderer: &mut Renderer,
        out: &mut [UiQuad],
        start: usize,
        texture: mint_render::TextureHandle,
    ) {
        let Some(atlas) = self.label_atlas.as_mut() else {
            return;
        };
        if atlas.take_dirty() {
            if let Some(page) = mint_ui::sync_atlas_texture(atlas, &mut self.label_page, renderer) {
                self.label_texture = Some(page);
                for quad in out.iter_mut().skip(start) {
                    if quad.texture == Some(texture) {
                        quad.texture = Some(page);
                    }
                }
            }
        }
    }
}

const LABEL_FONT_ID: mint_ui::FontId = mint_ui::FontId(0);

const LABEL_SIZE_PX: u32 = 14;

pub(crate) fn caption_text_px(points: f32, window_width: u32) -> u32 {
    #[allow(clippy::cast_precision_loss)]
    let points = (points * window_width as f32 / 1024.0).floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = points.clamp(1.0, mint_ui::MAX_SIZE_PX as f32) as u32;
    px
}

const CONSTRUCTION_LABEL_DROP: f32 = 0.333;

const OVERLAY_LIFT: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HealthBarStyle {
    #[default]
    Health,
    Player,
}

impl HealthBarStyle {
    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Health => Self::Player,
            Self::Player => Self::Health,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Health => "health ramp (EA's drawHealthBar: green - yellow - red)",
            Self::Player => "player colour (a mod look, not retail)",
        }
    }

    pub(crate) const fn rung(self) -> u8 {
        match self {
            Self::Health => 1,
            Self::Player => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HealthBox {
    pub(crate) lift: f32,
    pub(crate) scale: f32,
}

pub(crate) const HEALTH_BOX_OFFSET_DEFAULT: f32 = 10.0;

const HEALTH_BOX_SCALE_MAX: f32 = 8.0;

impl HealthBox {
    pub(crate) fn of(
        record: Option<&mint_sage_modules::ObjectRecord>,
        fallback_height: f32,
    ) -> Self {
        let height = record
            .and_then(|record| record.geometry.health_box_height)
            .filter(|height| height.is_finite())
            .unwrap_or(fallback_height);
        let offset = record
            .and_then(|record| record.health_box_offset)
            .filter(|offset| offset.is_finite())
            .unwrap_or(HEALTH_BOX_OFFSET_DEFAULT);
        let scale = record
            .and_then(|record| record.health_box_scale)
            .filter(|scale| scale.is_finite() && *scale > 0.0)
            .map_or(1.0, |scale| scale.min(HEALTH_BOX_SCALE_MAX));
        Self {
            lift: height + offset,
            scale,
        }
    }
}

const HEALTH_BAR_WIDTH_MIN: f32 = 20.0;
const HEALTH_BAR_WIDTH_MAX: f32 = 150.0;

const FRAME_OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 127.0 / 255.0];
const FRAME_TAN: [f32; 4] = [186.0 / 255.0, 146.0 / 255.0, 82.0 / 255.0, 1.0];
const FRAME_FILL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

const UNIT_FRAME_HEIGHTS: [f32; 3] = [9.0, 7.0, 5.0];
const STRUCTURE_FRAME_HEIGHTS: [f32; 3] = [10.0, 8.0, 6.0];

const HEALTH_HIGH: [[f32; 3]; 3] = [
    [12.0 / 255.0, 156.0 / 255.0, 36.0 / 255.0],
    [221.0 / 255.0, 245.0 / 255.0, 142.0 / 255.0],
    [5.0 / 255.0, 113.0 / 255.0, 22.0 / 255.0],
];
const HEALTH_MID: [[f32; 3]; 3] = [
    [230.0 / 255.0, 162.0 / 255.0, 0.0],
    [1.0, 1.0, 176.0 / 255.0],
    [176.0 / 255.0, 97.0 / 255.0, 0.0],
];
const HEALTH_LOW: [[f32; 3]; 3] = [
    [1.0, 19.0 / 255.0, 78.0 / 255.0],
    [1.0, 131.0 / 255.0, 108.0 / 255.0],
    [204.0 / 255.0, 0.0, 1.0 / 255.0],
];

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_health_bars(
    units: &[UnitView],
    boxes: &dyn Fn(&UnitView) -> HealthBox,
    view_proj: Mat4,
    viewport: (u32, u32),
    hovered: Option<mint_sim::Entity>,
    all_health_bars: bool,
    style: HealthBarStyle,
    player_colour: &dyn Fn(mint_sim::PlayerId) -> [f32; 4],
    out: &mut Vec<UiQuad>,
) {
    for unit in units {
        if unit.visibility == mint_sage_shroud::ObjectVisibility::Fogged
            || !shows_health_bar(
                unit.kind_of,
                unit.selected,
                hovered == Some(unit.entity),
                all_health_bars,
            )
        {
            continue;
        }
        let Some(fraction) = unit.health.filter(|f| f.is_finite() && *f > 0.0) else {
            continue;
        };
        let fraction = fraction.min(1.0);
        let health_box = boxes(unit);
        let point = Vec3::new(
            unit.position.x,
            unit.position.y,
            unit.ground + health_box.lift,
        );
        let Some((cx, cy)) = project_to_pixel(view_proj, point, viewport) else {
            continue;
        };
        let width = health_bar_width(unit.radius, health_box.scale);
        let x0 = (cx - width * 0.45).round();
        let y0 = (cy - 2.0).round();
        let heights = if unit.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE) {
            STRUCTURE_FRAME_HEIGHTS
        } else {
            UNIT_FRAME_HEIGHTS
        };
        let rect = |dx: f32, dy: f32, w: f32, h: f32, colour: [f32; 4]| {
            UiQuad::new(x0 + dx, y0 + dy, w, h).with_color_transform(colour, [0.0; 4])
        };
        out.push(rect(-3.0, -3.0, width + 6.0, heights[0], FRAME_OUTLINE));
        out.push(rect(-2.0, -2.0, width + 4.0, heights[1], FRAME_TAN));
        out.push(rect(-1.0, -1.0, width + 2.0, heights[2], FRAME_FILL));
        let (rows, row_count) = match style {
            HealthBarStyle::Health if is_under_construction(unit.conditions) => {
                (construction_colour_rows(fraction), 4)
            }
            HealthBarStyle::Health => {
                let [a, b, c] = health_colour_rows(fraction);
                ([a, b, c, c], 3)
            }
            HealthBarStyle::Player => {
                let (dark, bright) = bevel(player_colour(unit.owner));
                ([dark, bright, dark, dark], 3)
            }
        };
        let filled = (width * fraction).round().max(1.0);
        for (row, colour) in rows.into_iter().take(row_count).enumerate() {
            #[allow(clippy::cast_precision_loss)]
            out.push(rect(0.0, row as f32, filled, 1.0, colour));
        }
    }
}

fn health_bar_width(radius: f32, health_box_scale: f32) -> f32 {
    let clamped = (radius * health_box_scale).clamp(HEALTH_BAR_WIDTH_MIN, HEALTH_BAR_WIDTH_MAX);
    2.0 * clamped
}

#[must_use]
pub(crate) fn shows_health_bar(
    kind_of: mint_sage_asset::KindOf,
    selected: bool,
    hovered: bool,
    all_health_bars: bool,
) -> bool {
    use mint_sage_asset::KindOf;
    if !(selected || hovered) {
        return false;
    }
    if kind_of.contains(KindOf::UNATTACKABLE)
        || kind_of.contains(KindOf::ROCK_VENDOR)
        || kind_of.contains(KindOf::HORDE)
        || kind_of.contains(KindOf::IGNORED_IN_GUI)
    {
        return false;
    }
    if kind_of.contains(KindOf::DEFENSIVE_WALL) && !kind_of.contains(KindOf::HAS_HEALTH_BAR) {
        return false;
    }
    let always = kind_of.contains(KindOf::HERO)
        || kind_of.contains(KindOf::MONSTER)
        || kind_of.contains(KindOf::MACHINE)
        || kind_of.contains(KindOf::MINE)
        || kind_of.contains(KindOf::STRUCTURE)
        || kind_of.contains(KindOf::HAS_HEALTH_BAR);
    always
        || (all_health_bars
            && (kind_of.contains(KindOf::INFANTRY) || kind_of.contains(KindOf::CAVALRY)))
}

fn health_colour_rows(fraction: f32) -> [[f32; 4]; 3] {
    let f = if !fraction.is_finite() || fraction < 0.0 {
        0.0
    } else if fraction > 1.0 {
        1.0
    } else {
        fraction
    };
    let (lo, hi, t) = if f >= 0.8 {
        (HEALTH_HIGH, HEALTH_HIGH, 0.0)
    } else if f >= 0.6 {
        (HEALTH_MID, HEALTH_HIGH, (f - 0.6) * 5.0)
    } else if f >= 0.4 {
        (HEALTH_MID, HEALTH_MID, 0.0)
    } else if f >= 0.2 {
        (HEALTH_LOW, HEALTH_MID, (f - 0.2) * 5.0)
    } else {
        (HEALTH_LOW, HEALTH_LOW, 0.0)
    };
    let [lo0, lo1, lo2] = lo;
    let [hi0, hi1, hi2] = hi;
    [
        blend_row(lo0, hi0, t),
        blend_row(lo1, hi1, t),
        blend_row(lo2, hi2, t),
    ]
}

const CONSTRUCTION_ROWS_START: [[u8; 3]; 4] =
    [[21, 86, 173], [107, 224, 245], [18, 154, 220], [8, 39, 117]];
const CONSTRUCTION_ROWS_END: [[u8; 3]; 4] = [
    [28, 207, 251],
    [172, 255, 254],
    [16, 223, 229],
    [3, 165, 186],
];

fn construction_colour_rows(fraction: f32) -> [[f32; 4]; 4] {
    let f = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut rows = [[0.0, 0.0, 0.0, 1.0]; 4];
    for ((row, start), end) in rows
        .iter_mut()
        .zip(CONSTRUCTION_ROWS_START)
        .zip(CONSTRUCTION_ROWS_END)
    {
        for ((out, a), b) in row.iter_mut().zip(start).zip(end) {
            *out = (f32::from(a) * (1.0 - f) + f32::from(b) * f).trunc() / 255.0;
        }
    }
    rows
}

fn blend_row(lo: [f32; 3], hi: [f32; 3], t: f32) -> [f32; 4] {
    let [lr, lg, lb] = lo;
    let [hr, hg, hb] = hi;
    [
        lr + (hr - lr) * t,
        lg + (hg - lg) * t,
        lb + (hb - lb) * t,
        1.0,
    ]
}

fn is_under_construction(conditions: mint_sage_scene::Conditions) -> bool {
    conditions.contains(mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED)
        || conditions.contains(mint_sage_scene::Conditions::PARTIALLY_CONSTRUCTED)
        || conditions.contains(mint_sage_scene::Conditions::AWAITING_CONSTRUCTION)
}

fn is_actively_being_constructed(conditions: mint_sage_scene::Conditions) -> bool {
    conditions.contains(mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED)
}

fn bevel(base: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let dark = [base[0] * 0.5, base[1] * 0.5, base[2] * 0.5, 1.0];
    let bright = [
        base[0] + (1.0 - base[0]) * 0.55,
        base[1] + (1.0 - base[1]) * 0.55,
        base[2] + (1.0 - base[2]) * 0.55,
        1.0,
    ];
    (dark, bright)
}

pub(crate) fn project_to_pixel(
    view_proj: Mat4,
    point: Vec3,
    viewport: (u32, u32),
) -> Option<(f32, f32)> {
    let row = |r: usize| {
        view_proj.get(r, 0) * point.x
            + view_proj.get(r, 1) * point.y
            + view_proj.get(r, 2) * point.z
            + view_proj.get(r, 3)
    };
    let w = row(3);
    if !w.is_finite() || w <= 1.0e-4 {
        return None;
    }
    let (ndc_x, ndc_y) = (row(0) / w, row(1) / w);
    const MARGIN: f32 = 1.1;
    if !ndc_x.is_finite() || !ndc_y.is_finite() || ndc_x.abs() > MARGIN || ndc_y.abs() > MARGIN {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (width, height) = (viewport.0 as f32, viewport.1 as f32);
    Some(((ndc_x * 0.5 + 0.5) * width, (0.5 - ndc_y * 0.5) * height))
}

fn tint_for(unit: &UnitView) -> [f32; 4] {
    if unit.pick_type != PickType::Selectable {
        return [0.30, 0.42, 0.24, 1.0];
    }
    if unit.dying {
        return [0.16, 0.14, 0.14, 1.0];
    }
    match unit.owner.get() {
        0 => [0.35, 0.55, 0.95, 1.0],
        1 => [0.90, 0.32, 0.28, 1.0],
        _ => [0.85, 0.80, 0.35, 1.0],
    }
}

fn placement(x: f32, y: f32, z: f32, facing: f32, radius: f32, height: f32) -> Mat4 {
    let direction = mint_movement::Facing::from_radians(facing).to_vector();
    let (c, s) = (direction.x, direction.y);
    Mat4::from_cols([
        [c * radius, s * radius, 0.0, 0.0],
        [-s * radius, c * radius, 0.0, 0.0],
        [0.0, 0.0, height, 0.0],
        [x, y, z, 1.0],
    ])
}

fn bar_placement(x: f32, y: f32, z: f32, width: f32, depth: f32, height: f32) -> Mat4 {
    Mat4::from_cols([
        [width, 0.0, 0.0, 0.0],
        [0.0, depth, 0.0, 0.0],
        [0.0, 0.0, height.max(1.0), 0.0],
        [x, y, z, 1.0],
    ])
}

fn oriented_quad_placement(
    x: f32,
    y: f32,
    z: f32,
    facing: f32,
    width: f32,
    depth: f32,
    height: f32,
) -> Mat4 {
    let direction = mint_movement::Facing::from_radians(facing).to_vector();
    let (c, s) = (direction.x, direction.y);
    Mat4::from_cols([
        [c * width, s * width, 0.0, 0.0],
        [-s * depth, c * depth, 0.0, 0.0],
        [0.0, 0.0, height.max(1.0), 0.0],
        [x, y, z, 1.0],
    ])
}

fn quad_mesh() -> MeshData {
    let normal = [0.0, 0.0, 1.0];
    let corners = [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let vertices: Vec<Vertex> = corners
        .iter()
        .map(|(x, y)| Vertex::new([*x, *y, 0.0], normal, [*x + 0.5, *y + 0.5]))
        .collect();
    MeshData {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn index_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn unit_circle() -> Vec<(f32, f32)> {
    let mut points = Vec::with_capacity(SEGMENTS);
    let (mut c, mut s) = (1.0f32, 0.0f32);
    for _ in 0..SEGMENTS {
        points.push((c, s));
        let next_c = c * COS_STEP - s * SIN_STEP;
        let next_s = s * COS_STEP + c * SIN_STEP;
        c = next_c;
        s = next_s;
    }
    points
}

fn body_mesh() -> MeshData {
    let circle = unit_circle();
    let mut mesh = MeshData::new();
    let radius_at = |c: f32| 0.75 + 0.45 * c;

    for (index, (c, s)) in circle.iter().enumerate() {
        let r = radius_at(*c);
        let u = index as f32 / SEGMENTS as f32;
        mesh.vertices
            .push(Vertex::new([c * r, s * r, 0.0], [*c, *s, 0.0], [u, 1.0]));
        mesh.vertices
            .push(Vertex::new([c * r, s * r, 1.0], [*c, *s, 0.0], [u, 0.0]));
    }
    for index in 0..SEGMENTS {
        let a = index_of(index * 2);
        let b = a.wrapping_add(1);
        let next = index_of(((index + 1) % SEGMENTS) * 2);
        let d = next.wrapping_add(1);
        mesh.indices.extend_from_slice(&[a, next, b, b, next, d]);
    }
    let centre = index_of(mesh.vertices.len());
    mesh.vertices
        .push(Vertex::new([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.5, 0.5]));
    for index in 0..SEGMENTS {
        let a = index_of((index * 2) + 1);
        let b = index_of((((index + 1) % SEGMENTS) * 2) + 1);
        mesh.indices.extend_from_slice(&[centre, a, b]);
    }
    mesh
}

fn ring_mesh() -> MeshData {
    let circle = unit_circle();
    let mut mesh = MeshData::new();
    for (index, (c, s)) in circle.iter().enumerate() {
        let u = index as f32 / SEGMENTS as f32;
        mesh.vertices.push(Vertex::new(
            [c * RING_INNER, s * RING_INNER, 0.0],
            [0.0, 0.0, 1.0],
            [u, 0.0],
        ));
        mesh.vertices
            .push(Vertex::new([*c, *s, 0.0], [0.0, 0.0, 1.0], [u, 1.0]));
    }
    for index in 0..SEGMENTS {
        let a = index_of(index * 2);
        let b = a.wrapping_add(1);
        let next = index_of(((index + 1) % SEGMENTS) * 2);
        let d = next.wrapping_add(1);
        mesh.indices.extend_from_slice(&[a, next, b, b, next, d]);
    }
    mesh
}

#[cfg(test)]
mod tests;
