struct RetainedVertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) clip: vec4<f32>,
    @location(3) color_add: vec4<f32>,
};

struct TexturedVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) clip: vec4<f32>,
    @location(4) color_add: vec4<f32>,
};

struct RetainedInstance {
    placement_affine: vec4<f32>,
    placement_translation: vec4<f32>,
    uv_affine: vec4<f32>,
    uv_translation: vec4<f32>,
    color: vec4<f32>,
    color_add: vec4<f32>,
    clip: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: vec4<f32>;
@group(1) @binding(0) var<storage, read> solid_instances: array<RetainedInstance>;

fn place(t: RetainedInstance, local: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        t.placement_affine.x * local.x + t.placement_affine.y * local.y + t.placement_translation.x,
        t.placement_affine.z * local.x + t.placement_affine.w * local.y + t.placement_translation.y,
    );
}

fn to_ndc(screen: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        screen.x / viewport.x * 2.0 - 1.0,
        1.0 - screen.y / viewport.y * 2.0,
        0.0,
        1.0,
    );
}

@vertex
fn vs_solid(
    in: RetainedVertexInput,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let t = solid_instances[instance_index];
    let screen = place(t, in.position);
    var out: VertexOutput;
    out.position = to_ndc(screen);
    out.screen_position = screen;
    out.color = t.color;
    out.clip = t.clip;
    out.color_add = t.color_add;
    return out;
}

@fragment
fn fs_solid(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.screen_position.x < in.clip.x || in.screen_position.x >= in.clip.z ||
        in.screen_position.y < in.clip.y || in.screen_position.y >= in.clip.w) {
        discard;
    }
    return in.color + in.color_add;
}

@group(1) @binding(0) var mint_ui_geometry_texture: texture_2d<f32>;
@group(1) @binding(1) var mint_ui_geometry_sampler: sampler;
@group(2) @binding(0) var<storage, read> textured_instances: array<RetainedInstance>;

@vertex
fn vs_textured(
    in: RetainedVertexInput,
    @builtin(instance_index) instance_index: u32,
) -> TexturedVertexOutput {
    let t = textured_instances[instance_index];
    let local = in.position;
    let screen = place(t, local);
    var out: TexturedVertexOutput;
    out.position = to_ndc(screen);
    out.screen_position = screen;
    out.uv = vec2<f32>(
        t.uv_affine.x * local.x + t.uv_affine.y * local.y + t.uv_translation.x,
        t.uv_affine.z * local.x + t.uv_affine.w * local.y + t.uv_translation.y,
    );
    out.color = t.color;
    out.clip = t.clip;
    out.color_add = t.color_add;
    return out;
}

@fragment
fn fs_textured(in: TexturedVertexOutput) -> @location(0) vec4<f32> {
    if (in.screen_position.x < in.clip.x || in.screen_position.x >= in.clip.z ||
        in.screen_position.y < in.clip.y || in.screen_position.y >= in.clip.w) {
        discard;
    }
    let sampled = textureSample(mint_ui_geometry_texture, mint_ui_geometry_sampler, in.uv);
    return sampled * in.color + in.color_add;
}
