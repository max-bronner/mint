struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_eye: vec4<f32>,
    light_directions: array<vec4<f32>, 3>,
    light_colors: array<vec4<f32>, 3>,
    ambient_color: vec4<f32>,
    depth_params: vec4<f32>,
    clip_plane: vec4<f32>,
    shadow_cascades: array<mat4x4<f32>, 4>,
    shadow_params: vec4<f32>,
    shadow_filter: vec4<f32>,
    shadow_cascade_params: array<vec4<f32>, 4>,
    set_directions: array<vec4<f32>, 12>,
    set_colors: array<vec4<f32>, 12>,
    set_ambient: array<vec4<f32>, 4>,
    cookie: vec4<f32>,
    cookie_tint: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    contact_shadow: vec4<f32>,
    fog: array<vec4<f32>, 2>,
    visibility_mask: array<vec4<f32>, 2>,
};
@group(0) @binding(4) var<uniform> frame: Frame;
@group(0) @binding(5) var mint_cookie_texture: texture_2d<f32>;
@group(0) @binding(6) var mint_cookie_sampler: sampler;
@group(0) @binding(7) var mint_mask_texture: texture_2d<f32>;
@group(0) @binding(8) var mint_mask_sampler: sampler;

fn mint_light_direction(rig: i32, i: i32) -> vec3<f32> {
    if (rig <= 0) {
        return frame.light_directions[i].xyz;
    }
    return frame.set_directions[min((rig - 1) * 3 + i, 11)].xyz;
}

fn mint_light_color(rig: i32, i: i32) -> vec3<f32> {
    if (rig <= 0) {
        return frame.light_colors[i].rgb;
    }
    return frame.set_colors[min((rig - 1) * 3 + i, 11)].rgb;
}

fn mint_ambient(rig: i32) -> vec3<f32> {
    if (rig <= 0) {
        return frame.ambient_color.rgb;
    }
    return frame.set_ambient[min(rig - 1, 3)].rgb;
}

fn mint_cookie(rig: i32, world_position: vec3<f32>) -> vec3<f32> {
    if (rig <= 0 || frame.set_ambient[min(rig - 1, 3)].w < 0.5) {
        return vec3<f32>(1.0);
    }
    let uv = world_position.xy * frame.cookie.x + frame.cookie.yz;
    let sample = textureSampleLevel(mint_cookie_texture, mint_cookie_sampler, uv, 0.0).rgb;
    return mix(vec3<f32>(1.0), sample, frame.cookie.w) * frame.cookie_tint.rgb;
}

fn mint_fog_keep(world_position: vec3<f32>) -> f32 {
    let distance = length(world_position - frame.camera_eye.xyz);
    return 1.0 - frame.fog[0].w * clamp((distance - frame.fog[1].x) * frame.fog[1].y, 0.0, 1.0);
}

fn mint_visibility_mask(world_position: vec3<f32>) -> vec3<f32> {
    let uv = world_position.xy * frame.visibility_mask[0].xy + frame.visibility_mask[0].zw;
    let sample = textureSampleLevel(mint_mask_texture, mint_mask_sampler, uv, 0.0).rgb;
    return mix(vec3<f32>(1.0), sample, frame.visibility_mask[1].x);
}

fn mint_visibility_mask_alpha(world_position: vec3<f32>) -> f32 {
    if (instances[slot].env_map.w < 1.5) {
        return 1.0;
    }
    let uv = world_position.xy * frame.visibility_mask[0].xy + frame.visibility_mask[0].zw;
    let sample = textureSampleLevel(mint_mask_texture, mint_mask_sampler, uv, 0.0).a;
    return mix(1.0, sample, frame.visibility_mask[1].x);
}

fn mint_fog(rgb: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    return mix(frame.fog[0].rgb, rgb, mint_fog_keep(world_position));
}

struct Uniforms {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    tint: vec4<f32>,
    params: vec4<f32>,
    uv_linear: vec4<f32>,
    uv_offset: vec4<f32>,
    material_ambient: vec4<f32>,
    material_diffuse: vec4<f32>,
    material_emissive: vec4<f32>,
    uv2_linear: vec4<f32>,
    uv2_offset: vec4<f32>,
    detail_combine: vec4<f32>,
    house_color: vec4<f32>,
    house_skin: vec4<f32>,
    house_paint: vec4<f32>,
    env_map: vec4<f32>,
    color_source: vec4<f32>,
    surface_flags: vec4<f32>,
    height_shear: vec4<f32>,
    light_add: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> instances: array<Uniforms>;

struct MintBone {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
};
@group(0) @binding(3) var<storage, read> mint_bones: array<MintBone>;

struct MintSkinned {
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
};

fn mint_bone_point(b: MintBone, p: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        b.r0.x * p.x + b.r0.y * p.y + b.r0.z * p.z + b.r0.w,
        b.r1.x * p.x + b.r1.y * p.y + b.r1.z * p.z + b.r1.w,
        b.r2.x * p.x + b.r2.y * p.y + b.r2.z * p.z + b.r2.w,
    );
}

