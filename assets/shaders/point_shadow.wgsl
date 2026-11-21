struct PointShadowGlobals {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> p: PointShadowGlobals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
};

@vertex fn vs_point_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return p.view_proj * mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * vec4<f32>(v.position, 1.0);
}
