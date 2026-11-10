struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sun: vec4<f32>,
    settings: vec4<f32>, // exposure, fog, shadow quality, time
    lights: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var colors: texture_2d_array<f32>;
@group(0) @binding(2) var normals: texture_2d_array<f32>;
@group(0) @binding(3) var material_sampler: sampler;
@group(0) @binding(4) var shadow_map: texture_depth_2d;
@group(0) @binding(5) var shadow_sampler: sampler_comparison;
@group(0) @binding(6) var tool_color: texture_2d<f32>;

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
    @location(9) uv_scale: vec4<f32>, // uv scale, baked lighting, unused
};
struct Varying {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) surface: vec4<f32>,
    @location(5) uv_scale: vec4<f32>,
};

@vertex fn vs_main(v: VertexIn) -> Varying {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let world = model * vec4<f32>(v.position, 1.0);
    var out: Varying;
    out.clip = g.view_proj * world;
    out.world = world.xyz;
    // Cofactor normal matrix supports non-uniform prop and forearm scaling.
    let a = v.m0.xyz; let b = v.m1.xyz; let c = v.m2.xyz;
    let nmat = mat3x3<f32>(cross(b,c), cross(c,a), cross(a,b));
    out.normal = normalize(nmat * v.normal) * sign(dot(a, cross(b,c)));
    out.uv = v.uv * v.uv_scale.xy;
    out.color = v.color;
    out.surface = v.surface;
    out.uv_scale = v.uv_scale;
    return out;
}

@vertex fn vs_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return g.light_proj * mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * vec4<f32>(v.position, 1.0);
}

fn shadow_visibility(world: vec3<f32>, ndl: f32) -> f32 {
    let p = g.light_proj * vec4<f32>(world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if any(uv < vec2<f32>(0.001)) || any(uv > vec2<f32>(0.999)) || ndc.z > 1.0 || ndc.z < 0.0 { return 1.0; }
    let bias = 0.0003 + (1.0 - ndl) * 0.0012;
    var result = 0.0;
    let texel = 1.0 / f32(textureDimensions(shadow_map).x);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel * select(0.0, 1.4, g.settings.z > 1.5);
            result += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, ndc.z - bias);
        }
    }
    return select(1.0, result / 9.0, g.settings.z > 0.5);
}

fn fresnel(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cosine, 5.0);
}
fn brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, rough: f32, metal: f32) -> vec3<f32> {
    let h = normalize(l + v);
    let nl = max(dot(n,l), 0.001); let nv = max(dot(n,v), 0.001);
    let nh = max(dot(n,h), 0.0); let hv = max(dot(h,v), 0.0);
    let a = rough * rough;
    let a2 = a * a;
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(3.14159265 * denom * denom, 0.00001);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let geometry = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
    let f = fresnel(hv, mix(vec3<f32>(0.04), albedo, metal));
    return ((1.0 - f) * (1.0 - metal) * albedo / 3.14159265 + distribution * geometry * f / (4.0 * nl * nv)) * nl;
}

@fragment fn fs_main(v: Varying) -> @location(0) vec4<f32> {
    let layer = i32(v.surface.w + 0.5);
    var tex = textureSample(colors, material_sampler, v.uv, min(layer, 7));
    let tool_tex = textureSample(tool_color, material_sampler, v.uv);
    if layer == 8 { tex = tool_tex; tex.a = 0.65; }
    let albedo = max(tex.rgb * v.color.rgb, vec3<f32>(0.001));
    let rough = clamp(v.surface.x * tex.a / 0.7, 0.19, 0.98);
    let metal = v.surface.y;
    var n = normalize(v.normal);
    let tangent_normal = textureSample(normals, material_sampler, v.uv, min(layer,7)).xyz * 2.0 - 1.0;
    let dp1 = dpdx(v.world); let dp2 = dpdy(v.world);
    let duv1 = dpdx(v.uv); let duv2 = dpdy(v.uv);
    let t_raw = dp1 * duv2.y - dp2 * duv1.y;
    let b_raw = dp2 * duv1.x - dp1 * duv2.x;
    let orientation = sign(duv1.x * duv2.y - duv1.y * duv2.x);
    let tangent = normalize(t_raw * orientation + vec3<f32>(0.0000001));
    let bitangent = normalize(b_raw * orientation + vec3<f32>(0.0000001));
    if layer != 8 {
        n = normalize(mat3x3<f32>(tangent, bitangent, n) * vec3<f32>(tangent_normal.xy * 0.30, tangent_normal.z));
    }
    let view = normalize(g.eye.xyz - v.world);
    let sun = normalize(g.sun.xyz);
    let visibility = shadow_visibility(v.world + n * 0.008, max(dot(n,sun),0.0));
    let hemi = mix(vec3<f32>(0.10,0.085,0.063),vec3<f32>(0.26,0.32,0.40), n.y * 0.5 + 0.5);
    var light = albedo * hemi * (1.0 - metal * 0.68);
    light += brdf(n,view,sun,albedo,rough,metal) * vec3<f32>(3.8,3.28,2.62) * visibility;
    for (var i = 0; i < 4; i++) {
        let delta = g.lights[i].xyz - v.world;
        let d2 = max(dot(delta,delta),0.1);
        let tint = select(vec3<f32>(0.52,0.73,1.0),vec3<f32>(1.0,0.68,0.36), i == 1 || i == 3);
        light += brdf(n,view,normalize(delta),albedo,rough,metal) * tint * g.lights[i].w / (1.0 + d2);
    }
    // Analytic warehouse reflection supplies a readable rough-metal response.
    let reflected = reflect(-view,n);
    let sky = mix(vec3<f32>(0.10,0.09,0.075),vec3<f32>(0.48,0.59,0.73),clamp(reflected.y * 0.5 + 0.5,0.0,1.0));
    let f = fresnel(max(dot(n,view),0.0),mix(vec3<f32>(0.04),albedo,metal));
    light += sky * f * (1.0 - rough * 0.65) * 0.65;
    light = mix(light, albedo * 0.8, v.uv_scale.z);
    light += albedo * v.surface.z;
    let fog = 1.0 - exp(-length(g.eye.xyz - v.world) * 0.005 * g.settings.y);
    return vec4<f32>(mix(light,vec3<f32>(0.23,0.28,0.34),fog),1.0);
}
