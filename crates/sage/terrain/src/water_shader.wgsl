struct Water {
    rotations: vec4<f32>,
    bump_offsets: vec4<f32>,
    wave_offsets: vec4<f32>,
    uv_scales: vec4<f32>,
    env: vec4<f32>,
    terms: vec4<f32>,
    reflection: vec4<f32>,
    depth_terms: vec4<f32>,
    absorption: vec4<f32>,
    body: vec4<f32>,
    refraction: vec4<f32>,
    surface: vec4<f32>,
    planar: vec4<f32>,
    specular: vec4<f32>,
    highlights: vec4<f32>,
    ripple: vec4<f32>,
    ripple_offsets: vec4<f32>,
    ripple_light: vec4<f32>,
    shadow: vec4<f32>,
    wave_terms: vec4<f32>,
    wave_curve: vec4<f32>,
    swells: array<vec4<f32>, 192>,
};

@group(1) @binding(0) var<uniform> water: Water;
@group(1) @binding(1) var bump_texture: texture_2d<f32>;
@group(1) @binding(2) var bump_sampler: sampler;
@group(1) @binding(3) var env_texture: texture_2d<f32>;
@group(1) @binding(4) var env_sampler: sampler;
@group(1) @binding(5) var wave_texture: texture_2d<f32>;
@group(1) @binding(6) var wave_sampler: sampler;
@group(1) @binding(7) var reflection_texture: texture_2d<f32>;
@group(1) @binding(8) var reflection_sampler: sampler;

const UV_BASE: f32 = 600.0;

struct WaterOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) bump0: vec2<f32>,
    @location(1) bump1: vec2<f32>,
    @location(2) env_uv: vec2<f32>,
    @location(3) wave0: vec2<f32>,
    @location(4) wave1: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) @interpolate(flat) instance: u32,
    @location(7) world_pos: vec3<f32>,
    @location(8) world_normal: vec3<f32>,
    @location(9) wave_p: vec2<f32>,
    @location(10) still_depth: f32,
};

fn rot(a: vec2<f32>, p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(p.x * a.x + p.y * a.y, p.y * a.x - p.x * a.y);
}

const WAVE_PI: f32 = 3.14159265;

struct SwellSample {
    displacement: vec3<f32>,
    slope: vec2<f32>,
    fold: f32,
    height: f32,
    peak: f32,
};

fn swell_at(p: vec2<f32>, still_depth: f32) -> SwellSample {
    var out: SwellSample;
    out.displacement = vec3<f32>(0.0);
    out.slope = vec2<f32>(0.0);
    out.fold = 1.0;
    out.height = 0.0;
    out.peak = 0.0;

    let count = i32(water.wave_terms.x);
    if (count <= 0) {
        return out;
    }
    let seconds = water.wave_terms.y;
    let beach = clamp(still_depth / max(water.wave_terms.w, 1e-3), 0.0, 1.0);
    if (beach <= 0.0) {
        return out;
    }

    for (var i = 0; i < count; i = i + 1) {
        let base = clamp(i, 0, 47) * 4;
        let seg = water.swells[base];
        let axis = water.swells[base + 1];
        let size = water.swells[base + 2];
        let life_row = water.swells[base + 3];

        let rel = p - seg.xy;
        let half_width = max(axis.w, 1e-3);
        let along = dot(rel, seg.zw);
        let inside = min(along + half_width, (axis.z + half_width) - along);
        let wing = clamp(inside / half_width, 0.0, 1.0);
        if (wing <= 0.0) {
            continue;
        }

        let cycle = max(life_row.x, 1e-3);
        let life = max(life_row.y, 1e-3);
        let age = fract(seconds / cycle + size.w) * cycle;
        if (age > life) {
            continue;
        }
        let t = clamp(age / life, 0.0, 1.0);
        let rise = smoothstep(0.0, max(water.wave_curve.x, 1e-3), t);
        let fall = 1.0 - smoothstep(clamp(water.wave_curve.y, 0.0, 1.0), 1.0, t);

        let s = dot(rel, axis.xy);
        let d = s - (size.y * age - size.z);
        if (abs(d) >= half_width) {
            continue;
        }
        let theta = WAVE_PI * d / half_width;
        let window = 0.5 * (1.0 + cos(theta));
        let amplitude = size.x * rise * fall * wing * beach;
        let q = life_row.z;

        let height = amplitude * window;
        let drag = -q * amplitude * 0.5 * sin(theta);
        out.displacement = out.displacement + vec3<f32>(axis.xy * drag, height);
        let dh = -amplitude * 0.5 * (WAVE_PI / half_width) * sin(theta);
        let dg = -q * amplitude * 0.5 * (WAVE_PI / half_width) * cos(theta);
        out.slope = out.slope + axis.xy * dh;
        out.fold = out.fold + dg;
        out.height = out.height + height;
        out.peak = out.peak + amplitude;
    }
    return out;
}

