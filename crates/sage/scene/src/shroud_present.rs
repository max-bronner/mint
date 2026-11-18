use mint_render::{
    AddressMode, CustomShaderHandle, PixelFormat, Renderer, TextureData, TextureHandle,
};
use mint_sage_shroud::{CellVisibility, VisibilityGrid};
use mint_sage_terrain::ShroudSceneBlock;
use mint_sim::PlayerId;

pub const SHROUD_ALPHA: u8 = 0;

pub const FOG_ALPHA: u8 = 127;

pub const CLEAR_ALPHA: u8 = 255;

pub const FOG_LEVELS_PER_SECOND: f32 = 255.0;

const MAX_FADE_STEP: f32 = 255.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShroudFade {
    Snap,
    #[default]
    Ramp,
}

impl ShroudFade {
    pub const ALL: [Self; 2] = [Self::Snap, Self::Ramp];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Snap => Self::Ramp,
            Self::Ramp => Self::Snap,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Snap => "snap",
            Self::Ramp => "ramp",
        }
    }
}

pub const MAX_SHROUD_EDGE: u32 = 4_096;

#[derive(Debug)]
pub struct ShroudView {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    target: Vec<u8>,
    texture: TextureHandle,
    settled: bool,
    dirty: bool,
    uploaded: Option<(u8, bool)>,
}

impl ShroudView {
    pub fn new(renderer: &mut Renderer, cells_x: u32, cells_y: u32) -> mint_render::Result<Self> {
        let width = cells_x.clamp(1, MAX_SHROUD_EDGE);
        let height = cells_y.clamp(1, MAX_SHROUD_EDGE);
        let count = usize::try_from(width)
            .unwrap_or(1)
            .saturating_mul(usize::try_from(height).unwrap_or(1))
            .saturating_mul(4);
        let pixels = vec![CLEAR_ALPHA; count];
        let target = vec![CLEAR_ALPHA; count.checked_div(4).unwrap_or(0)];
        let data = TextureData::single(
            PixelFormat::Rgba8Unorm,
            width.saturating_add(2),
            height.saturating_add(2),
            padded_rgba(width, height, &pixels, SHROUD_ALPHA),
        )
        .with_addressing(AddressMode::Clamp, AddressMode::Clamp);
        let texture = renderer.upload_texture(&data)?;
        Ok(Self {
            width,
            height,
            pixels,
            target,
            texture,
            settled: true,
            dirty: false,
            uploaded: Some((SHROUD_ALPHA, false)),
        })
    }

    #[must_use]
    pub const fn texture(&self) -> TextureHandle {
        self.texture
    }

    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[must_use]
    pub fn level_at(&self, x: u32, y: u32) -> u8 {
        let Some(index) = self.index_of(x, y) else {
            return CLEAR_ALPHA;
        };
        self.pixels.get(index).copied().unwrap_or(CLEAR_ALPHA)
    }

    #[must_use]
    pub fn target_at(&self, x: u32, y: u32) -> u8 {
        let Some(index) = self.cell_index_of(x, y) else {
            return CLEAR_ALPHA;
        };
        self.target.get(index).copied().unwrap_or(CLEAR_ALPHA)
    }

    pub fn rebuild_from(&mut self, grid: &VisibilityGrid, viewer: PlayerId) -> (u32, u32, u32) {
        let (mut unexplored, mut explored, mut visible) = (0u32, 0u32, 0u32);
        let mut changed = false;
        for y in 0..self.height {
            for x in 0..self.width {
                let (Ok(cell_x), Ok(cell_y)) = (i32::try_from(x), i32::try_from(y)) else {
                    continue;
                };
                let level = match grid.cell_state(viewer, cell_x, cell_y) {
                    CellVisibility::Unexplored => {
                        unexplored = unexplored.saturating_add(1);
                        SHROUD_ALPHA
                    }
                    CellVisibility::Explored => {
                        explored = explored.saturating_add(1);
                        FOG_ALPHA
                    }
                    CellVisibility::Visible => {
                        visible = visible.saturating_add(1);
                        CLEAR_ALPHA
                    }
                };
                if let Some(slot) = self
                    .cell_index_of(x, y)
                    .and_then(|index| self.target.get_mut(index))
                {
                    changed |= *slot != level;
                    *slot = level;
                }
            }
        }
        if changed {
            self.settled = false;
        }
        (unexplored, explored, visible)
    }

    pub fn fade(&mut self, seconds: f32, mode: ShroudFade) -> u32 {
        if self.settled {
            return 0;
        }
        let step = match mode {
            ShroudFade::Snap => MAX_FADE_STEP,
            ShroudFade::Ramp => {
                if !seconds.is_finite() || seconds <= 0.0 {
                    return self.moving();
                }
                (seconds * FOG_LEVELS_PER_SECOND).clamp(0.0, MAX_FADE_STEP)
            }
        };
        let (moving, changed) = step_towards(&mut self.pixels, &self.target, fade_step(step));
        self.dirty |= changed;
        self.settled = moving == 0;
        moving
    }

