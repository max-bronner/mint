@group(1) @binding(0) var particle_texture: texture_2d<f32>;
@group(1) @binding(1) var particle_sampler: sampler;

const SMUDGE_TINT: vec3<f32> = vec3<f32>(1.0, 0.93333334, 0.8666667);

struct SmudgeOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) clip_distance: f32,
    @location(3) @interpolate(flat) instance: u32,
    @location(4) shift: f32,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> SmudgeOut {
    slot = index;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    var out: SmudgeOut;
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    out.color = v.color;
    out.clip_distance = mint_clip_distance(world.xyz);
    out.instance = index;
    out.shift = v.tangent.y;
    return out;
}

@fragment
fn fs_main(in: SmudgeOut) -> @location(0) vec4<f32> {
    slot = in.instance;
    let size = vec2<f32>(textureDimensions(mint_scene_color));
    let du = length(vec2<f32>(dpdx(in.uv.x), dpdy(in.uv.x)));
    let dv = length(vec2<f32>(dpdx(in.uv.y), dpdy(in.uv.y)));
    let extent = vec2<f32>(1.0 / max(du * size.x, 1e-6), 1.0 / max(dv * size.y, 1e-6));
    if (in.clip_distance < 0.0) {
        discard;
    }
    let centred = abs(in.uv - vec2<f32>(0.5, 0.5));
    let weight = clamp(1.0 - 2.0 * max(centred.x, centred.y), 0.0, 1.0);
    let screen = in.clip_position.xy / size;
    let sampled = textureSample(
        mint_scene_color,
        mint_scene_color_sampler,
        clamp(screen + extent * (in.shift * weight), vec2<f32>(0.0), vec2<f32>(1.0)),
    );
    return vec4<f32>(sampled.rgb * SMUDGE_TINT, clamp(in.color.a * weight, 0.0, 1.0));
}
