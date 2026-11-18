@group(1) @binding(0) var diffuse_texture: texture_2d<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@group(1) @binding(2) var normal_texture: texture_2d<f32>;
@group(1) @binding(3) var stage2_texture: texture_2d<f32>;
@group(1) @binding(4) var stage2_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_tangent: vec3<f32>,
    @location(3) handedness: f32,
    @location(4) uv2: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) @interpolate(flat) slot: u32,
    @location(7) clip_distance: f32,
    @location(8) world_position: vec3<f32>,
};

fn env_map_uv(mode: i32, world_normal: vec3<f32>, world_position: vec3<f32>) -> vec2<f32> {
    let n = normalize(world_normal);
    if (mode == 1) {
        let cam_n = (frame.view * vec4<f32>(n, 0.0)).xyz;
        return cam_n.xy * 0.5 + vec2<f32>(0.5);
    }
    let incident = normalize(world_position - frame.camera_eye.xyz);
    let reflection = reflect(incident, n);
    if (mode == 2) {
        let cam_r = (frame.view * vec4<f32>(reflection, 0.0)).xyz;
        return cam_r.xy * 0.25 + vec2<f32>(0.5);
    } else if (mode == 3) {
        return n.xy * 0.5 + vec2<f32>(0.5);
    } else if (mode == 4) {
        return reflection.xy * 0.25 + vec2<f32>(0.5);
    }
    return vec2<f32>(0.0);
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) color: vec4<f32>,
    @builtin(instance_index) instance: u32,
) -> VertexOutput {
    return vertex_common(position, normal, uv, tangent, uv2, color, instance);
}

@vertex
fn vs_skinned(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) bones: vec2<u32>,
    @location(7) weights: vec2<f32>,
    @location(8) position2: vec3<f32>,
    @location(9) normal2: vec3<f32>,
    @builtin(instance_index) instance: u32,
) -> VertexOutput {
    let base = u32(instances[instance].surface_flags.y);
    let posed = mint_skin(base, bones, weights, position, normal, tangent.xyz, position2, normal2);
    return vertex_common(
        posed.position,
        posed.normal,
        uv,
        vec4<f32>(posed.tangent, tangent.w),
        uv2,
        color,
        instance,
    );
}

fn vertex_common(
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    tangent: vec4<f32>,
    uv2: vec2<f32>,
    color: vec4<f32>,
    instance: u32,
) -> VertexOutput {
    slot = instance;
    var out: VertexOutput;
    out.slot = instance;
    let placed = instances[slot].model * vec4<f32>(position, 1.0);
    let world_position = vec4<f32>(mint_height_shear(placed.xyz), placed.w);
    out.clip_position = frame.view_proj * world_position;
    out.world_normal = (instances[slot].normal_matrix * vec4<f32>(normal, 0.0)).xyz;
    out.world_tangent = (instances[slot].model * vec4<f32>(tangent.xyz, 0.0)).xyz;
    out.handedness = tangent.w;
    out.uv = uv;
    if (instances[slot].env_map.x > 0.5) {
        out.uv = env_map_uv(i32(instances[slot].env_map.x), out.world_normal, world_position.xyz);
    }
    out.uv2 = uv2;
    if (instances[slot].env_map.y > 0.5) {
        out.uv2 = env_map_uv(i32(instances[slot].env_map.y), out.world_normal, world_position.xyz);
    }
    out.color = color;
    out.clip_distance = mint_clip_distance(world_position.xyz);
    out.world_position = world_position.xyz;
    return out;
}

