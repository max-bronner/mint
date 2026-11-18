pub const ROAD_SHADER_WGSL: &str = concat!(
    include_str!("shroud_scene.wgsl"),
    include_str!("road_shader.wgsl")
);

pub const ROAD_TEXTURE_SLOTS: u32 = 1;

pub const ROAD_SCENE_TEXTURE_SLOTS: u32 = crate::shroud_scene::SHROUD_SCENE_TEXTURE_SLOTS;

pub const ROAD_SCENE_UNIFORM_SIZE: u32 = crate::shroud_scene::SHROUD_SCENE_UNIFORM_SIZE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shroud_scene_binding_is_prepended() {
        assert!(ROAD_SHADER_WGSL.contains("fn mint_shroud_at"));
        assert!(ROAD_SHADER_WGSL.contains("@group(3) @binding(1) var mint_shroud_texture"));
    }

    #[test]
    fn the_shroud_multiplies_both_lit_paths() {
        assert!(
            ROAD_SHADER_WGSL.contains("mint_fog(sampled.rgb * lit, in.world_position) * shroud")
        );
        assert!(ROAD_SHADER_WGSL
            .contains("mint_fog(sampled.rgb * flat_lit, in.world_position) * shroud"));
    }

    #[test]
    fn a_road_is_shadowed_the_way_the_built_in_shader_shadows_it() {
        assert!(ROAD_SHADER_WGSL
            .contains("lightsum = lightsum * mint_shadow_factor(in.world_position);"));
        assert!(!ROAD_SHADER_WGSL.contains("sun_shade"));
    }

    #[test]
    fn the_declared_slot_counts_match_what_the_wgsl_uses() {
        assert_eq!(ROAD_TEXTURE_SLOTS, 1);
        assert_eq!(ROAD_SCENE_TEXTURE_SLOTS, 1);
        assert_eq!(ROAD_SCENE_UNIFORM_SIZE, 32);
        assert!(ROAD_SHADER_WGSL.contains("@group(1) @binding(1) var base_texture"));
        assert!(!ROAD_SHADER_WGSL.contains("@group(1) @binding(3)"));
    }
}
