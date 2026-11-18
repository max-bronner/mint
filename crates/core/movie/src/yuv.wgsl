struct VideoParams {
    size: vec4<i32>,
};

@group(1) @binding(0) var<uniform> params: VideoParams;
@group(1) @binding(1) var plane_y: texture_2d<f32>;
@group(1) @binding(3) var plane_u: texture_2d<f32>;
@group(1) @binding(5) var plane_v: texture_2d<f32>;
@group(1) @binding(7) var plane_a: texture_2d<f32>;

struct VideoVertex {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_main(v: MintVertex) -> VideoVertex {
    var out: VideoVertex;
    out.clip = vec4<f32>(v.position.xy, 0.5, 1.0);
    return out;
}

fn sample_byte(plane: texture_2d<f32>, x: i32, y: i32) -> i32 {
    return i32(round(textureLoad(plane, vec2<i32>(x, y), 0).r * 255.0));
}

fn channel(s: i32) -> f32 {
    return f32(clamp((s * 6 - 75) / 5, 0, 255)) / 255.0;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let x = i32(position.x);
    let row = i32(position.y);
    var y = row;
    var chroma_row = row / 2;
    if (params.size.z != 0) {
        if (row >= (params.size.y / 2) * 2) {
            discard;
        }
        y = params.size.y - 1 - row;
        chroma_row = params.size.y / 2 - 1 - row / 2;
    }
    let luma = sample_byte(plane_y, x, y);
    let u = sample_byte(plane_u, x / 2, chroma_row) - 128;
    let v = sample_byte(plane_v, x / 2, chroma_row) - 128;
    let red = luma + (351 * v) / 256;
    let green = luma + (-86 * u) / 256 + (-179 * v) / 256;
    let blue = luma + (443 * u) / 256;
    var alpha = 1.0;
    if (params.size.w != 0) {
        let a = sample_byte(plane_a, x, y);
        alpha = f32(clamp((255 * (a - 23)) / 211, 0, 255)) / 255.0;
    }
    return vec4<f32>(channel(red), channel(green), channel(blue), alpha);
}
