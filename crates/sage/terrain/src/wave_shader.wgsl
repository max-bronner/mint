struct Wave {
    size: vec4<f32>,
    life: vec4<f32>,
    timing: vec4<f32>,
    mode: vec4<f32>,
    beach: vec4<f32>,
};

@group(1) @binding(0) var<uniform> wave: Wave;
@group(1) @binding(1) var wave_texture: texture_2d<f32>;
@group(1) @binding(2) var wave_sampler: sampler;

struct WaveOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) coverage: f32,
    @location(2) @interpolate(flat) instance: u32,
    @location(3) world_pos: vec3<f32>,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> WaveOut {
    slot = index;

    let along = v.tangent.xy;
    let shore = v.normal.xy;

    let cycle = max(wave.timing.x, 1e-3);
    let age = fract(wave.timing.y / cycle + v.color.x) * cycle;
    let life = max(wave.life.w, 1e-3);
    let alive = step(age, life);
    let t = clamp(age / life, 0.0, 1.0);

    let grow = clamp(age / max(wave.life.z, 1e-3), 0.0, 1.0);
    let width = wave.size.x * mix(wave.life.x, 1.0, grow) * alive;
    let squash = clamp((age - wave.life.z) / max(life - wave.life.z, 1e-3), 0.0, 1.0);
    let depth = wave.size.y * mix(wave.life.y, 1.0, grow)
        * mix(1.0, clamp(wave.timing.z, 0.0, 1.0), squash) * alive;

    let offset = wave.size.z - wave.size.w * age;

    var local = vec3<f32>(
        v.position.xy - shore * offset + along * (v.uv2.x * width) + shore * (v.uv2.y * depth),
        v.position.z,
    );

    if (wave.mode.x >= 1.5 && wave.mode.x < 2.5 && wave.mode.y > 0.5) {
        let across = clamp(v.uv2.y + 0.5, 0.0, 1.0);
        local.z = local.z + across * wave.timing.w * alive * mix(0.3, 1.0, grow);
    }

    local.z = local.z + wave.beach.z;
    let world = instances[index].model * vec4<f32>(local, 1.0);

    var out: WaveOut;
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    out.instance = index;
    out.world_pos = world.xyz;

    let rise = smoothstep(0.0, max(wave.mode.z, 1e-3), t);
    let fall = 1.0 - smoothstep(clamp(wave.mode.w, 0.0, 1.0), 1.0, t);
    out.coverage = rise * fall * alive;
    return out;
}

@fragment
fn fs_main(in: WaveOut) -> @location(0) vec4<f32> {
    slot = in.instance;

    let texel = textureSample(wave_texture, wave_sampler, in.uv);
    let tint = instances[in.instance].tint;
    var alpha = texel.a * in.coverage * tint.a * wave.beach.y;
    let rgb = texel.rgb * tint.rgb * mint_shadow_factor(in.world_pos);

    let terms = frame.depth_params;
    let stored = textureLoad(mint_scene_depth, vec2<i32>(in.clip_position.xy), 0);
    let behind = terms.x / (terms.y * stored + terms.z);
    let here = 1.0 / max(in.clip_position.w, 1e-9);
    let column = max(behind - here, 0.0);
    alpha = alpha * clamp(column / max(wave.beach.x, 1e-3), 0.0, 1.0);

    if (alpha <= 0.0) {
        discard;
    }
    let shrouded = mint_fog(rgb, in.world_pos) * mint_shroud_at(in.world_pos.xy);
    return vec4<f32>(shrouded, clamp(alpha, 0.0, 1.0));
}
