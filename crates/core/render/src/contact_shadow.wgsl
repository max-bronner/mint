@vertex
fn vs_contact(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

fn mint_contact_distance(depth: f32) -> f32 {
    let p = frame.depth_params;
    return p.x / (p.y * depth + p.z);
}

fn mint_contact_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fs_contact(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(mint_scene_depth));
    let last = vec2<i32>(textureDimensions(mint_scene_depth)) - vec2<i32>(1);
    let pixel = clamp(vec2<i32>(position.xy), vec2<i32>(0), last);
    let depth = textureLoad(mint_scene_depth, pixel, 0);
    let ndc = vec2<f32>(position.x / size.x * 2.0 - 1.0, 1.0 - position.y / size.y * 2.0);
    let rebuilt = frame.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    let world = rebuilt.xyz / rebuilt.w;
    let normal_raw = cross(dpdx(world), dpdy(world));

    let params = frame.contact_shadow;
    let steps = i32(params.y + 0.5);
    if (steps <= 0 || abs(depth - frame.depth_params.w) < 1e-7) {
        return vec4<f32>(1.0);
    }
    let travel = frame.light_directions[0].xyz;
    if (dot(travel, travel) < 1e-8 || dot(normal_raw, normal_raw) < 1e-20) {
        return vec4<f32>(1.0);
    }
    let to_light = -normalize(travel);
    var normal = normalize(normal_raw);
    if (dot(normal, frame.camera_eye.xyz - world) < 0.0) {
        normal = -normal;
    }
    let facing = dot(normal, to_light);
    if (facing <= 0.0) {
        return vec4<f32>(1.0);
    }

    let jitter = mint_contact_noise(position.xy);
    let reach = params.x;
    let thickness = max(params.z, 1e-3);
    var occluded = 0.0;
    for (var i = 0; i < steps; i = i + 1) {
        let along = (f32(i) + jitter) / f32(steps) * reach;
        let point = world + to_light * along;
        let clip = frame.view_proj * vec4<f32>(point, 1.0);
        if (clip.w <= 0.0) {
            break;
        }
        let at = clip.xy / clip.w;
        if (abs(at.x) > 1.0 || abs(at.y) > 1.0) {
            break;
        }
        let texel = clamp(
            vec2<i32>(vec2<f32>(at.x * 0.5 + 0.5, 0.5 - at.y * 0.5) * size),
            vec2<i32>(0),
            last,
        );
        let scene = mint_contact_distance(textureLoad(mint_scene_depth, texel, 0));
        let behind = clip.w - scene;
        if (behind > 0.002 * clip.w && behind < thickness) {
            occluded = 1.0;
            break;
        }
    }
    if (occluded < 0.5) {
        return vec4<f32>(1.0);
    }
    let sun = mint_shadow_visibility(world) * smoothstep(0.0, 0.25, facing);
    let keep = mix(1.0, params.w, sun);
    return vec4<f32>(keep, keep, keep, 1.0);
}