fn mint_bone_vector(b: MintBone, v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        b.r0.x * v.x + b.r0.y * v.y + b.r0.z * v.z,
        b.r1.x * v.x + b.r1.y * v.y + b.r1.z * v.z,
        b.r2.x * v.x + b.r2.y * v.y + b.r2.z * v.z,
    );
}

fn mint_safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    if (l > 0.0) {
        return v / l;
    }
    return vec3<f32>(0.0);
}

fn mint_skin(
    base: u32,
    bones: vec2<u32>,
    weights: vec2<f32>,
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
    position2: vec3<f32>,
    normal2: vec3<f32>,
) -> MintSkinned {
    var moved = vec3<f32>(0.0);
    var turned = vec3<f32>(0.0);
    var swung = vec3<f32>(0.0);
    var applied = 0.0;
    if (weights.x > 0.0) {
        let b = mint_bones[base + bones.x];
        moved = moved + mint_bone_point(b, position) * weights.x;
        turned = turned + mint_bone_vector(b, normal) * weights.x;
        swung = swung + mint_bone_vector(b, tangent) * weights.x;
        applied = applied + weights.x;
    }
    if (weights.y > 0.0) {
        let b = mint_bones[base + bones.y];
        moved = moved + mint_bone_point(b, position2) * weights.y;
        turned = turned + mint_bone_vector(b, normal2) * weights.y;
        swung = swung + mint_bone_vector(b, tangent) * weights.y;
        applied = applied + weights.y;
    }
    var out: MintSkinned;
    if (applied > 0.0) {
        out.position = moved / applied;
        out.normal = mint_safe_normalize(turned);
        out.tangent = mint_safe_normalize(swung);
    } else {
        out.position = position;
        out.normal = normal;
        out.tangent = tangent;
    }
    return out;
}

var<private> slot: u32 = 0u;

fn mint_height_shear(world: vec3<f32>) -> vec3<f32> {
    let shear = instances[slot].height_shear;
    return world + (world.z - shear.w) * shear.xyz;
}

fn mint_clip_distance(world_position: vec3<f32>) -> f32 {
    let plane = frame.clip_plane;
    return dot(plane.xyz, world_position) + plane.w;
}

@group(0) @binding(1) var mint_shadow_map: texture_depth_2d_array;
@group(0) @binding(2) var mint_shadow_sampler: sampler_comparison;

fn mint_shadow_visibility(world_position: vec3<f32>) -> f32 {
    let params = frame.shadow_params;
    if (params.x < 0.5) {
        return 1.0;
    }
    let count = clamp(i32(params.z + 0.5), 1, 4);
    let size = f32(textureDimensions(mint_shadow_map).x);
    let guard = 2.0 * (mint_shadow_reach(size) + 1.0) / size;
    let band = max(frame.shadow_filter.x, 0.0);
    for (var cascade = 0; cascade < count; cascade = cascade + 1) {
        let point = mint_shadow_project(cascade, world_position);
        let last = cascade + 1 >= count;
        if (point.w < 0.0 || (point.w < guard && !last)) {
            continue;
        }
        var lit = mint_shadow_filtered(cascade, point.xy, point.z);
        if (!last && band > 0.0 && point.w < guard + band) {
            let next = mint_shadow_project(cascade + 1, world_position);
            if (next.w >= 0.0) {
                let coarse = mint_shadow_filtered(cascade + 1, next.xy, next.z);
                lit = mix(coarse, lit, (point.w - guard) / band);
            }
        }
        return lit;
    }
    return 1.0;
}

fn mint_shadow_project(cascade: i32, world_position: vec3<f32>) -> vec4<f32> {
    let clip = frame.shadow_cascades[cascade] * vec4<f32>(world_position, 1.0);
    if (clip.w <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    let ndc = clip.xyz / clip.w;
    if (ndc.z > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let reference = ndc.z - frame.shadow_cascade_params[cascade].x;
    return vec4<f32>(uv, reference, 1.0 - max(abs(ndc.x), abs(ndc.y)));
}

fn mint_shadow_reach(size: f32) -> f32 {
    let mode = frame.shadow_filter.y;
    if (mode > 1.5) {
        return max(frame.shadow_filter.w, 1.0);
    }
    if (mode > 0.5) {
        return 1.5;
    }
    return frame.shadow_params.w * size;
}

fn mint_shadow_filtered(cascade: i32, uv: vec2<f32>, reference: f32) -> f32 {
    let mode = frame.shadow_filter.y;
    if (mode > 1.5) {
        return mint_shadow_contact(cascade, uv, reference, frame.shadow_params.w);
    }
    if (mode > 0.5) {
        return mint_shadow_tent(cascade, uv, reference);
    }
    let step = frame.shadow_params.w;
    if (step <= 0.0) {
        return textureSampleCompareLevel(
            mint_shadow_map, mint_shadow_sampler, uv, cascade, reference);
    }
    var lit = 0.0;
    lit = lit + textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, uv + vec2<f32>(-step, -step), cascade, reference);
    lit = lit + textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, uv + vec2<f32>(step, -step), cascade, reference);
    lit = lit + textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, uv + vec2<f32>(-step, step), cascade, reference);
    lit = lit + textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, uv + vec2<f32>(step, step), cascade, reference);
    return lit * 0.25;
}