    pub fn snap(&mut self) {
        let _ = self.fade(0.0, ShroudFade::Snap);
    }

    #[must_use]
    pub fn moving(&self) -> u32 {
        let mut moving = 0u32;
        for (cell, &want) in self.target.iter().enumerate() {
            let index = cell.saturating_mul(4);
            if self.pixels.get(index).copied() != Some(want) {
                moving = moving.saturating_add(1);
            }
        }
        moving
    }

    pub fn upload(&mut self, renderer: &mut Renderer, ring: u8) -> mint_render::Result<bool> {
        if !self.dirty && self.uploaded == Some((ring, false)) {
            return Ok(false);
        }
        self.upload_padded(
            renderer,
            padded_rgba(self.width, self.height, &self.pixels, ring),
        )?;
        self.dirty = false;
        self.uploaded = Some((ring, false));
        Ok(true)
    }

    pub fn upload_revealed(
        &mut self,
        renderer: &mut Renderer,
        ring: u8,
    ) -> mint_render::Result<bool> {
        if self.uploaded == Some((ring, true)) {
            return Ok(false);
        }
        let clear = vec![CLEAR_ALPHA; self.pixels.len()];
        self.upload_padded(renderer, padded_rgba(self.width, self.height, &clear, ring))?;
        self.uploaded = Some((ring, true));
        Ok(true)
    }

    fn upload_padded(&self, renderer: &mut Renderer, padded: Vec<u8>) -> mint_render::Result<()> {
        let data = TextureData::single(
            PixelFormat::Rgba8Unorm,
            self.width.saturating_add(2),
            self.height.saturating_add(2),
            padded,
        )
        .with_addressing(AddressMode::Clamp, AddressMode::Clamp);
        renderer.update_texture(self.texture, &data)
    }

    fn index_of(&self, x: u32, y: u32) -> Option<usize> {
        self.cell_index_of(x, y)?.checked_mul(4)
    }

    fn cell_index_of(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let row = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.width).ok()?)?;
        row.checked_add(usize::try_from(x).ok()?)
    }
}

fn padded_rgba(width: u32, height: u32, drawn: &[u8], ring: u8) -> Vec<u8> {
    let padded_w = width.saturating_add(2);
    let padded_h = height.saturating_add(2);
    let texels = usize::try_from(padded_w)
        .unwrap_or(0)
        .saturating_mul(usize::try_from(padded_h).unwrap_or(0));
    let ring_texel = [ring, ring, ring, shroud_coverage(ring)];
    let mut out: Vec<u8> = ring_texel
        .iter()
        .copied()
        .cycle()
        .take(texels.saturating_mul(4))
        .collect();
    let padded_w = usize::try_from(padded_w).unwrap_or(0);
    let row_width = usize::try_from(width).unwrap_or(0);
    for y in 0..height {
        for x in 0..width {
            let (Ok(row), Ok(col)) = (usize::try_from(y), usize::try_from(x)) else {
                continue;
            };
            let Some(src) = row
                .checked_mul(row_width)
                .and_then(|r| r.checked_add(col))
                .and_then(|c| c.checked_mul(4))
            else {
                continue;
            };
            let Some(texel) = drawn.get(src..src.saturating_add(4)) else {
                continue;
            };
            let Some(dst) = row
                .saturating_add(1)
                .checked_mul(padded_w)
                .and_then(|r| r.checked_add(col.saturating_add(1)))
                .and_then(|c| c.checked_mul(4))
            else {
                continue;
            };
            if let Some(slot) = out.get_mut(dst..dst.saturating_add(4)) {
                slot.copy_from_slice(texel);
                if let (Some(alpha), Some(&level)) = (slot.get_mut(3), texel.first()) {
                    *alpha = shroud_coverage(level);
                }
            }
        }
    }
    out
}

#[must_use]
pub fn shroud_coverage(level: u8) -> u8 {
    let scaled = u32::from(level.saturating_sub(4)).saturating_mul(255) / 46;
    u8::try_from(scaled.min(255)).unwrap_or(u8::MAX)
}

#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn fade_step(levels: f32) -> u8 {
    if levels.is_nan() || levels <= 0.0 {
        return 1;
    }
    (levels.ceil() as u32).clamp(1, 255) as u8
}

#[must_use]
pub fn ramp(drawn: &mut [u8], target: &[u8], step: u8) -> u32 {
    step_towards(drawn, target, step).0
}