fn swell_normal(sample: SwellSample) -> vec3<f32> {
    let fold = max(sample.fold, 0.1);
    return normalize(vec3<f32>(-sample.slope / fold, 1.0));
}

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> WaterOut {
    slot = index;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    let p = world.xy / UV_BASE;
    let still_depth = v.uv2.x;
    let swell = swell_at(world.xy, still_depth);
    let displaced = vec4<f32>(world.xyz + swell.displacement, 1.0);

    var out: WaterOut;
    out.clip_position = frame.view_proj * displaced;
    out.wave_p = world.xy;
    out.still_depth = still_depth;
    out.bump0 = rot(water.rotations.xy, p + water.bump_offsets.xy) * water.uv_scales.xy;
    out.bump1 = rot(water.rotations.zw, p + water.bump_offsets.zw) * water.uv_scales.xy;
    out.env_uv = (p + water.env.zw) * water.env.xy;
    out.wave0 = rot(water.rotations.xy, p + water.wave_offsets.xy) * water.uv_scales.zw;
    out.wave1 = rot(water.rotations.zw, p + water.wave_offsets.zw) * water.uv_scales.zw;
    out.color = v.color;
    out.instance = index;
    out.world_pos = displaced.xyz;
    out.world_normal = (instances[index].normal_matrix * vec4<f32>(v.normal, 0.0)).xyz;
    return out;
}

fn bump_at(uv: vec2<f32>) -> vec3<f32> {
    return 2.0 * textureSampleBias(bump_texture, bump_sampler, uv, water.ripple.z).xyz
        - vec3<f32>(1.0);
}

fn bump_slope(decoded: vec3<f32>) -> vec2<f32> {
    return decoded.xy / max(decoded.z, 0.25);
}

const RIPPLE_OCTAVE_RATIO: f32 = 3.17;

fn detail_bump(world_xy: vec2<f32>) -> vec2<f32> {
    if (water.ripple.x <= 0.0) {
        return vec2<f32>(0.0);
    }
    let coarse = max(water.ripple.y, 1.0);
    let fine = coarse / RIPPLE_OCTAVE_RATIO;
    let a = bump_slope(2.0 * textureSample(
        bump_texture,
        bump_sampler,
        (world_xy + water.ripple_offsets.xy) / coarse
    ).xyz - vec3<f32>(1.0));
    let b = bump_slope(2.0 * textureSample(
        bump_texture,
        bump_sampler,
        (world_xy + water.ripple_offsets.zw) / fine
    ).xyz - vec3<f32>(1.0));
    return water.ripple.x * (a + 0.6 * b);
}

fn surface_normal(world_normal: vec3<f32>, b: vec2<f32>, unit: vec2<f32>) -> vec3<f32> {
    var slope = water.terms.z * unit;
    if (water.surface.x > 0.5) {
        let direction = b / max(length(b), 1e-6);
        slope = water.surface.y * mix(b, direction, water.surface.z);
    }
    return normalize(normalize(world_normal) + vec3<f32>(slope, 0.0));
}

