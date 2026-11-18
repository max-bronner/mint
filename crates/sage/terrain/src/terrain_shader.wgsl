struct TerrainBlock {
    cloud: vec4<f32>,
    cloud_factor: vec4<f32>,
    macro_uv: vec4<f32>,
    macro_terms: vec4<f32>,
    shadow: vec4<f32>,
    shroud_uv: vec4<f32>,
    shroud_terms: vec4<f32>,
    relief: vec4<f32>,
    sun: vec4<f32>,
};

@group(1) @binding(1) var base_texture: texture_2d<f32>;
@group(1) @binding(2) var base_sampler: sampler;
@group(1) @binding(3) var normal_texture: texture_2d<f32>;
@group(1) @binding(4) var normal_sampler: sampler;

const SPECULAR_EXPONENT: f32 = 1200.0;
const SPECULAR_SCALE: f32 = 0.9;

fn to_tangent(v: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let t = vec3<f32>(n.z, 0.0, -n.x);
    let b = vec3<f32>(0.0, n.z, -n.y);
    return vec3<f32>(dot(v, t), dot(v, b), dot(v, n));
}

fn sun_lambert(
    n: vec3<f32>,
    to_light: vec3<f32>,
    to_eye: vec3<f32>,
    texel: vec4<f32>,
) -> vec2<f32> {
    let xy = texel.xy * 2.0 - vec2<f32>(1.0);
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    let bumped = normalize(vec3<f32>(xy, z));
    let light = normalize(to_tangent(to_light, n));
    let half_vector = normalize(to_tangent(to_eye + to_light, n));
    let n_dot_l = dot(bumped, light);
    let n_dot_h = dot(bumped, half_vector);
    let lit = select(0.0, 1.0, n_dot_l > 0.0) * select(0.0, 1.0, n_dot_h > 0.0);
    let specular = lit * pow(max(n_dot_h, 0.0), SPECULAR_EXPONENT) * texel.b * SPECULAR_SCALE;
    return vec2<f32>(max(n_dot_l, 0.0), specular);
}

@group(3) @binding(0) var<uniform> terrain: TerrainBlock;
@group(3) @binding(1) var cloud_texture: texture_2d<f32>;
@group(3) @binding(2) var cloud_sampler: sampler;
@group(3) @binding(3) var macro_texture: texture_2d<f32>;
@group(3) @binding(4) var macro_sampler: sampler;
@group(3) @binding(5) var shroud_texture: texture_2d<f32>;
@group(3) @binding(6) var shroud_sampler: sampler;

struct TerrainVertex {
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
fn vs_main(vertex: MintVertex, @builtin(instance_index) instance: u32) -> TerrainVertex {
    slot = instance;
    var out: TerrainVertex;
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
fn fs_main(in: TerrainVertex, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
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
    let normal_texel = textureSample(normal_texture, normal_sampler, uv);
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

    let sun_shade = mix(
        clamp(terrain.shadow.x, 0.0, 1.0),
        1.0,
        mint_shadow_visibility(in.world_position),
    );
    let relief = terrain.relief.x > 0.5;
    let to_eye = normalize(frame.camera_eye.xyz - in.world_position);
    var specular = vec3<f32>(0.0);
    var lightsum = vec3<f32>(0.0);
    for (var i = 0; i < 3; i = i + 1) {
        let travel = frame.light_directions[i].xyz;
        if (dot(travel, travel) > 0.0) {
            let to_light = normalize(-travel);
            var lambert = max(dot(n, to_light), 0.0);
            if (i == 0 && relief) {
                let sun = sun_lambert(n, to_light, to_eye, normal_texel);
                lambert = sun.x;
                specular = frame.light_colors[0].rgb * terrain.sun.x * sun.y * sun_shade;
            }
            var term = frame.light_colors[i].rgb * lambert;
            if (i == 0) {
                term = term * terrain.sun.x * sun_shade;
            }
            lightsum = lightsum + term;
        }
    }
    let normal_lit = clamp(
        lightsum * cd + frame.ambient_color.rgb * ca + ce,
        vec3<f32>(0.0),
        vec3<f32>(ceiling),
    );
    let unlit = instances[slot].params.w > 0.5;
    let lit = select(normal_lit, vec3<f32>(1.0) + ce, unlit);
    let lobe = select(specular, vec3<f32>(0.0), unlit);

    let cloud_uv = in.world_xy * terrain.cloud.x + terrain.cloud.yz;
    let cloud_sample = textureSample(cloud_texture, cloud_sampler, cloud_uv).rgb;
    let cloud = mix(vec3<f32>(1.0), cloud_sample, terrain.cloud.w) * terrain.cloud_factor.rgb;

    let macro_uv = in.world_xy * terrain.macro_uv.xy + terrain.macro_uv.zw;
    let macro_sample = textureSample(macro_texture, macro_sampler, macro_uv).rgb;
    let macro_layer = mix(vec3<f32>(1.0), macro_sample, terrain.macro_terms.x);

    let shroud_uv = in.world_xy * terrain.shroud_uv.xy + terrain.shroud_uv.zw;
    let shroud_sample = textureSample(shroud_texture, shroud_sampler, shroud_uv).r;
    let shroud = mix(1.0, shroud_sample, terrain.shroud_terms.x);

    let alpha = select(sampled.a, 1.0, instances[slot].params.z > 0.5);
    let fogged = mint_fog((sampled.rgb * lit + lobe) * cloud * macro_layer, in.world_position);
    return vec4<f32>(fogged * shroud, alpha);
}
