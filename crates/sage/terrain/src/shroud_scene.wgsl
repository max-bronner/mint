struct MintShroudBlock {
    shroud_uv: vec4<f32>,
    shroud_terms: vec4<f32>,
};

@group(3) @binding(0) var<uniform> mint_shroud: MintShroudBlock;
@group(3) @binding(1) var mint_shroud_texture: texture_2d<f32>;
@group(3) @binding(2) var mint_shroud_sampler: sampler;

fn mint_shroud_at(world_xy: vec2<f32>) -> f32 {
    let uv = world_xy * mint_shroud.shroud_uv.xy + mint_shroud.shroud_uv.zw;
    let sampled = textureSample(mint_shroud_texture, mint_shroud_sampler, uv).r;
    return mix(1.0, sampled, mint_shroud.shroud_terms.x);
}