fn reflection_at(
    world_pos: vec3<f32>,
    normal: vec3<f32>,
    position: vec4<f32>,
    dims: vec2<f32>,
    view_distance: f32,
) -> vec4<f32> {
    if (water.reflection.z < 0.5) {
        return vec4<f32>(0.0);
    }
    let view = normalize(frame.camera_eye.xyz - world_pos);
    var rgb: vec3<f32>;
    if (water.planar.x > 0.5) {
        let offset = normal.xy * water.planar.y;
        let uv = clamp(
            vec2<f32>(1.0 - position.x / dims.x - offset.x, position.y / dims.y + offset.y),
            vec2<f32>(0.0),
            vec2<f32>(1.0)
        );
        let blur = water.ripple.w
            * clamp(view_distance / max(water.refraction.z, 1.0), 0.0, 1.0);
        rgb = textureSample(reflection_texture, reflection_sampler, uv).rgb;
        if (blur > 1e-4) {
            let step = vec2<f32>(blur * dims.y / max(dims.x, 1.0), blur);
            for (var i = 0u; i < 4u; i = i + 1u) {
                let a = 1.5707963 * f32(i) + 0.7853982;
                let tap = clamp(
                    uv + vec2<f32>(cos(a), sin(a)) * step,
                    vec2<f32>(0.0),
                    vec2<f32>(1.0)
                );
                rgb = rgb + textureSample(reflection_texture, reflection_sampler, tap).rgb;
            }
            rgb = rgb * 0.2;
        }
    } else {
        let r = reflect(-view, normal);
        let denom = max(1.0 + r.z, 1e-3);
        let uv = clamp(r.xy / denom * 0.5 + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
        rgb = textureSample(env_texture, env_sampler, uv).rgb;
    }

    let f0 = water.reflection.y;
    let cos_theta = clamp(dot(normal, view), 0.0, 1.0);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
    let floored = max(fresnel, clamp(water.reflection.w, 0.0, 1.0));
    let f = select(floored, 1.0, water.reflection.z > 1.5);
    if (water.reflection.z > 2.5) {
        return vec4<f32>(rgb, 1.0);
    }
    return vec4<f32>(rgb, clamp(f * water.reflection.x, 0.0, 1.0));
}

fn water_depth_at(texel: vec2<i32>, position: vec4<f32>, world_pos: vec3<f32>) -> vec2<f32> {
    let terms = frame.depth_params;
    let stored = textureLoad(mint_scene_depth, texel, 0);
    let behind = terms.x / (terms.y * stored + terms.z);
    let here = 1.0 / max(position.w, 1e-9);
    let to_pixel = world_pos - frame.camera_eye.xyz;
    let span = length(to_pixel);
    let path = max(behind - here, 0.0) * (span / max(here, 1e-9));
    let down = max(-to_pixel.z / max(span, 1e-9), 0.0);
    return vec2<f32>(path * down, path);
}

fn refract_uv(position: vec4<f32>, normal: vec3<f32>, here: f32, dims: vec2<f32>) -> vec2<f32> {
    let screen_uv = position.xy / dims;
    if (water.refraction.x < 0.5) {
        return screen_uv;
    }
    let falloff = clamp(water.refraction.z / max(here, 1e-3), 0.0, 1.0);
    let uv = clamp(
        screen_uv + normal.xy * water.refraction.y * falloff,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    if (water.refraction.x > 1.5) {
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

fn ripple_light(b: vec2<f32>) -> f32 {
    if (water.ripple_light.x <= 0.0) {
        return 1.0;
    }
    let sun = -frame.light_directions[0].xy;
    let toward = sun / max(length(sun), 1e-3);
    let facing = dot(b, toward) / max(length(b), 1e-6);
    return clamp(1.0 + water.ripple_light.x * facing, 0.0, 2.0);
}

fn specular_at(world_pos: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let to_light = -normalize(frame.light_directions[0].xyz);
    let view = normalize(frame.camera_eye.xyz - world_pos);
    let n_l = dot(normal, to_light);
    let n_v = dot(normal, view);
    if (n_l <= 0.0 || n_v <= 0.0) {
        return vec3<f32>(0.0);
    }
    let h = normalize(to_light + view);
    let n_h = max(dot(normal, h), 0.0);
    let a = sqrt(2.0 / max(water.specular.w + 2.0, 2.0));
    let a2 = max(a * a, 1e-6);
    let denom = n_h * n_h * (a2 - 1.0) + 1.0;
    let d = a2 / (3.14159265 * max(denom * denom, 1e-6));
    let k = a2 * 0.5;
    let g = 1.0 / max((n_l * (1.0 - k) + k) * (n_v * (1.0 - k) + k), 1e-4);
    let f0 = water.reflection.y;
    let f = f0 + (1.0 - f0) * pow(1.0 - clamp(dot(h, view), 0.0, 1.0), 5.0);
    let light = frame.light_colors[0].rgb;
    return water.specular.rgb * light * (d * g * f * n_l * 0.25);
}

fn whitecap(uv0: vec2<f32>, uv1: vec2<f32>) -> vec3<f32> {
    return textureSample(wave_texture, wave_sampler, uv0).rgb
        * textureSample(wave_texture, wave_sampler, uv1).rgb;
}

@fragment
fn fs_main(in: WaterOut) -> @location(0) vec4<f32> {
    slot = in.instance;

    let layer0 = bump_at(in.bump0);
    let layer1 = bump_at(in.bump1);
    let b = layer0.xy + layer1.xy;
    let unit = normalize(vec3<f32>(b, 2e-5)).xy;
    let slope_b = bump_slope(layer0) + bump_slope(layer1) + detail_bump(in.wave_p);
    let swell = swell_at(in.wave_p, in.still_depth);
    let normal = surface_normal(swell_normal(swell), slope_b, unit);

    let here = 1.0 / max(in.clip_position.w, 1e-9);
    let dims = vec2<f32>(textureDimensions(mint_scene_color));
    let refract_at = refract_uv(in.clip_position, normal, here, dims);
    let refracting = water.refraction.x >= 0.5;

    let mode = water.depth_terms.x;
    let fade = max(water.depth_terms.y, 1e-4);
    let max_alpha = water.depth_terms.z;
    var body = in.color.rgb;
    var emit_color = in.color.rgb;
    var alpha = in.color.a;
    var vertical = 0.0;
    var transmit = vec3<f32>(-1.0);
    if (mode >= 0.5) {
        let measured = water_depth_at(vec2<i32>(in.clip_position.xy), in.clip_position, in.world_pos);
        vertical = measured.x;
        var path = measured.y;
        if (refracting) {
            path = water_depth_at(vec2<i32>(refract_at * dims), in.clip_position, in.world_pos).y;
        }
        if (mode < 1.5) {
            alpha = clamp(vertical / fade, 0.0, 1.0) * max_alpha;
            if (water.body.w > 0.5) {
                body = water.body.rgb;
                emit_color = body;
            }
        } else if (mode < 2.5) {
            transmit = exp(-water.absorption.rgb * path);
            let lum = dot(transmit, vec3<f32>(0.2126, 0.7152, 0.0722));
            let opacity = 1.0 - lum;
            alpha = clamp(opacity * max_alpha, 0.0, 1.0);
            let hue = (vec3<f32>(1.0) - transmit) / max(opacity, 1e-3);
            body = water.body.rgb * clamp(hue, vec3<f32>(0.0), vec3<f32>(2.0));
            emit_color = water.body.rgb;
        }
    }

    let diffuse = body * water.terms.x;

    let shade = mix(
        clamp(water.shadow.x, 0.0, 1.0),
        1.0,
        mint_shadow_visibility(in.world_pos),
    );

    var rgb: vec3<f32>;
    if (water.terms.w < 0.5) {
        let env = textureSample(env_texture, env_sampler, in.env_uv + water.terms.z * unit).rgb;
        let foam = water.terms.y
            * textureSample(wave_texture, wave_sampler, in.wave0).rgb
            * textureSample(wave_texture, wave_sampler, in.wave1).rgb;
        let reflection = reflection_at(in.world_pos, normal, in.clip_position, dims, here);
        let ripple_l = ripple_light(slope_b);
        rgb = mix(diffuse * env * ripple_l * shade, reflection.rgb * ripple_l, reflection.a)
            + foam;

        if (refracting) {
            let behind = textureSample(mint_scene_color, mint_scene_color_sampler, refract_at).rgb;
            var t = vec3<f32>(clamp(1.0 - alpha, 0.0, 1.0));
            if (transmit.x >= 0.0) {
                t = clamp(transmit, vec3<f32>(0.0), vec3<f32>(1.0));
            }
            t = mix(vec3<f32>(1.0), t, clamp(max_alpha, 0.0, 1.0));
            let emitted = emit_color * water.terms.x * env * ripple_l * shade;
            let through = behind * t + emitted * (vec3<f32>(1.0) - t);
            if (water.planar.z > 0.5) {
                let surface_rgb = mix(emitted, reflection.rgb * ripple_l, reflection.a);
                rgb = behind * t + surface_rgb * (vec3<f32>(1.0) - t) + foam;
            } else {
                rgb = mix(through, reflection.rgb * ripple_l, reflection.a) + foam;
            }
            alpha = 1.0;
        }
    } else {
        let low = bump_at(in.bump0).xy;
        let env = textureSample(env_texture, env_sampler, in.env_uv + water.terms.z * low).rgb;
        let foam = water.terms.y * textureSample(wave_texture, wave_sampler, in.wave0).rgb;
        rgb = diffuse * env + foam;
    }

    let rung = water.highlights.x;
    if (rung >= 0.5 && water.terms.w < 0.5) {
        let all = rung > 3.5;
        if (all || rung < 1.5) {
            rgb = rgb + specular_at(in.world_pos, normal);
        }
        if (all || (rung > 1.5 && rung < 2.5)) {
            if (mode >= 0.5) {
                let band = max(water.highlights.y, 1e-3);
                let shore = 1.0 - clamp(vertical / band, 0.0, 1.0);
                rgb = rgb + whitecap(in.wave0, in.wave1) * water.highlights.w * shore * shore;
            }
        }
        if (all || (rung > 2.5 && rung < 3.5)) {
            let fold = smoothstep(0.95, 0.65, swell.fold);
            let steep = smoothstep(
                water.highlights.z,
                water.highlights.z + 0.5,
                length(slope_b)
            );
            let lift = clamp(swell.height / max(swell.peak, 1e-3), 0.0, 1.0);
            let bright = pow(lift, max(water.wave_curve.w, 1e-3)) * water.wave_curve.z;
            let crest = max(max(fold * water.wave_terms.z, steep), bright);
            rgb = rgb + whitecap(in.wave0, in.wave1) * water.highlights.w * crest;
        }
    }

    if (mode > 2.5) {
        rgb = vec3<f32>(clamp(vertical / fade, 0.0, 1.0));
        alpha = 1.0;
    }
    if (water.depth_terms.w > 0.5 && mode >= 0.5) {
        rgb = rgb * alpha;
    }

    rgb = mint_fog(rgb, in.world_pos) * mint_shroud_at(in.world_pos.xy);
    return vec4<f32>(rgb, alpha);
}
