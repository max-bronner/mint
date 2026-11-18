struct ShadowInstance {
    model: mat4x4<f32>,
    uv: vec4<f32>,
    uv_offset: vec4<f32>,
    height_shear: vec4<f32>,
};

fn shadow_height_shear(index: u32, world: vec3<f32>) -> vec3<f32> {
    let shear = instances[index].height_shear;
    return world + (world.z - shear.w) * shear.xyz;
}

@group(0) @binding(0) var<storage, read> instances: array<ShadowInstance>;

@group(0) @binding(1) var<uniform> light_view_proj: mat4x4<f32>;

struct ShadowVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) color: vec4<f32>,
};

@vertex
fn vs_main(input: ShadowVertex, @builtin(instance_index) index: u32) -> @builtin(position) vec4<f32> {
    return shadow_clip(index, input.position);
}

fn shadow_clip(index: u32, position: vec3<f32>) -> vec4<f32> {
    let placed = instances[index].model * vec4<f32>(position, 1.0);
    let world = vec4<f32>(shadow_height_shear(index, placed.xyz), placed.w);
    return light_view_proj * world;
}

struct ShadowBone {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
};
@group(0) @binding(2) var<storage, read> shadow_bones: array<ShadowBone>;

fn shadow_bone_point(b: ShadowBone, p: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        b.r0.x * p.x + b.r0.y * p.y + b.r0.z * p.z + b.r0.w,
        b.r1.x * p.x + b.r1.y * p.y + b.r1.z * p.z + b.r1.w,
        b.r2.x * p.x + b.r2.y * p.y + b.r2.z * p.z + b.r2.w,
    );
}

struct ShadowSkin {
    @location(6) bones: vec2<u32>,
    @location(7) weights: vec2<f32>,
    @location(8) position2: vec3<f32>,
    @location(9) normal2: vec3<f32>,
};

fn shadow_skin(index: u32, position: vec3<f32>, skin: ShadowSkin) -> vec3<f32> {
    let base = u32(instances[index].uv_offset.w);
    var moved = vec3<f32>(0.0);
    var applied = 0.0;
    if (skin.weights.x > 0.0) {
        moved = moved + shadow_bone_point(shadow_bones[base + skin.bones.x], position) * skin.weights.x;
        applied = applied + skin.weights.x;
    }
    if (skin.weights.y > 0.0) {
        moved = moved + shadow_bone_point(shadow_bones[base + skin.bones.y], skin.position2) * skin.weights.y;
        applied = applied + skin.weights.y;
    }
    if (applied > 0.0) {
        return moved / applied;
    }
    return position;
}

@vertex
fn vs_skinned(
    input: ShadowVertex,
    skin: ShadowSkin,
    @builtin(instance_index) index: u32,
) -> @builtin(position) vec4<f32> {
    return shadow_clip(index, shadow_skin(index, input.position, skin));
}

struct ShadowAlphaOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) cutoff: f32,
};

@vertex
fn vs_alpha(input: ShadowVertex, @builtin(instance_index) index: u32) -> ShadowAlphaOut {
    return shadow_alpha_out(index, input.position, input.uv);
}

@vertex
fn vs_alpha_skinned(
    input: ShadowVertex,
    skin: ShadowSkin,
    @builtin(instance_index) index: u32,
) -> ShadowAlphaOut {
    return shadow_alpha_out(index, shadow_skin(index, input.position, skin), input.uv);
}

fn shadow_alpha_out(index: u32, position: vec3<f32>, uv: vec2<f32>) -> ShadowAlphaOut {
    let linear = instances[index].uv;
    var out: ShadowAlphaOut;
    out.clip_position = shadow_clip(index, position);
    out.uv = vec2<f32>(
        dot(linear.xy, uv),
        dot(linear.zw, uv),
    ) + instances[index].uv_offset.xy;
    out.cutoff = instances[index].uv_offset.z;
    return out;
}

@group(1) @binding(0) var diffuse_texture: texture_2d<f32>;
@group(1) @binding(1) var diffuse_sampler: sampler;

@fragment
fn fs_alpha(input: ShadowAlphaOut) {
    let alpha = textureSample(diffuse_texture, diffuse_sampler, input.uv).a;
    if (alpha < input.cutoff) {
        discard;
    }
}
