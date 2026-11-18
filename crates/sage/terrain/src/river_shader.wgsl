struct River {
    stage: vec4<f32>,
    noise: vec4<f32>,
    phases: vec4<f32>,
    flow: vec4<f32>,
    glint: vec4<f32>,
    edge: vec4<f32>,
    refraction: vec4<f32>,
    reflection: vec4<f32>,
};

@group(1) @binding(0) var<uniform> river: River;
@group(1) @binding(11) var reflection_texture: texture_2d<f32>;
@group(1) @binding(12) var reflection_sampler: sampler;
@group(1) @binding(1) var river_texture: texture_2d<f32>;
@group(1) @binding(2) var river_sampler: sampler;
@group(1) @binding(3) var opacity_texture: texture_2d<f32>;
@group(1) @binding(4) var opacity_sampler: sampler;
@group(1) @binding(5) var noise_texture: texture_2d<f32>;
@group(1) @binding(6) var noise_sampler: sampler;
@group(1) @binding(7) var sparkle_texture: texture_2d<f32>;
@group(1) @binding(8) var sparkle_sampler: sampler;
@group(1) @binding(9) var ripple_texture: texture_2d<f32>;
@group(1) @binding(10) var ripple_sampler: sampler;

struct RiverOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(6) river_uv: vec2<f32>,
    @location(1) flow: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) noise_uv: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> RiverOut {
    slot = index;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);

    var out: RiverOut;
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    out.river_uv = v.uv + vec2<f32>(0.0, river.phases.w);
    out.flow = v.uv2;
    out.world_pos = world.xyz;
    out.color = v.color;
    out.instance = index;

    if (river.flow.x > 0.5) {
        out.noise_uv = world.xy * river.noise.z + river.noise.xy;
    } else {
        let span = river.flow.w * river.noise.z;
        out.noise_uv = v.uv * span - vec2<f32>(0.0, river.noise.x);
    }
    return out;
}

fn ripple_at(uv: vec2<f32>) -> vec2<f32> {
    let t = textureSample(ripple_texture, ripple_sampler, uv).xyz * 2.0 - vec3<f32>(1.0);
    return t.xy / max(t.z, 0.25);
}

fn flow_slope(ribbon_uv: vec2<f32>, phases: vec3<f32>) -> vec2<f32> {
    let p = ribbon_uv * river.flow.w * river.flow.y;

    var sum = vec2<f32>(0.0);
    let layers = array<vec3<f32>, 3>(
        vec3<f32>(1.0, 1.0, 0.0),
        vec3<f32>(2.7, 0.55, 0.0),
        vec3<f32>(1.6, 0.35, 1.0)
    );
    let rates = array<f32, 3>(phases.x, phases.y, phases.z);
    for (var i = 0; i < 3; i = i + 1) {
        let layer = layers[i];
        var uv = p * layer.x;
        var travel = vec2<f32>(0.0, rates[i]);
        if (layer.z > 0.5) {
            uv = vec2<f32>(uv.y, uv.x);
            travel = vec2<f32>(rates[i], 0.0);
        }
        var slope = ripple_at(uv - travel) * layer.y;
        if (layer.z > 0.5) {
            slope = vec2<f32>(slope.y, slope.x);
        }
        sum = sum + slope;
    }
    return sum;
}

fn column_at(position: vec4<f32>, world_pos: vec3<f32>) -> f32 {
    let terms = frame.depth_params;
    let stored = textureLoad(mint_scene_depth, vec2<i32>(position.xy), 0);
    let behind = terms.x / (terms.y * stored + terms.z);
    let here = 1.0 / max(position.w, 1e-9);
    let to_pixel = world_pos - frame.camera_eye.xyz;
    let span = length(to_pixel);
    let path = max(behind - here, 0.0) * (span / max(here, 1e-9));
    let down = max(-to_pixel.z / max(span, 1e-9), 0.0);
    return path * down;
}

