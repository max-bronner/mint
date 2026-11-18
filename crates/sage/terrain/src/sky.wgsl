struct SkyBlock {
    horizon_terms: vec4<f32>,
    horizon: vec4<f32>,
    zenith: vec4<f32>,
    cloud: vec4<f32>,
    cloud_shape: vec4<f32>,
};

@group(1) @binding(0) var<uniform> sky: SkyBlock;
@group(1) @binding(1) var cloud_texture: texture_2d<f32>;
@group(1) @binding(2) var cloud_sampler: sampler;

struct SkyVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) direction: vec3<f32>,
};

@vertex
fn vs_main(vertex: MintVertex, @builtin(instance_index) instance: u32) -> SkyVertex {
    slot = instance;
    var out: SkyVertex;
    let world_position = instances[slot].model * vec4<f32>(vertex.position, 1.0);
    let clip = frame.view_proj * world_position;
    out.clip_position = vec4<f32>(clip.xy, 0.0, clip.w);
    out.direction = vertex.position;
    return out;
}

@fragment
fn fs_main(in: SkyVertex) -> @location(0) vec4<f32> {
    let dir = normalize(in.direction);
    if (sky.horizon_terms.x > 0.5) {
        let plane_xy = dir.xy * (sky.cloud_shape.x / max(abs(dir.z), 1.0e-4));
        let uv = plane_xy * sky.cloud.x + sky.cloud.yz;
        let texel = textureSample(cloud_texture, cloud_sampler, uv).rgb;
        return vec4<f32>(texel * sky.horizon_terms.y, 1.0);
    }

    let height = clamp(dir.z, 0.0, 1.0);
    let ramp = pow(height, sky.horizon_terms.w);
    var color = mix(sky.horizon.rgb, sky.zenith.rgb, ramp);

    if (sky.cloud.w > 0.0 && dir.z > 0.0) {
        let fade = smoothstep(0.0, sky.cloud_shape.y, dir.z);
        let plane_xy = dir.xy * (sky.cloud_shape.x / max(dir.z, 1.0e-4));
        let uv = plane_xy * sky.cloud.x + sky.cloud.yz;
        let cloud = textureSample(cloud_texture, cloud_sampler, uv);
        color = mix(color, cloud.rgb, clamp(cloud.a * sky.cloud.w * fade, 0.0, 1.0));
    }

    return vec4<f32>(color, 1.0);
}
