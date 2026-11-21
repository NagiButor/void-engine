struct PointLight {
    pos_radius: vec4<f32>,
    color_power: vec4<f32>,
};

struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    sky_proj: mat4x4<f32>,
    eye: vec4<f32>,
    prev_eye: vec4<f32>,
    proj_params: vec4<f32>,
    sun: vec4<f32>,
    moon: vec4<f32>,
    settings: vec4<f32>,
    sky: vec4<f32>,
    atmos: vec4<f32>,
    gi_vol: vec4<f32>,
    lights: array<PointLight, 16>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var direct_hdr: texture_2d<f32>;
@group(0) @binding(2) var gi_tex: texture_2d<f32>;
@group(0) @binding(3) var vol_tex: texture_2d<f32>;
@group(0) @binding(4) var g_albedo_metal: texture_2d<f32>;
@group(0) @binding(5) var g_normal_rough: texture_2d<f32>;
@group(0) @binding(6) var g_depth: texture_depth_2d;
@group(0) @binding(7) var linear_sampler: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var o: Out;
    o.clip = vec4<f32>(p[i], 0.0, 1.0);
    o.uv = p[i] * vec2<f32>(0.5, -0.5) + 0.5;
    return o;
}

fn linear_z(depth: f32) -> f32 {
    let near = g.proj_params.z;
    let far = g.proj_params.w;
    return (far * near) / max(far - depth * (far - near), 0.0001);
}

fn reconstruct_world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let wp = g.inv_view_proj * ndc;
    return wp.xyz / wp.w;
}

fn upsample_gi(pixel: vec2<i32>, full_dims: vec2<i32>, center_depth: f32, center_n: vec3<f32>) -> vec3<f32> {
    let gi_dims = vec2<i32>(textureDimensions(gi_tex));
    let center_z = linear_z(center_depth);
    let base_gp = clamp(pixel / 2, vec2<i32>(0), gi_dims - 1);
    let sx = select(-1, 1, (pixel.x & 1) == 1);
    let sy = select(-1, 1, (pixel.y & 1) == 1);
    var offs = array<vec2<i32>, 4>(
        vec2<i32>(0, 0),
        vec2<i32>(sx, 0),
        vec2<i32>(0, sy),
        vec2<i32>(sx, sy),
    );
    var spatial = array<f32, 4>(1.0, 0.65, 0.65, 0.42);
    let z_scale = 1.0 / max(center_z * 0.08, 0.08);

    var acc = vec3<f32>(0.0);
    var w_sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let gp = clamp(base_gp + offs[i], vec2<i32>(0), gi_dims - 1);
        let fp = clamp(gp * 2, vec2<i32>(0), full_dims - 1);
        let sd = textureLoad(g_depth, fp, 0);
        if sd >= 0.999999 {
            continue;
        }
        let sn = textureLoad(g_normal_rough, fp, 0).xyz;
        let sz = linear_z(sd);
        let dz = (center_z - sz) * z_scale;
        let w_z = 1.0 / (1.0 + dz * dz);
        let nd = max(dot(center_n, sn), 0.0);
        let nd2 = nd * nd;
        let nd4 = nd2 * nd2;
        let nd8 = nd4 * nd4;
        let w_n = nd8 * nd8;
        let w = w_z * w_n * spatial[i];
        acc += textureLoad(gi_tex, gp, 0).rgb * w;
        w_sum += w;
    }
    if w_sum < 0.001 {
        return textureLoad(gi_tex, base_gp, 0).rgb;
    }
    return acc / w_sum;
}

fn upsample_vol(uv: vec2<f32>, full_dims: vec2<i32>, center_depth: f32) -> vec3<f32> {
    let vol_dims = vec2<f32>(textureDimensions(vol_tex));
    let texel = 1.0 / vol_dims;
    let center_z = linear_z(center_depth);
    var offs = array<vec2<f32>, 4>(
        vec2<f32>(-0.35, -0.35),
        vec2<f32>( 0.35, -0.35),
        vec2<f32>(-0.35,  0.35),
        vec2<f32>( 0.35,  0.35),
    );
    var acc = vec3<f32>(0.0);
    var w_sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let suv = clamp(uv + offs[i] * texel, vec2<f32>(0.001), vec2<f32>(0.999));
        let fp = clamp(vec2<i32>(suv * vec2<f32>(full_dims)), vec2<i32>(0), full_dims - 1);
        let sz = linear_z(textureLoad(g_depth, fp, 0));
        let z_diff = abs(center_z - sz) / max(center_z * 0.14, 0.25);
        let w = exp(-z_diff * z_diff) + 0.005;
        acc += textureSampleLevel(vol_tex, linear_sampler, suv, 0.0).rgb * w;
        w_sum += w;
    }
    return acc / max(w_sum, 0.0001);
}

@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(g_depth));
    let pixel = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), dims - 1);
    let depth = textureLoad(g_depth, pixel, 0);
    let direct_sample = textureLoad(direct_hdr, pixel, 0);
    var color = direct_sample.rgb;

    let use_gi = g.gi_vol.x > 0.5;
    let use_vol = g.gi_vol.z > 0.5;

    if depth >= 0.999999 {
        let vol = select(
            vec3<f32>(0.0),
            textureSampleLevel(vol_tex, linear_sampler, v.uv, 0.0).rgb,
            use_vol
        );
        return vec4<f32>(color + vol, 1.0);
    }

    if use_gi {
        let center_n = normalize(textureLoad(g_normal_rough, pixel, 0).xyz);
        let gi = upsample_gi(pixel, dims, depth, center_n);
        let albedo_metal = textureLoad(g_albedo_metal, pixel, 0);
        color += gi * albedo_metal.rgb * (1.0 - albedo_metal.a * 0.68);
    }

    let vol = select(vec3<f32>(0.0), upsample_vol(v.uv, dims, depth), use_vol);

    // Aerial perspective fog synced with time of day
    let sun = normalize(g.sun.xyz);
    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let night = 1.0 - smoothstep(-0.18, 0.05, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));
    let fog_day = vec3<f32>(0.36, 0.44, 0.54);
    let fog_sunset = vec3<f32>(0.42, 0.24, 0.18);
    let fog_night = vec3<f32>(0.015, 0.022, 0.042);
    let fog_color = mix(mix(fog_night, fog_day, day), fog_sunset, sunset * 0.7) * (1.0 - night * 0.15);

    let world = reconstruct_world(v.uv, depth);
    let fog = 1.0 - exp(-length(g.eye.xyz - world) * 0.004 * g.settings.y);
    color = mix(color, fog_color, fog);

    return vec4<f32>(color + vol, 1.0);
}
