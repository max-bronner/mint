struct UiInstance {
    affine: vec4<f32>,
    translation: vec4<f32>,
    uv_rect: vec4<f32>,
    color_multiply: vec4<f32>,
    color_add: vec4<f32>,
    clip: vec4<f32>,
    wedge: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> instances: array<UiInstance>;
@group(0) @binding(1) var<uniform> viewport: vec4<f32>;

@group(1) @binding(0) var mint_ui_texture: texture_2d<f32>;
@group(1) @binding(1) var mint_ui_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_position: vec2<f32>,
    @location(2) color_multiply: vec4<f32>,
    @location(3) color_add: vec4<f32>,
    @location(4) clip: vec4<f32>,
    @location(5) @interpolate(flat) premultiplied: f32,
    @location(6) @interpolate(flat) greyscale: f32,
    @location(7) @interpolate(flat) opaque: f32,
    @location(8) local: vec2<f32>,
    @location(9) @interpolate(flat) disc: f32,
    @location(10) @interpolate(flat) wedge: f32,
    @location(11) @interpolate(flat) hole: f32,
    @location(12) @interpolate(flat) additive: f32,
};

const CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let local = CORNERS[vertex_index % 6u];
    let inst = instances[instance_index];
    let screen = vec2<f32>(
        inst.affine.x * local.x + inst.affine.y * local.y + inst.translation.x,
        inst.affine.z * local.x + inst.affine.w * local.y + inst.translation.y,
    );
    let ndc = vec2<f32>(
        screen.x / viewport.x * 2.0 - 1.0,
        1.0 - screen.y / viewport.y * 2.0,
    );
    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    let uv_flags = u32(inst.translation.w + 0.5);
    var uv_local = local;
    if ((uv_flags & 16u) != 0u) {
        uv_local = vec2<f32>(1.0 - local.y, local.x);
    }
    out.uv = inst.uv_rect.xy + uv_local * inst.uv_rect.zw;
    out.screen_position = screen;
    out.color_multiply = inst.color_multiply;
    out.color_add = inst.color_add;
    out.clip = inst.clip;
    out.premultiplied = inst.translation.z;
    let flags = u32(inst.translation.w + 0.5);
    out.greyscale = f32(flags & 1u);
    out.opaque = f32((flags >> 1u) & 1u);
    out.disc = f32((flags >> 2u) & 1u);
    out.additive = f32((flags >> 3u) & 1u);
    out.local = local;
    out.wedge = inst.wedge.x;
    out.hole = inst.wedge.y;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.screen_position.x < in.clip.x || in.screen_position.x >= in.clip.z ||
        in.screen_position.y < in.clip.y || in.screen_position.y >= in.clip.w) {
        discard;
    }
    if (in.disc > 0.5) {
        let d = in.local * 2.0 - vec2<f32>(1.0, 1.0);
        if (dot(d, d) > 1.0) {
            discard;
        }
    }
    if (in.hole > 0.0) {
        let d = in.local * 2.0 - vec2<f32>(1.0, 1.0);
        if (dot(d, d) < in.hole * in.hole) {
            discard;
        }
    }
    if (in.wedge < 0.999) {
        let d = in.local - vec2<f32>(0.5, 0.5);
        let clockwise = atan2(d.x, -d.y) / (2.0 * 3.14159265);
        let turn = select(clockwise, clockwise + 1.0, clockwise < 0.0);
        if (turn < 1.0 - in.wedge) {
            discard;
        }
    }
    var sampled = textureSample(mint_ui_texture, mint_ui_sampler, in.uv);
    if (in.greyscale > 0.5) {
        let luma = dot(sampled.rgb, vec3<f32>(0.299, 0.587, 0.114));
        sampled = vec4<f32>(luma, luma, luma, sampled.a);
    }
    if (in.opaque > 0.5) {
        sampled.a = 1.0;
    }
    if (in.additive > 0.5) {
        let added = sampled * in.color_multiply + in.color_add;
        return vec4<f32>(added.rgb, 0.0);
    }
    if (in.premultiplied > 0.5) {
        let alpha = clamp(sampled.a * in.color_multiply.a + in.color_add.a, 0.0, 1.0);
        let rgb = sampled.rgb * in.color_multiply.rgb * in.color_multiply.a + in.color_add.rgb * alpha;
        return vec4<f32>(rgb, alpha);
    }
    let straight = sampled * in.color_multiply + in.color_add;
    return vec4<f32>(straight.rgb * straight.a, straight.a);
}