fn mint_shadow_tent(cascade: i32, uv: vec2<f32>, reference: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(mint_shadow_map));
    let texel = uv * size;
    let base = floor(texel + vec2<f32>(0.5));
    let f = texel + vec2<f32>(0.5) - base;
    let origin = base - vec2<f32>(0.5);
    let w0 = vec2<f32>(3.0) - 2.0 * f;
    let w1 = vec2<f32>(1.0) + 2.0 * f;
    let o0 = (vec2<f32>(2.0) - f) / w0 - vec2<f32>(1.0);
    let o1 = f / w1 + vec2<f32>(1.0);
    var lit = 0.0;
    lit = lit + w0.x * w0.y * textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, (origin + vec2<f32>(o0.x, o0.y)) / size, cascade,
        reference);
    lit = lit + w1.x * w0.y * textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, (origin + vec2<f32>(o1.x, o0.y)) / size, cascade,
        reference);
    lit = lit + w0.x * w1.y * textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, (origin + vec2<f32>(o0.x, o1.y)) / size, cascade,
        reference);
    lit = lit + w1.x * w1.y * textureSampleCompareLevel(
        mint_shadow_map, mint_shadow_sampler, (origin + vec2<f32>(o1.x, o1.y)) / size, cascade,
        reference);
    return lit / 16.0;
}

fn mint_shadow_contact(cascade: i32, uv: vec2<f32>, reference: f32, narrowest: f32) -> f32 {
    let soft = frame.shadow_filter;
    let layer = frame.shadow_cascade_params[cascade];
    let dimensions = textureDimensions(mint_shadow_map);
    let size = vec2<f32>(dimensions);
    let last = vec2<i32>(dimensions) - vec2<i32>(1);
    let widest = max(soft.w, 1.0);
    let centre = uv * size - vec2<f32>(0.5);
    var blockers = 0.0;
    var blocker_depth = 0.0;
    for (var k = 0; k < 17; k = k + 1) {
        var offset = vec2<f32>(0.0);
        if (k < 16) {
            offset = (vec2<f32>(f32(k % 4), f32(k / 4)) - vec2<f32>(1.5)) * (widest / 1.5);
        }
        let texel = clamp(vec2<i32>(round(centre + offset)), vec2<i32>(0), last);
        let depth = textureLoad(mint_shadow_map, texel, cascade, 0);
        if (depth < reference) {
            blockers = blockers + 1.0;
            blocker_depth = blocker_depth + depth;
        }
    }
    if (blockers < 0.5) {
        return textureSampleCompareLevel(
            mint_shadow_map, mint_shadow_sampler, uv, cascade, reference);
    }
    if (blockers > 16.5) {
        return 0.0;
    }
    let gap = max(reference - blocker_depth / blockers, 0.0) * layer.z;
    let texels = clamp(gap * soft.z / max(layer.y, 1e-6), narrowest * size.x, widest);
    let radius = 0.5 * texels / size;
    var disk = array<vec2<f32>, 16>(
        vec2<f32>(-0.94201624, -0.39906216), vec2<f32>(0.94558609, -0.76890725),
        vec2<f32>(-0.09418410, -0.92938870), vec2<f32>(0.34495938, 0.29387760),
        vec2<f32>(-0.91588581, 0.45771432), vec2<f32>(-0.81544232, -0.87912464),
        vec2<f32>(-0.38277543, 0.27676845), vec2<f32>(0.97484398, 0.75648379),
        vec2<f32>(0.44323325, -0.97511554), vec2<f32>(0.53742981, -0.47373420),
        vec2<f32>(-0.26496911, -0.41893023), vec2<f32>(0.79197514, 0.19090188),
        vec2<f32>(-0.24188840, 0.99706507), vec2<f32>(-0.81409955, 0.91437590),
        vec2<f32>(0.19984126, 0.78641367), vec2<f32>(0.14383161, -0.14100790),
    );
    var lit = 0.0;
    for (var i = 0; i < 16; i = i + 1) {
        lit = lit + textureSampleCompareLevel(
            mint_shadow_map, mint_shadow_sampler, uv + disk[i] * radius, cascade, reference);
    }
    return lit / 16.0;
}

fn mint_shadow_factor(world_position: vec3<f32>) -> f32 {
    return mix(
        frame.shadow_params.y,
        1.0,
        mint_shadow_visibility(world_position),
    );
}

fn mint_fog_mode(rgb: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    var masked = vec3<f32>(1.0);
    if (instances[slot].env_map.w > 0.5) {
        masked = mint_visibility_mask(world_position);
    }
    return mint_fogged(rgb, world_position) * masked;
}

fn mint_fogged(rgb: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let mode = i32(instances[slot].env_map.z + 0.5);
    if (mode == 0) {
        return rgb;
    }
    let keep = mint_fog_keep(world_position);
    if (mode == 2) {
        return rgb * keep;
    }
    if (mode == 3) {
        return mix(vec3<f32>(1.0), rgb, keep);
    }
    return mix(frame.fog[0].rgb, rgb, keep);
}
