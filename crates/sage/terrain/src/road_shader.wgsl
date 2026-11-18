@group(1) @binding(1) var base_texture: texture_2d<f32>;
@group(1) @binding(2) var base_sampler: sampler;

struct RoadVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_xy: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) @interpolate(flat) slot: u32,
    @location(5) clip_distance: f32,
    @location(6) world_position: vec3<f32>,
};

@vertex
fn vs_main(vertex: MintVertex, @builtin(instance_index) instance: u32) -> RoadVertex {
    slot = instance;
    var out: RoadVertex;
    out.slot = instance;
    let world_position = instances[slot].model * vec4<f32>(vertex.position, 1.0);
    out.clip_position = frame.view_proj * world_position;
    out.world_normal = (instances[slot].normal_matrix * vec4<f32>(vertex.normal, 0.0)).xyz;
    out.uv = vertex.uv;
    out.world_xy = world_position.xy;
    out.color = vertex.color;
    out.clip_distance = mint_clip_distance(world_position.xyz);
    out.world_position = world_position.xyz;
    return out;
}

@fragment
fn fs_main(in: RoadVertex, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    slot = in.slot;
    if (in.clip_distance < 0.0) {
        discard;
    }

    let uv = vec2<f32>(
        instances[slot].uv_linear.x * in.uv.x + instances[slot].uv_linear.y * in.uv.y
            + instances[slot].uv_offset.x,
        instances[slot].uv_linear.z * in.uv.x + instances[slot].uv_linear.w * in.uv.y
            + instances[slot].uv_offset.y,
    );
    let stage0 = textureSample(base_texture, base_sampler, uv) * instances[slot].tint;
    let coverage = select(
        1.0,
        clamp(in.color.a, 0.0, 1.0),
        instances[slot].surface_flags.x > 0.5,
    );
    let sampled = vec4<f32>(stage0.rgb, stage0.a * coverage);
    if (sampled.a < instances[slot].params.y) {
        discard;
    }

    var n = normalize(in.world_normal);
    if (!front_facing) {
        n = -n;
    }
    let ca = instances[slot].material_ambient.rgb;
    let cd = instances[slot].material_diffuse.rgb;
    let ce = instances[slot].material_emissive.rgb;
    let ceiling = instances[slot].params.x;
    let alpha = select(sampled.a, 1.0, instances[slot].params.z > 0.5);
    let shroud = mint_shroud_at(in.world_xy);

    if (!(dot(n, n) > 0.0)) {
        let flat_lit = select(
            clamp(frame.ambient_color.rgb * ca + ce, vec3<f32>(0.0), vec3<f32>(ceiling)),
            vec3<f32>(1.0) + ce,
            instances[slot].params.w > 0.5,
        );
        return vec4<f32>(mint_fog(sampled.rgb * flat_lit, in.world_position) * shroud, alpha);
    }

    var lightsum = vec3<f32>(0.0);
    for (var i = 0; i < 3; i = i + 1) {
        let travel = frame.light_directions[i].xyz;
        if (dot(travel, travel) > 0.0) {
            lightsum = lightsum + frame.light_colors[i].rgb
                     * max(dot(n, normalize(-travel)), 0.0);
        }
    }
    lightsum = lightsum * mint_shadow_factor(in.world_position);
    let normal_lit = clamp(
        lightsum * cd + frame.ambient_color.rgb * ca + ce,
        vec3<f32>(0.0),
        vec3<f32>(ceiling),
    );
    let lit = select(normal_lit, vec3<f32>(1.0) + ce, instances[slot].params.w > 0.5);
    return vec4<f32>(mint_fog(sampled.rgb * lit, in.world_position) * shroud, alpha);
}