fn refract_uv(position: vec4<f32>, normal: vec3<f32>, here: f32, dims: vec2<f32>) -> vec2<f32> {
    let screen_uv = position.xy / dims;
    if (river.refraction.x < 0.5) {
        return screen_uv;
    }
    let falloff = clamp(river.refraction.z / max(here, 1e-3), 0.0, 1.0);
    let uv = clamp(
        screen_uv + normal.xy * river.refraction.y * falloff,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    if (river.refraction.x > 1.5) {
        return uv;
    }
    let terms = frame.depth_params;
    let stored = textureLoad(mint_scene_depth, vec2<i32>(uv * dims), 0);
    let behind = terms.x / (terms.y * stored + terms.z);
    if (behind < here) {
        return screen_uv;
    }
    return uv;
}

@fragment
fn fs_main(in: RiverOut) -> @location(0) vec4<f32> {
    slot = in.instance;

    let stage = river.stage.x;
    let diffuse = in.color * instances[slot].tint;

    var flow = in.flow;
    let flow_len = length(flow);
    if (flow_len > 1e-4) {
        flow = flow / flow_len;
    } else {
        flow = vec2<f32>(1.0, 0.0);
    }
    let from_centre = clamp(abs(in.uv.x - 0.5) * 2.0, 0.0, 1.0);
    var gain = 1.0 + river.glint.w * smoothstep(0.55, 1.0, from_centre);

    let column = column_at(in.clip_position, in.world_pos);
    let band = max(river.edge.x, 1e-4);
    let shallow = 1.0 - clamp(column / band, 0.0, 1.0);
    gain = gain * (1.0 + river.edge.y * shallow);

    let slope = flow_slope(in.uv, river.phases.xyz) * river.flow.z * gain;
    let across = vec2<f32>(-flow.y, flow.x);
    let tilt = across * slope.x + flow * slope.y;
    let normal = normalize(vec3<f32>(tilt, 1.0));

    let surface = textureSample(river_texture, river_sampler, in.river_uv);
    let opacity = textureSample(opacity_texture, opacity_sampler, in.uv);
    let noise = textureSample(noise_texture, noise_sampler, in.noise_uv);
    let sparkle_uv = in.uv + normal.xy * 0.05;
    let sparkle = textureSample(sparkle_texture, sparkle_sampler, sparkle_uv);

    var rgb = diffuse.rgb * surface.rgb;
    var alpha = diffuse.a * surface.a;
    if (stage >= 1.5) {
        rgb = rgb + opacity.rgb;
        alpha = alpha * opacity.a;
    }
    if (stage >= 2.5) {
        rgb = rgb + sparkle.rgb * noise.rgb * river.glint.z;
    }
    alpha = clamp(alpha * river.stage.y, 0.0, 1.0);

    if (river.flow.z > 0.0) {
        rgb = rgb * mint_shadow_factor(in.world_pos);
    }

    if (stage >= 3.5) {
        let view = normalize(frame.camera_eye.xyz - in.world_pos);
        let to_light = -frame.light_directions[0].xyz;
        if (dot(to_light, to_light) > 1e-6) {
            let h = normalize(view + normalize(to_light));
            let spec = pow(max(dot(normal, h), 0.0), max(river.glint.y, 1.0));
            rgb = rgb + frame.light_colors[0].rgb * spec * river.glint.x * gain;
        }
        if (river.flow.z > 0.0) {
            alpha = alpha * clamp(column / band, 0.0, 1.0);
        }
    }

    if (river.reflection.x > 0.5 && stage >= 3.5) {
        let view = normalize(frame.camera_eye.xyz - in.world_pos);
        let dims = vec2<f32>(textureDimensions(mint_scene_color));
        let offset = normal.xy * river.reflection.z;
        let uv = clamp(
            vec2<f32>(
                1.0 - in.clip_position.x / dims.x - offset.x,
                in.clip_position.y / dims.y + offset.y,
            ),
            vec2<f32>(0.0),
            vec2<f32>(1.0)
        );
        let mirrored = textureSample(reflection_texture, reflection_sampler, uv).rgb;
        let f0 = river.reflection.w;
        let cos_theta = clamp(dot(normal, view), 0.0, 1.0);
        let fresnel = f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
        let weight = clamp(fresnel * river.reflection.y, 0.0, 1.0);
        rgb = mix(rgb, mirrored, weight);
    }

    if (stage > 4.5) {
        return vec4<f32>(normal * 0.5 + vec3<f32>(0.5), 1.0);
    }

    if (river.refraction.x >= 0.5) {
        let here = 1.0 / max(in.clip_position.w, 1e-9);
        let dims = vec2<f32>(textureDimensions(mint_scene_color));
        let uv = refract_uv(in.clip_position, normal, here, dims);
        let behind = textureSample(mint_scene_color, mint_scene_color_sampler, uv).rgb;
        rgb = mix(behind, rgb, alpha);
        alpha = 1.0;
    }

    if (river.stage.z > 0.5) {
        rgb = rgb * alpha;
    }
    rgb = mint_fog(rgb, in.world_pos) * mint_shroud_at(in.world_pos.xy);
    return vec4<f32>(rgb, alpha);
}
