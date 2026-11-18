struct Params {
    offsets: array<vec4<f32>, 4>,
    weights: vec4<f32>,
    c0: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var lut: texture_3d<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> VOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

@fragment
fn fs_copy(in: VOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(tex, samp, in.uv, 0.0);
}

@fragment
fn fs_bright(in: VOut) -> @location(0) vec4<f32> {
    return saturate(textureSampleLevel(tex, samp, in.uv, 0.0) + params.c0);
}

@fragment
fn fs_blur(in: VOut) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        sum = sum + textureSampleLevel(tex, samp, in.uv + params.offsets[k].xy, 0.0) * params.weights[k];
    }
    return sum;
}

@fragment
fn fs_lut(in: VOut) -> @location(0) vec4<f32> {
    let f = textureSampleLevel(tex, samp, in.uv, 0.0);
    let l = textureSampleLevel(lut, samp, saturate(f.rgb), 0.0);
    return vec4<f32>(mix(f.rgb, l.rgb, params.c0.x), f.a);
}