fn combine_detail_rgb(
    mode: i32,
    local: vec3<f32>,
    other: vec3<f32>,
    local_a: f32,
    other_a: f32,
) -> vec3<f32> {
    if (mode == 1) {
        return other;
    } else if (mode == 2) {
        return local * other;
    } else if (mode == 3) {
        return local + (vec3<f32>(1.0) - local) * other;
    } else if (mode == 4) {
        return local + other;
    } else if (mode == 5) {
        return local - other;
    } else if (mode == 6) {
        return other - local;
    } else if (mode == 7) {
        return mix(other, local, local_a);
    } else if (mode == 8) {
        return mix(other, local, other_a);
    } else if (mode == 9) {
        return clamp(local + other - vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (mode == 10) {
        return clamp((local + other - vec3<f32>(0.5)) * 2.0, vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (mode == 11) {
        return clamp(2.0 * local * other, vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (mode == 12) {
        return clamp(local + local_a * other, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return local;
}

fn combine_detail_alpha(mode: i32, local: f32, other: f32) -> f32 {
    if (mode == 1) {
        return other;
    } else if (mode == 2) {
        return local * other;
    } else if (mode == 3) {
        return local + (1.0 - local) * other;
    }
    return local;
}

fn mint_stage0(texel: vec3<f32>, lit: vec3<f32>) -> vec3<f32> {
    if (instances[slot].surface_flags.w > 0.5) {
        return clamp(texel * clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0)) * 2.0, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return texel * lit;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    slot = in.slot;
    if (in.clip_distance < 0.0) {
        discard;
    }
    let uv = vec2<f32>(
        instances[slot].uv_linear.x * in.uv.x + instances[slot].uv_linear.y * in.uv.y + instances[slot].uv_offset.x,
        instances[slot].uv_linear.z * in.uv.x + instances[slot].uv_linear.w * in.uv.y + instances[slot].uv_offset.y,
    );
    let ambient_from_vertex = instances[slot].color_source.x > 0.5;
    let diffuse_from_vertex = instances[slot].color_source.y > 0.5;
    let dcg_lighting_disabled = instances[slot].color_source.z > 0.5;
    let dcg_multiplies_result = instances[slot].color_source.w > 0.5;
    let texel0 = textureSample(diffuse_texture, material_sampler, uv);
    let bump = instances[slot].surface_flags.z > 0.5;
    let stage0 = select(texel0, vec4<f32>(1.0), bump) * instances[slot].tint
        * vec4<f32>(1.0, 1.0, 1.0, select(1.0, in.color.a, diffuse_from_vertex || dcg_multiplies_result));
    let bump_offset = select(
        vec2<f32>(0.0),
        texel0.r * instances[slot].uv_offset.zw + texel0.g * instances[slot].uv2_offset.zw,
        bump,
    );

    let uv2 = vec2<f32>(
        instances[slot].uv2_linear.x * in.uv2.x + instances[slot].uv2_linear.y * in.uv2.y + instances[slot].uv2_offset.x,
        instances[slot].uv2_linear.z * in.uv2.x + instances[slot].uv2_linear.w * in.uv2.y + instances[slot].uv2_offset.y,
    ) + bump_offset;
    let house = i32(instances[slot].detail_combine.x) == 13;
    let mask_uv = select(uv2, uv, house);
    let grad_x = dpdx(mask_uv);
    let grad_y = dpdy(mask_uv);
    let footprint = max(length(grad_x), length(grad_y));
    let sample1 = textureSampleGrad(
        stage2_texture,
        stage2_sampler,
        mask_uv,
        select(grad_x, vec2<f32>(footprint, 0.0), bump),
        select(grad_y, vec2<f32>(0.0, footprint), bump),
    );
    let stage1 = sample1 * instances[slot].house_color;
    let house_layer = vec4<f32>(
        sample1.r * instances[slot].house_color.rgb
            + sample1.g * instances[slot].house_skin.rgb
            + sample1.b * instances[slot].house_paint.rgb,
        sample1.a,
    );
    let combined_rgb = combine_detail_rgb(
        i32(instances[slot].detail_combine.x),
        stage0.rgb,
        stage1.rgb,
        stage0.a,
        stage1.a,
    );
    let combined_a = combine_detail_alpha(i32(instances[slot].detail_combine.y), stage0.a, stage1.a);
    let vertex_coverage = select(
        1.0,
        clamp(in.color.a, 0.0, 1.0),
        instances[slot].surface_flags.x > 0.5,
    );
    let sampled = vec4<f32>(
        combined_rgb,
        combined_a * vertex_coverage * mint_visibility_mask_alpha(in.world_position),
    );

    if (sampled.a < instances[slot].params.y) {
        discard;
    }

    var n = normalize(in.world_normal);
    if (!front_facing) {
        n = -n;
    }
    let ca = select(instances[slot].material_ambient.rgb, in.color.rgb, ambient_from_vertex);
    let cd = select(instances[slot].material_diffuse.rgb, in.color.rgb, diffuse_from_vertex);
    let ce = instances[slot].material_emissive.rgb;
    let dcg_result_multiplier = select(vec3<f32>(1.0), in.color.rgb, dcg_multiplies_result);
    let ceiling = instances[slot].params.x;

    let light_set = i32(instances[slot].light_add.w + 0.5);
    let cookie = mint_cookie(light_set, in.world_position);
    if (!(dot(n, n) > 0.0)) {
        let flat_alpha = select(sampled.a, 1.0, instances[slot].params.z > 0.5);
        let flat_normal_lit = select(
            clamp((mint_ambient(light_set) + instances[slot].light_add.rgb) * ca + ce, vec3<f32>(0.0), vec3<f32>(ceiling)),
            cd,
            dcg_lighting_disabled,
        );
        let flat_lit = select(flat_normal_lit, vec3<f32>(1.0) + ce, instances[slot].params.w > 0.5);
        let flat_base = mint_stage0(sampled.rgb, flat_lit) * dcg_result_multiplier;
        let flat_rgb = select(flat_base, mix(flat_base, house_layer.rgb, house_layer.a), house) * cookie;
        return vec4<f32>(mint_fog_mode(flat_rgb, in.world_position), flat_alpha);
    }

    let t_raw = in.world_tangent - n * dot(n, in.world_tangent);
    if (dot(t_raw, t_raw) > 1e-12) {
        let t = normalize(t_raw);
        let b = select(-1.0, 1.0, in.handedness >= 0.0) * cross(n, t);
        let sample = textureSample(normal_texture, material_sampler, uv).xyz * 2.0 - 1.0;
        let mapped = sample.x * t + sample.y * b + sample.z * n;
        if (dot(mapped, mapped) > 1e-12) {
            n = normalize(mapped);
        }
    }

    var lightsum = vec3<f32>(0.0);
    for (var i = 0; i < 3; i = i + 1) {
        let travel = mint_light_direction(light_set, i);
        if (dot(travel, travel) > 0.0) {
            lightsum = lightsum + (mint_light_color(light_set, i) + instances[slot].light_add.rgb)
                     * max(dot(n, normalize(-travel)), 0.0);
        }
    }
    lightsum = lightsum * mint_shadow_factor(in.world_position);
    let normal_lit = clamp(
        lightsum * cd + (mint_ambient(light_set) + instances[slot].light_add.rgb) * ca + ce,
        vec3<f32>(0.0),
        vec3<f32>(ceiling),
    );
    let with_dcg_unlit = select(normal_lit, cd, dcg_lighting_disabled);
    let lit = select(with_dcg_unlit, vec3<f32>(1.0) + ce, instances[slot].params.w > 0.5);

    let alpha = select(sampled.a, 1.0, instances[slot].params.z > 0.5);
    let lit_base = mint_stage0(sampled.rgb, lit) * dcg_result_multiplier;
    let final_rgb = select(lit_base, mix(lit_base, house_layer.rgb, house_layer.a), house);
    return vec4<f32>(mint_fog_mode(final_rgb * cookie, in.world_position), alpha);
}
