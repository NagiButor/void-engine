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
@group(0) @binding(1) var g_depth: texture_depth_2d;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
@group(0) @binding(4) var clouds_tex: texture_2d<f32>;
@group(0) @binding(5) var linear_sampler: sampler;
@group(0) @binding(6) var cloud_noise_3d: texture_3d<f32>;
@group(0) @binding(7) var cloud_sampler: sampler;

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

const PI: f32 = 3.14159265;

fn reconstruct_world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let wp = g.inv_view_proj * ndc;
    return wp.xyz / wp.w;
}

fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn hg_phase(cos_theta: f32, g_aniso: f32) -> f32 {
    let g2 = g_aniso * g_aniso;
    let denom = pow(max(1.0 + g2 - 2.0 * g_aniso * cos_theta, 0.001), 1.5);
    return (1.0 - g2) / (4.0 * PI * denom);
}

fn shadow_at(world: vec3<f32>) -> f32 {
    if g.settings.z < 0.5 {
        return 1.0;
    }
    let p = g.light_proj * vec4<f32>(world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if any(uv < vec2<f32>(0.001)) || any(uv > vec2<f32>(0.999)) || ndc.z > 1.0 || ndc.z < 0.0 {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_map, shadow_sampler, uv, ndc.z - 0.0015);
}

@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    if g.gi_vol.z < 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let sun = normalize(g.sun.xyz);
    let moon = normalize(g.moon.xyz);
    let sun_vis = smoothstep(-0.04, 0.06, sun.y);
    let moon_vis = (1.0 - sun_vis) * smoothstep(-0.03, 0.09, moon.y);
    if sun_vis + moon_vis < 0.005 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let full_dims = vec2<i32>(textureDimensions(g_depth));
    let fp = clamp(vec2<i32>(v.uv * vec2<f32>(full_dims)), vec2<i32>(0), full_dims - 1);
    let depth = textureLoad(g_depth, fp, 0);

    let ro = g.eye.xyz;
    let far_w = reconstruct_world(v.uv, 1.0);
    let rd = normalize(far_w - ro);

    var march_dist = 46.0;
    if depth < 0.999999 {
        let fuv = (vec2<f32>(fp) + 0.5) / vec2<f32>(full_dims);
        let wp = reconstruct_world(fuv, depth);
        march_dist = min(length(wp - ro), 46.0);
    }

    let active_dir = select(moon, sun, sun_vis >= 0.5);
    let cos_theta = dot(rd, active_dir);
    let phase = min(mix(hg_phase(cos_theta, 0.28), hg_phase(cos_theta, 0.74), 0.58), 1.65);

    let sunset = exp(-pow((sun.y - 0.02) / 0.15, 2.0));
    let sun_col = mix(vec3<f32>(1.0, 0.48, 0.18), vec3<f32>(1.0, 0.92, 0.76), smoothstep(0.0, 0.35, sun.y));
    let moon_col = vec3<f32>(0.34, 0.50, 0.88);
    let shaft_color = sun_col * (sun_vis * (1.05 + sunset * 0.75)) + moon_col * (moon_vis * 0.72);

    var sky_trans = 1.0;
    if depth >= 0.999999 && g.atmos.x > 0.5 {
        let cloud_t = textureSampleLevel(clouds_tex, linear_sampler, v.uv, 0.0).a;
        sky_trans = mix(0.45, 1.0, cloud_t);
    }

    let steps = 12;
    let step_len = march_dist / f32(steps);
    let jitter = fract(ign(v.clip.xy) + g.prev_eye.w * 0.61803398875);
    let fog_scale = 0.48 + 0.52 * clamp(g.settings.y, 0.0, 2.0);

    var accum = 0.0;
    var optical = 0.0;
    for (var i = 0; i < steps; i++) {
        let t = (f32(i) + jitter) * step_len;
        let p = ro + rd * t;
        let vis = shadow_at(p);

        let h_dens = exp(-max(p.y, 0.0) * 0.048);
        let dust_sample = textureSampleLevel(
            cloud_noise_3d,
            cloud_sampler,
            p * 0.028 + vec3<f32>(g.sky.x * 0.015, 0.0, 0.0),
            0.0,
        ).a;
        let dust = 0.82 + 0.18 * dust_sample;
        let local_dens = 0.022 * fog_scale * h_dens * dust;

        optical += local_dens * step_len;
        let trans = exp(-optical);
        accum += vis * trans * local_dens * step_len;
    }

    let volumetric = shaft_color * (accum * phase * 2.8 * sky_trans * g.gi_vol.w);
    return vec4<f32>(min(volumetric, vec3<f32>(3.5)), 1.0);
}
