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
@group(0) @binding(2) var cloud_noise_3d: texture_3d<f32>;
@group(0) @binding(3) var weather_tex: texture_2d<f32>;
@group(0) @binding(4) var clouds_history: texture_2d<f32>;
@group(0) @binding(5) var cloud_sampler: sampler;
@group(0) @binding(6) var linear_sampler: sampler;

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
const R_EARTH: f32 = 6371000.0;
const CLOUD_BOT: f32 = 900.0;
const CLOUD_TOP: f32 = 2250.0;

fn reconstruct_world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let wp = g.inv_view_proj * ndc;
    return wp.xyz / wp.w;
}

fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn remap(v: f32, lo0: f32, hi0: f32, lo1: f32, hi1: f32) -> f32 {
    return lo1 + ((v - lo0) / max(hi0 - lo0, 0.0001)) * (hi1 - lo1);
}

fn hg_phase(cos_theta: f32, g_aniso: f32) -> f32 {
    let g2 = g_aniso * g_aniso;
    let denom = pow(max(1.0 + g2 - 2.0 * g_aniso * cos_theta, 0.001), 1.5);
    return (1.0 - g2) / (4.0 * PI * denom);
}

fn dual_lobe_phase(cos_theta: f32) -> f32 {
    return mix(hg_phase(cos_theta, -0.25), hg_phase(cos_theta, 0.72), 0.68);
}

fn ray_sphere_outer(ro: vec3<f32>, rd: vec3<f32>, radius: f32) -> f32 {
    let b = dot(ro, rd);
    let c = dot(ro, ro) - radius * radius;
    let disc = max(b * b - c, 0.0);
    return max(-b + sqrt(disc), 0.0);
}

fn sample_cloud_density(p: vec3<f32>, h_frac: f32, detail: bool) -> f32 {
    let cov = clamp(g.atmos.y, 0.0, 1.0);
    if cov <= 0.01 {
        return 0.0;
    }
    let wind = vec3<f32>(g.sky.x * 16.0, 0.0, g.sky.x * 6.5);
    let weather = textureSampleLevel(weather_tex, cloud_sampler, (p.xz + wind.xz) * 0.000048, 0.0);

    let threshold = mix(0.74, 0.22, cov);
    let coverage = smoothstep(threshold - 0.16, threshold + 0.22, weather.r);
    if coverage <= 0.002 {
        return 0.0;
    }

    let max_h = mix(0.50, 0.98, coverage * (0.65 + 0.35 * weather.g));
    let bot_grad = smoothstep(0.0, 0.13, h_frac);
    let top_grad = 1.0 - smoothstep(max_h * 0.52, max_h, h_frac);
    let h_prof = bot_grad * top_grad;
    if h_prof <= 0.002 {
        return 0.0;
    }

    let uvw = (p + wind * 0.65) * 0.00024;
    let n = textureSampleLevel(cloud_noise_3d, cloud_sampler, uvw, 0.0);
    let shape = remap(n.r, (1.0 - n.g) * 0.42, 1.0, 0.0, 1.0);
    var density = clamp(remap(shape * h_prof * coverage, 0.16, 0.84, 0.0, 1.0), 0.0, 1.0);

    if detail && density > 0.005 {
        let erode = mix(n.b, n.a, 0.5);
        density = max(density - erode * (1.0 - density) * 0.42, 0.0);
    }

    return density * 0.036;
}

fn light_march(p: vec3<f32>, light_dir: vec3<f32>) -> f32 {
    var optical = 0.0;
    for (var i = 0; i < 2; i++) {
        let step_dist = select(110.0, 320.0, i == 1);
        let sp = p + light_dir * step_dist;
        let alt = length(vec3<f32>(sp.x, R_EARTH + sp.y, sp.z)) - R_EARTH;
        let h_frac = clamp((alt - CLOUD_BOT) / (CLOUD_TOP - CLOUD_BOT), 0.0, 1.0);
        if h_frac > 0.0 && h_frac < 1.0 {
            optical += sample_cloud_density(sp, h_frac, false) * (step_dist * 1.35);
        }
    }
    return optical;
}

