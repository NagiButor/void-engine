struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sun: vec4<f32>,
    settings: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var colors: texture_2d_array<f32>;
@group(0) @binding(2) var normals: texture_2d_array<f32>;
@group(0) @binding(3) var material_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
    @location(7) color: vec4<f32>,
    @location(8) surface: vec4<f32>, // roughness, metallic, emissive, layer
    @location(9) uv_scale: vec4<f32>,
};

struct Varying {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) surface: vec4<f32>,
};

struct GBufferOut {
    @location(0) albedo_metal: vec4<f32>,
    @location(1) normal_rough: vec4<f32>,
    @location(2) emissive: vec4<f32>,
};

@vertex fn vs_main(v: VertexIn) -> Varying {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let world = model * vec4<f32>(v.position, 1.0);
    var out: Varying;
    out.clip = g.view_proj * world;
    out.world = world.xyz;
    let a = v.m0.xyz;
    let b = v.m1.xyz;
    let c = v.m2.xyz;
    let nmat = mat3x3<f32>(cross(b, c), cross(c, a), cross(a, b));
    out.normal = normalize(nmat * v.normal) * sign(dot(a, cross(b, c)));
    out.uv = v.uv * v.uv_scale.xy;
    out.color = v.color;
    out.surface = v.surface;
    return out;
}

@vertex fn vs_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return g.light_proj * mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * vec4<f32>(v.position, 1.0);
}

@vertex fn vs_point_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return g.view_proj * mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * vec4<f32>(v.position, 1.0);
}

@fragment fn fs_main(v: Varying) -> GBufferOut {
    let layer = min(i32(v.surface.w + 0.5), 4);
    let tex = textureSample(colors, material_sampler, v.uv, layer);
    let albedo = max(tex.rgb * v.color.rgb, vec3<f32>(0.001));
    let rough = clamp(v.surface.x * tex.a / 0.7, 0.16, 0.98);
    let metal = clamp(v.surface.y, 0.0, 1.0);

    var n = normalize(v.normal);
    let tangent_normal = textureSample(normals, material_sampler, v.uv, layer).xyz * 2.0 - 1.0;
    let dp1 = dpdx(v.world);
    let dp2 = dpdy(v.world);
    let duv1 = dpdx(v.uv);
    let duv2 = dpdy(v.uv);
    let t_raw = dp1 * duv2.y - dp2 * duv1.y;
    let b_raw = dp2 * duv1.x - dp1 * duv2.x;
    let orientation = sign(duv1.x * duv2.y - duv1.y * duv2.x);
    let tangent = normalize(t_raw * orientation + vec3<f32>(0.0000001));
    let bitangent = normalize(b_raw * orientation + vec3<f32>(0.0000001));
    n = normalize(mat3x3<f32>(tangent, bitangent, n) * vec3<f32>(tangent_normal.xy * 0.35, tangent_normal.z));

    var out: GBufferOut;
    out.albedo_metal = vec4<f32>(albedo, metal);
    out.normal_rough = vec4<f32>(n, rough);
    out.emissive = vec4<f32>(albedo * v.surface.z, 1.0);
    return out;
}
