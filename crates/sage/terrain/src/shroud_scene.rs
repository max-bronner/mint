pub const SHROUD_SCENE_WGSL: &str = include_str!("shroud_scene.wgsl");

pub const SHROUD_SCENE_TEXTURE_SLOTS: u32 = 1;

pub const SHROUD_SCENE_UNIFORM_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShroudSceneBlock {
    uv: [f32; 4],
    terms: [f32; 4],
}

impl Default for ShroudSceneBlock {
    fn default() -> Self {
        Self {
            uv: [1.0, 1.0, 0.0, 0.0],
            terms: [0.0; 4],
        }
    }
}

impl ShroudSceneBlock {
    #[must_use]
    pub fn new(extent: [f32; 4], strength: f32) -> Self {
        Self {
            uv: shroud_uv(extent),
            terms: [clamped(strength), 0.0, 0.0, 0.0],
        }
    }

    #[must_use]
    pub const fn uv(&self) -> [f32; 4] {
        self.uv
    }

    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SHROUD_SCENE_UNIFORM_SIZE as usize);
        for value in self.uv.iter().chain(self.terms.iter()) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }
}

fn shroud_uv(extent: [f32; 4]) -> [f32; 4] {
    let [min_x, min_y, size_x, size_y] = extent;
    let size_x = finite(size_x, 1.0);
    let size_y = finite(size_y, 1.0);
    if size_x <= 0.0 || size_y <= 0.0 {
        return [1.0, 1.0, 0.0, 0.0];
    }
    let min_x = finite(min_x, 0.0);
    let min_y = finite(min_y, 0.0);
    [1.0 / size_x, 1.0 / size_y, -min_x / size_x, -min_y / size_y]
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn clamped(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_extents_corners_map_to_the_textures_corners() {
        let extent = [-500.0, 250.0, 1_000.0, 2_000.0];
        let block = ShroudSceneBlock::new(extent, 1.0);
        let at = |x: f32, y: f32| (x * block.uv[0] + block.uv[2], y * block.uv[1] + block.uv[3]);
        let (u, v) = at(-500.0, 250.0);
        assert!(u.abs() < 1e-5 && v.abs() < 1e-5, "{u} {v}");
        let (u, v) = at(500.0, 2_250.0);
        assert!((u - 1.0).abs() < 1e-5 && (v - 1.0).abs() < 1e-5, "{u} {v}");
    }

    #[test]
    fn a_degenerate_extent_gives_the_identity_rather_than_a_division_by_zero() {
        for extent in [
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 10.0],
            [f32::NAN, 0.0, 10.0, 10.0],
            [0.0, 0.0, f32::INFINITY, 10.0],
        ] {
            let block = ShroudSceneBlock::new(extent, 1.0);
            for value in block.uv.iter().chain(block.terms.iter()) {
                assert!(value.is_finite(), "{extent:?} gave {value}");
            }
        }
    }

    #[test]
    fn a_hostile_strength_reads_as_off_and_never_past_full() {
        assert_eq!(
            ShroudSceneBlock::new([0.0, 0.0, 1.0, 1.0], f32::NAN).terms[0],
            0.0
        );
        assert_eq!(
            ShroudSceneBlock::new([0.0, 0.0, 1.0, 1.0], -3.0).terms[0],
            0.0
        );
        assert_eq!(
            ShroudSceneBlock::new([0.0, 0.0, 1.0, 1.0], 9.0).terms[0],
            1.0
        );
    }

    #[test]
    fn the_block_is_the_size_the_shader_declares() {
        let bytes = ShroudSceneBlock::default().to_bytes();
        assert_eq!(bytes.len(), SHROUD_SCENE_UNIFORM_SIZE as usize);
    }

    #[test]
    fn the_default_block_draws_no_shroud_at_all() {
        assert_eq!(ShroudSceneBlock::default().terms[0], 0.0);
    }

    #[test]
    fn the_wgsl_declares_the_bindings_the_registration_promises() {
        assert!(SHROUD_SCENE_WGSL.contains("@group(3) @binding(0) var<uniform> mint_shroud:"));
        assert!(SHROUD_SCENE_WGSL.contains("@group(3) @binding(1) var mint_shroud_texture:"));
        assert!(SHROUD_SCENE_WGSL.contains("@group(3) @binding(2) var mint_shroud_sampler:"));
        assert!(SHROUD_SCENE_WGSL.contains("fn mint_shroud_at(world_xy: vec2<f32>) -> f32"));
    }
}
