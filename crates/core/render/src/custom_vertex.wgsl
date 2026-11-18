struct MintVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) color: vec4<f32>,
};