fn step_towards(drawn: &mut [u8], target: &[u8], step: u8) -> (u32, bool) {
    let mut moving = 0u32;
    let mut changed = false;
    for (cell, &want) in target.iter().enumerate() {
        let Some(index) = cell.checked_mul(4) else {
            continue;
        };
        let Some(texel) = drawn.get_mut(index..index.saturating_add(4)) else {
            continue;
        };
        let Some(&have) = texel.first() else { continue };
        if have == want {
            continue;
        }
        let next = if want > have {
            have.saturating_add(step).min(want)
        } else {
            have.saturating_sub(step).max(want)
        };
        texel.fill(next);
        changed = true;
        if next != want {
            moving = moving.saturating_add(1);
        }
    }
    (moving, changed)
}

pub fn bind_shroud_scene(
    renderer: &mut Renderer,
    shader: Option<CustomShaderHandle>,
    texture: Option<TextureHandle>,
    extent: [f32; 4],
    strength: f32,
) -> mint_render::Result<()> {
    let Some(shader) = shader else {
        return Ok(());
    };
    let block = ShroudSceneBlock::new(extent, strength).to_bytes();
    renderer.set_custom_scene(shader, &[texture], &block)
}

pub fn push_shroud_scene(
    renderer: &mut Renderer,
    shader: Option<CustomShaderHandle>,
    extent: [f32; 4],
    strength: f32,
) {
    let Some(shader) = shader else {
        return;
    };
    let block = ShroudSceneBlock::new(extent, strength).to_bytes();
    if let Err(error) = renderer.update_custom_scene(shader, &block) {
        eprintln!("  could not update a shroud scene block: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shroud_texture_is_written_only_when_what_it_holds_would_change() {
        let mut renderer = match Renderer::offscreen(4, 4) {
            Ok(renderer) => renderer,
            Err(mint_render::Error::NoAdapter) => {
                eprintln!("SKIPPED: no graphics adapter");
                return;
            }
            Err(error) => panic!("creating an offscreen renderer failed: {error}"),
        };
        let extent = mint_sage_shroud::GridExtent::covering((0.0, 0.0), (40.0, 40.0), 10.0);
        let mut grid = VisibilityGrid::new(extent, 1).expect("grid");
        let viewer = PlayerId::new(0);
        let (width, height) = (extent.cells_x, extent.cells_y);
        let mut view = ShroudView::new(&mut renderer, width, height).expect("view");
        let mut upload =
            |view: &mut ShroudView, ring| view.upload(&mut renderer, ring).expect("up");

        assert!(
            !upload(&mut view, SHROUD_ALPHA),
            "`new` uploaded these bytes"
        );
        let _ = view.rebuild_from(&grid, viewer);
        view.snap();
        assert!(upload(&mut view, SHROUD_ALPHA), "the grid changed");
        assert!(!upload(&mut view, SHROUD_ALPHA), "nothing changed");
        let _ = view.rebuild_from(&grid, viewer);
        assert_eq!(view.fade(0.1, ShroudFade::Ramp), 0);
        assert!(
            !upload(&mut view, SHROUD_ALPHA),
            "a rebuild to the same grid"
        );
        assert!(upload(&mut view, CLEAR_ALPHA), "a new ring");

        grid.add_looker(viewer, 1, 1);
        let _ = view.rebuild_from(&grid, viewer);
        let mut writes = 0;
        loop {
            let moving = view.fade(0.1, ShroudFade::Ramp);
            assert!(
                upload(&mut view, CLEAR_ALPHA),
                "every step of the ramp is drawn"
            );
            writes += 1;
            if moving == 0 {
                break;
            }
        }
        assert!(writes > 1, "the ramp took more than one step");
        assert_eq!(view.level_at(1, 1), CLEAR_ALPHA);
        assert_eq!(view.fade(0.1, ShroudFade::Ramp), 0);
        assert!(!upload(&mut view, CLEAR_ALPHA), "settled");
    }

    #[test]
    fn the_texel_alpha_follows_retails_shroud_packer() {
        assert_eq!(shroud_coverage(SHROUD_ALPHA), 0);
        assert_eq!(shroud_coverage(4), 0);
        assert_eq!(shroud_coverage(27), 127);
        assert_eq!(shroud_coverage(50), 255);
        assert_eq!(shroud_coverage(FOG_ALPHA), 255);
        assert_eq!(shroud_coverage(CLEAR_ALPHA), 255);
    }

    fn drawn(levels: &[u8]) -> Vec<u8> {
        levels.iter().flat_map(|&l| [l, l, l, l]).collect()
    }

    #[test]
    fn a_cell_takes_about_a_second_to_go_from_shrouded_to_clear() {
        let target = [CLEAR_ALPHA];
        let mut pixels = drawn(&[SHROUD_ALPHA]);
        let step = fade_step(FOG_LEVELS_PER_SECOND / 144.0);
        assert_eq!(step, 2, "1.77 levels a frame rounds up to 2");
        let mut frames = 0u32;
        while ramp(&mut pixels, &target, step) > 0 {
            frames = frames.saturating_add(1);
            assert!(frames < 1_000, "the ramp never arrived");
        }
        assert_eq!(frames, 127);
        assert_eq!(pixels[0], CLEAR_ALPHA);
    }

    #[test]
    fn the_largest_step_arrives_in_one_frame() {
        let target = [CLEAR_ALPHA, SHROUD_ALPHA];
        let mut pixels = drawn(&[SHROUD_ALPHA, CLEAR_ALPHA]);
        assert_eq!(ramp(&mut pixels, &target, 255), 0);
        assert_eq!(pixels[0], CLEAR_ALPHA);
        assert_eq!(pixels[4], SHROUD_ALPHA);
    }

    #[test]
    fn a_ramp_stops_exactly_on_its_target_in_both_directions() {
        let target = [FOG_ALPHA, FOG_ALPHA];
        let mut pixels = drawn(&[CLEAR_ALPHA, SHROUD_ALPHA]);
        for _ in 0..100 {
            if ramp(&mut pixels, &target, 100) == 0 {
                break;
            }
        }
        assert_eq!(pixels[0], FOG_ALPHA);
        assert_eq!(pixels[4], FOG_ALPHA);
    }

    #[test]
    fn the_ramp_writes_all_four_channels() {
        let target = [CLEAR_ALPHA];
        let mut pixels = drawn(&[SHROUD_ALPHA]);
        let _ = ramp(&mut pixels, &target, 10);
        assert_eq!(pixels, vec![10, 10, 10, 10]);
    }

    #[test]
    fn a_degenerate_frame_time_still_advances_by_one_level() {
        for levels in [0.0, -1.0, f32::NAN, f32::NEG_INFINITY, 1.0e-9] {
            assert!(fade_step(levels) >= 1, "step for {levels} was zero");
        }
        assert_eq!(fade_step(f32::INFINITY), 255);
        assert_eq!(fade_step(1.0e-9), 1);
    }

    #[test]
    fn mismatched_lengths_are_ignored_rather_than_indexed() {
        let mut short = drawn(&[SHROUD_ALPHA]);
        assert_eq!(ramp(&mut short, &[CLEAR_ALPHA; 8], 255), 0);
        let mut long = drawn(&[SHROUD_ALPHA; 8]);
        assert_eq!(ramp(&mut long, &[CLEAR_ALPHA], 255), 0);
        assert_eq!(long[0], CLEAR_ALPHA);
        assert_eq!(long[4], SHROUD_ALPHA);
        let mut empty: Vec<u8> = Vec::new();
        assert_eq!(ramp(&mut empty, &[CLEAR_ALPHA], 255), 0);
    }

    #[test]
    fn the_padded_texture_rings_the_cells_in_black() {
        let drawn_cells = drawn(&[CLEAR_ALPHA, CLEAR_ALPHA]);
        let padded = padded_rgba(2, 1, &drawn_cells, SHROUD_ALPHA);
        assert_eq!(padded.len(), 4 * 3 * 4);
        let texel = |x: usize, y: usize| &padded[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        for y in [0, 2] {
            for x in 0..4 {
                assert_eq!(texel(x, y), [SHROUD_ALPHA; 4], "ring at ({x},{y})");
            }
        }
        assert_eq!(texel(0, 1), [SHROUD_ALPHA; 4], "left ring");
        assert_eq!(texel(3, 1), [SHROUD_ALPHA; 4], "right ring");
        assert_eq!(texel(1, 1), [CLEAR_ALPHA; 4], "cell (0,0) at offset (1,1)");
        assert_eq!(texel(2, 1), [CLEAR_ALPHA; 4], "cell (1,0) at offset (2,1)");
    }

    #[test]
    fn a_ring_lifted_to_clear_alpha_is_fully_lit() {
        let padded = padded_rgba(1, 1, &drawn(&[SHROUD_ALPHA]), CLEAR_ALPHA);
        assert_eq!(padded.get(..4), Some(&[CLEAR_ALPHA; 4][..]));
        let cell = (3 + 1) * 4;
        assert_eq!(padded.get(cell..cell + 4), Some(&[SHROUD_ALPHA; 4][..]));
    }

    #[test]
    fn the_fade_rungs_cycle() {
        assert_eq!(ShroudFade::default(), ShroudFade::Ramp);
        assert_eq!(ShroudFade::Ramp.next(), ShroudFade::Snap);
        assert_eq!(ShroudFade::Snap.next(), ShroudFade::Ramp);
        assert_ne!(ShroudFade::Snap.label(), ShroudFade::Ramp.label());
        assert_eq!(ShroudFade::ALL.len(), 2);
    }
}
