@group(1) @binding(0) var particle_texture: texture_2d<f32>;
@group(1) @binding(1) var particle_sampler: sampler;

struct FlipbookOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) next_uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) weight: f32,
    @location(4) clip_distance: f32,
    @location(5) @interpolate(flat) instance: u32,
    @location(6) world: vec3<f32>,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> FlipbookOut {
    slot = index;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    var out: FlipbookOut;
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    out.next_uv = v.uv2;
    out.color = v.color;
    out.weight = clamp(v.tangent.x, 0.0, 1.0);
    out.clip_distance = mint_clip_distance(world.xyz);
    out.instance = index;
    out.world = world.xyz;
    return out;
}

@fragment
fn fs_main(in: FlipbookOut) -> @location(0) vec4<f32> {
    slot = in.instance;
    if (in.clip_distance < 0.0) {
        discard;
    }
    let here = textureSample(particle_texture, particle_sampler, in.uv);
    let next = textureSample(particle_texture, particle_sampler, in.next_uv);
    let cell = mix(here, next, in.weight);
    let tint = instances[in.instance].tint;
    let rgb = mint_fog_mode(cell.rgb * in.color.rgb * tint.rgb, in.world);
    return vec4<f32>(rgb, cell.a * in.color.a * tint.a);
}