@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    if g.atmos.x < 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Early sky-visibility test over the full-res footprint (including 2px tent margin)
    let full_dims = vec2<i32>(textureDimensions(g_depth));
    let center_fp = vec2<i32>(v.uv * vec2<f32>(full_dims));
    let d0 = textureLoad(g_depth, clamp(center_fp + vec2<i32>(-3, -3), vec2<i32>(0), full_dims - 1), 0);
    let d1 = textureLoad(g_depth, clamp(center_fp + vec2<i32>( 3, -3), vec2<i32>(0), full_dims - 1), 0);
    let d2 = textureLoad(g_depth, clamp(center_fp + vec2<i32>(-3,  3), vec2<i32>(0), full_dims - 1), 0);
    let d3 = textureLoad(g_depth, clamp(center_fp + vec2<i32>( 3,  3), vec2<i32>(0), full_dims - 1), 0);
    let dc = textureLoad(g_depth, clamp(center_fp, vec2<i32>(0), full_dims - 1), 0);
    if max(max(d0, d1), max(max(d2, d3), dc)) < 0.999999 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let far_world = reconstruct_world(v.uv, 1.0);
    let rd = normalize(far_world - g.eye.xyz);
    if rd.y <= 0.002 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let sun = normalize(g.sun.xyz);
    let moon = normalize(g.moon.xyz);
    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));
    let sun_vis = smoothstep(-0.06, 0.06, elev);
    let moon_vis = smoothstep(-0.04, 0.10, moon.y) * (1.0 - sun_vis * 0.85);

    let sun_col = mix(vec3<f32>(1.0, 0.42, 0.14), vec3<f32>(1.0, 0.94, 0.85), smoothstep(0.0, 0.35, elev))
        * (2.6 * sun_vis);
    let moon_col = vec3<f32>(0.36, 0.48, 0.72) * (0.48 * moon_vis);

    let primary_dir = select(moon, sun, sun_vis > 0.15);
    let sun_phase = dual_lobe_phase(dot(rd, sun));
    let moon_phase_sc = dual_lobe_phase(dot(rd, moon));

    let amb_top_day = vec3<f32>(0.54, 0.66, 0.84);
    let amb_bot_day = vec3<f32>(0.24, 0.28, 0.38);
    let amb_top_sunset = vec3<f32>(0.68, 0.36, 0.28);
    let amb_bot_sunset = vec3<f32>(0.24, 0.16, 0.24);
    let amb_top_night = vec3<f32>(0.032, 0.048, 0.088);
    let amb_bot_night = vec3<f32>(0.012, 0.018, 0.032);

    let amb_top = mix(mix(amb_top_night, amb_top_day, day), amb_top_sunset, sunset * 0.72);
    let amb_bot = mix(mix(amb_bot_night, amb_bot_day, day), amb_bot_sunset, sunset * 0.65);

    let ro_planet = vec3<f32>(g.eye.x, R_EARTH + g.eye.y, g.eye.z);
    let t_start = ray_sphere_outer(ro_planet, rd, R_EARTH + CLOUD_BOT);
    let t_end_raw = ray_sphere_outer(ro_planet, rd, R_EARTH + CLOUD_TOP);
    let t_end = min(t_end_raw, t_start + 9000.0);
    let march_len = max(t_end - t_start, 1.0);

    let steps = 12;
    let step_len = march_len / f32(steps);
    let jitter = fract(ign(v.clip.xy) + g.prev_eye.w * 0.61803398875);

    var accum = vec3<f32>(0.0);
    var transmittance = 1.0;

    for (var i = 0; i < steps; i++) {
        if transmittance < 0.02 {
            break;
        }
        let t = t_start + (f32(i) + jitter) * step_len;
        let p_world = g.eye.xyz + rd * t;
        let alt = length(vec3<f32>(p_world.x, R_EARTH + p_world.y, p_world.z)) - R_EARTH;
        let h_frac = (alt - CLOUD_BOT) / (CLOUD_TOP - CLOUD_BOT);
        if h_frac <= 0.0 || h_frac >= 1.0 {
            continue;
        }

        let density = sample_cloud_density(p_world, h_frac, true);
        if density <= 0.0002 {
            continue;
        }

        let optical = light_march(p_world, primary_dir);
        let beer = max(exp(-optical * 1.15), exp(-optical * 0.28) * 0.58);
        let powder = 1.0 - exp(-density * 320.0);
        let light_energy = beer * mix(0.65, 1.35, powder);

        let direct_scatter = sun_col * (light_energy * sun_phase)
            + moon_col * (light_energy * moon_phase_sc);
        let ambient = mix(amb_bot, amb_top, clamp(h_frac * 1.15, 0.0, 1.0));
        let sample_lit = direct_scatter + ambient * (0.75 + 0.25 * beer);

        let step_trans = exp(-density * step_len);
        accum += transmittance * sample_lit * (1.0 - step_trans);
        transmittance *= step_trans;
    }

    // High-altitude wispy cirrus layer via precomputed weather B channel (1 texture sample)
    let cirrus_t = 6500.0 / max(rd.y, 0.04);
    if cirrus_t < 60000.0 {
        let cp = (g.eye.xz + rd.xz * cirrus_t) * 0.000032 + vec2<f32>(g.sky.x * 0.0008, g.sky.x * 0.0003);
        let cirrus_n = textureSampleLevel(weather_tex, cloud_sampler, cp, 0.0).b;
        let cirrus_cov = smoothstep(0.54 - g.atmos.y * 0.16, 0.84, cirrus_n) * 0.34 * smoothstep(0.03, 0.20, rd.y);
        if cirrus_cov > 0.002 {
            let cirrus_lit = sun_col * (0.35 + 0.65 * sun_phase) + moon_col * 0.35 + amb_top * 0.85;
            accum += transmittance * cirrus_lit * cirrus_cov;
            transmittance *= (1.0 - cirrus_cov * 0.65);
        }
    }

    let horizon_fade = smoothstep(0.003, 0.13, rd.y);
    accum *= horizon_fade;
    transmittance = mix(1.0, transmittance, horizon_fade);

    var res = vec4<f32>(accum, transmittance);

    // Temporal reprojection for silky-smooth jitter convergence
    if g.prev_eye.w > 1.5 {
        let sky_anchor = g.eye.xyz + rd * 5000.0;
        let prev_clip = g.prev_view_proj * vec4<f32>(sky_anchor, 1.0);
        if prev_clip.w > 0.1 {
            let prev_uv = (prev_clip.xy / prev_clip.w) * vec2<f32>(0.5, -0.5) + 0.5;
            if all(prev_uv > vec2<f32>(0.01)) && all(prev_uv < vec2<f32>(0.99)) {
                let hist = textureSampleLevel(clouds_history, linear_sampler, prev_uv, 0.0);
                res = mix(hist, res, 0.28);
            }
        }
    }

    return res;
}
