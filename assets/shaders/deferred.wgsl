struct PointLight {
    pos_radius: vec4<f32>,   // xyz = world pos, w = radius
    color_power: vec4<f32>,  // rgb = color, w = intensity
};

struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    sky_proj: mat4x4<f32>,
    eye: vec4<f32>,          // xyz = eye, w = 1.0
    prev_eye: vec4<f32>,     // xyz = prev_eye, w = frame_index
    proj_params: vec4<f32>,  // x = 1/P00, y = 1/P11, z = near, w = far
    sun: vec4<f32>,          // xyz = sun dir, w = active point light count
    moon: vec4<f32>,         // xyz = moon dir, w = moon phase
    settings: vec4<f32>,     // x = exposure, y = fog, z = shadow quality, w = ao enabled
    sky: vec4<f32>,          // x = time, y = time_of_day, z = sun_azimuth_rad, w = sun_tilt_rad
    atmos: vec4<f32>,        // x = clouds_enabled, y = cloud_coverage, z = show_trajectory, w = aspect
    gi_vol: vec4<f32>,       // x = gi_enabled, y = gi_intensity, z = vol_enabled, w = vol_intensity
    lights: array<PointLight, 16>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var g_albedo_metal: texture_2d<f32>;
@group(0) @binding(2) var g_normal_rough: texture_2d<f32>;
@group(0) @binding(3) var g_emissive: texture_2d<f32>;
@group(0) @binding(4) var g_depth: texture_depth_2d;
@group(0) @binding(5) var shadow_map: texture_depth_2d;
@group(0) @binding(6) var shadow_sampler: sampler_comparison;
@group(0) @binding(7) var point_shadow_map: texture_depth_2d_array;
@group(0) @binding(8) var sun_tex: texture_2d<f32>;
@group(0) @binding(9) var moon_tex: texture_2d<f32>;
@group(0) @binding(10) var clouds_tex: texture_2d<f32>;
@group(0) @binding(11) var sky_sampler: sampler;
@group(0) @binding(12) var gi_tex: texture_2d<f32>;

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
const TAU: f32 = 6.28318530;

fn reconstruct_world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let wp = g.inv_view_proj * ndc;
    return wp.xyz / wp.w;
}

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn rotate_y(v: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec3<f32>(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
}

fn compute_ssao(pixel: vec2<i32>, dims: vec2<i32>, uv: vec2<f32>, world: vec3<f32>, n: vec3<f32>) -> f32 {
    if g.settings.w < 0.5 || g.gi_vol.x > 0.5 {
        return 1.0;
    }
    var poisson = array<vec2<f32>, 12>(
        vec2<f32>(-0.326, -0.406), vec2<f32>(-0.840, -0.074),
        vec2<f32>(-0.696,  0.457), vec2<f32>(-0.203,  0.621),
        vec2<f32>( 0.962, -0.195), vec2<f32>( 0.473, -0.480),
        vec2<f32>( 0.519,  0.767), vec2<f32>( 0.185, -0.893),
        vec2<f32>( 0.507,  0.064), vec2<f32>( 0.896,  0.412),
        vec2<f32>(-0.322, -0.933), vec2<f32>(-0.792, -0.598),
    );
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * TAU;
    let c = cos(angle);
    let s = sin(angle);
    let rot = mat2x2<f32>(vec2<f32>(c, s), vec2<f32>(-s, c));

    let view_dist = length(g.eye.xyz - world);
    let screen_radius = clamp(24.0 / max(view_dist, 1.0), 4.0, 22.0);
    let inv_dims = 1.0 / vec2<f32>(dims);

    let tap_count = select(12, 6, g.gi_vol.x > 0.5);
    let step_inc = select(1, 2, g.gi_vol.x > 0.5);
    var occ = 0.0;
    for (var i = 0; i < 12; i += step_inc) {
        let dir = rot * poisson[i];
        let step_scale = 0.25 + 0.75 * (f32(i) + 0.5) / 12.0;
        let offset = dir * (screen_radius * step_scale);
        let sp = clamp(pixel + vec2<i32>(offset), vec2<i32>(0), dims - 1);
        let sd = textureLoad(g_depth, sp, 0);
        if sd >= 0.999999 {
            continue;
        }
        let suv = (vec2<f32>(sp) + 0.5) * inv_dims;
        let sw = reconstruct_world(suv, sd);
        let diff = sw - world;
        let dist = length(diff);
        if dist < 0.015 || dist > 1.35 {
            continue;
        }
        let ndot = max(dot(n, diff / dist) - 0.08, 0.0);
        let range = 1.0 - smoothstep(0.35, 1.35, dist);
        occ += ndot * range;
    }
    return clamp(1.0 - (occ / f32(tap_count)) * 2.1, 0.22, 1.0);
}

fn shadow_visibility(pixel: vec2<i32>, world: vec3<f32>, n: vec3<f32>, light_dir: vec3<f32>) -> f32 {
    if g.settings.z < 0.5 {
        return 1.0;
    }
    let ndl = max(dot(n, light_dir), 0.0);
    let offset_world = world + n * (0.014 + 0.022 * (1.0 - ndl));
    let p = g.light_proj * vec4<f32>(offset_world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if any(uv < vec2<f32>(0.001)) || any(uv > vec2<f32>(0.999)) || ndc.z > 1.0 || ndc.z < 0.0 {
        return 1.0;
    }
    let bias = 0.0002 + (1.0 - ndl) * 0.0008;
    let texel = 1.0 / f32(textureDimensions(shadow_map).x);

    if g.settings.z < 1.5 {
        return textureSampleCompareLevel(shadow_map, shadow_sampler, uv, ndc.z - bias);
    }

    var poisson = array<vec2<f32>, 16>(
        vec2<f32>(-0.94201624, -0.39906216), vec2<f32>( 0.94558609, -0.76890725),
        vec2<f32>(-0.09418410, -0.92938870), vec2<f32>( 0.34495938,  0.29387760),
        vec2<f32>(-0.91588581,  0.45771432), vec2<f32>(-0.81544232, -0.87912464),
        vec2<f32>(-0.38277543,  0.27676845), vec2<f32>( 0.97484398,  0.75648379),
        vec2<f32>( 0.44323325, -0.97511554), vec2<f32>( 0.53742981, -0.47373420),
        vec2<f32>(-0.26496911, -0.41893023), vec2<f32>( 0.79197514,  0.19090188),
        vec2<f32>(-0.24188840,  0.99706507), vec2<f32>(-0.81409955,  0.91437590),
        vec2<f32>( 0.19984126,  0.78641367), vec2<f32>( 0.14383161, -0.14100790),
    );
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * TAU;
    let c = cos(angle);
    let s = sin(angle);
    let rot = mat2x2<f32>(vec2<f32>(c, s), vec2<f32>(-s, c));
    let spread = texel * 1.85;

    var sum = 0.0;
    for (var i = 0; i < 16; i++) {
        let off = (rot * poisson[i]) * spread;
        sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + off, ndc.z - bias);
    }
    return sum / 16.0;
}

fn point_shadow_visibility(
    pixel: vec2<i32>,
    light_idx: i32,
    light_pos: vec3<f32>,
    radius: f32,
    world: vec3<f32>,
    n: vec3<f32>,
) -> f32 {
    if g.settings.z < 0.5 || light_idx >= 8 {
        return 1.0;
    }
    let to_light = light_pos - world;
    let dist_dir = normalize(to_light);
    let ndl = max(dot(n, dist_dir), 0.0);
    let d = (world + n * (0.012 + 0.024 * (1.0 - ndl))) - light_pos;
    let ad = abs(d);

    var face: i32 = 0;
    var uv: vec2<f32>;
    var ma: f32;
    if ad.x >= ad.y && ad.x >= ad.z {
        ma = max(ad.x, 0.001);
        if d.x >= 0.0 {
            face = 0;
            uv = vec2<f32>(-d.z, d.y) / ma;
        } else {
            face = 1;
            uv = vec2<f32>(d.z, d.y) / ma;
        }
    } else if ad.y >= ad.x && ad.y >= ad.z {
        ma = max(ad.y, 0.001);
        if d.y >= 0.0 {
            face = 2;
            uv = vec2<f32>(d.x, -d.z) / ma;
        } else {
            face = 3;
            uv = vec2<f32>(d.x, d.z) / ma;
        }
    } else {
        ma = max(ad.z, 0.001);
        if d.z >= 0.0 {
            face = 4;
            uv = vec2<f32>(d.x, d.y) / ma;
        } else {
            face = 5;
            uv = vec2<f32>(-d.x, d.y) / ma;
        }
    }

    uv = uv * 0.5 + 0.5;
    let near = 0.08;
    let far = max(radius, 0.5);
    let z_ref = (far / (far - near)) * (1.0 - near / max(ma, near + 0.001));
    let bias = 0.0004 + (1.0 - ndl) * 0.0012;
    let layer = light_idx * 6 + face;

    if g.settings.z < 1.5 {
        return textureSampleCompareLevel(
            point_shadow_map,
            shadow_sampler,
            clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)),
            layer,
            z_ref - bias,
        );
    }

    var taps = array<vec2<f32>, 8>(
        vec2<f32>(-0.707, -0.707), vec2<f32>( 0.707, -0.707),
        vec2<f32>(-0.707,  0.707), vec2<f32>( 0.707,  0.707),
        vec2<f32>(-0.350,  0.000), vec2<f32>( 0.350,  0.000),
        vec2<f32>( 0.000, -0.350), vec2<f32>( 0.000,  0.350),
    );
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * TAU;
    let c = cos(angle);
    let s = sin(angle);
    let rot = mat2x2<f32>(vec2<f32>(c, s), vec2<f32>(-s, c));
    let texel = 1.0 / f32(textureDimensions(point_shadow_map).x);
    let spread = texel * 1.65;

    var sum = 0.0;
    for (var i = 0; i < 8; i++) {
        let sample_uv = clamp(uv + (rot * taps[i]) * spread, vec2<f32>(0.001), vec2<f32>(0.999));
        sum += textureSampleCompareLevel(point_shadow_map, shadow_sampler, sample_uv, layer, z_ref - bias);
    }
    return sum / 8.0;
}

fn fresnel(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cosine, 5.0);
}

fn brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, rough: f32, metal: f32) -> vec3<f32> {
    let h = normalize(l + v);
    let nl = max(dot(n, l), 0.001);
    let nv = max(dot(n, v), 0.001);
    let nh = max(dot(n, h), 0.0);
    let hv = max(dot(h, v), 0.0);
    let a = rough * rough;
    let a2 = a * a;
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(PI * denom * denom, 0.00001);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let geometry = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
    let f = fresnel(hv, mix(vec3<f32>(0.04), albedo, metal));
    return ((1.0 - f) * (1.0 - metal) * albedo / PI + distribution * geometry * f / (4.0 * nl * nv)) * nl;
}

fn orbit_arc_intensity(dir: vec3<f32>, azimuth_rad: f32, tilt_rad: f32) -> f32 {
    if dir.y < 0.005 {
        return 0.0;
    }
    let plane_r = rotate_y(vec3<f32>(1.0, 0.0, 0.0), azimuth_rad);
    let plane_u = rotate_y(vec3<f32>(0.0, sin(tilt_rad), cos(tilt_rad)), azimuth_rad);
    let plane_n = normalize(cross(plane_r, plane_u));

    let dist_to_plane = abs(dot(dir, plane_n));
    let line_mask = 1.0 - smoothstep(0.0009, 0.0023, dist_to_plane);
    if line_mask <= 0.001 {
        return 0.0;
    }
    let angle = atan2(dot(dir, plane_u), dot(dir, plane_r));
    let hours = fract(angle / TAU) * 24.0;
    // Fine dotted astronomical arc + hourly markers
    let dash = smoothstep(0.35, 0.65, sin(hours * TAU * 6.0));
    let tick = 1.0 - smoothstep(0.025, 0.07, abs(fract(hours) - 0.5));
    let horizon_fade = smoothstep(0.005, 0.07, dir.y);
    return line_mask * max(dash * 0.55, tick * 1.0) * horizon_fade;
}

fn sample_clouds_smooth(uv: vec2<f32>) -> vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(clouds_tex));
    let c = textureSampleLevel(clouds_tex, sky_sampler, uv, 0.0) * 0.40;
    let a = textureSampleLevel(clouds_tex, sky_sampler, uv + vec2<f32>(-0.75, -0.75) * texel, 0.0) * 0.15;
    let b = textureSampleLevel(clouds_tex, sky_sampler, uv + vec2<f32>( 0.75, -0.75) * texel, 0.0) * 0.15;
    let d = textureSampleLevel(clouds_tex, sky_sampler, uv + vec2<f32>(-0.75,  0.75) * texel, 0.0) * 0.15;
    let e = textureSampleLevel(clouds_tex, sky_sampler, uv + vec2<f32>( 0.75,  0.75) * texel, 0.0) * 0.15;
    return c + a + b + d + e;
}

fn sky_color(dir: vec3<f32>, sun: vec3<f32>, moon: vec3<f32>, draw_discs: bool) -> vec3<f32> {
    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let night = 1.0 - smoothstep(-0.18, 0.05, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));

    let zenith_day = vec3<f32>(0.15, 0.26, 0.44);
    let zenith_sunset = vec3<f32>(0.09, 0.11, 0.24);
    let zenith_night = vec3<f32>(0.009, 0.015, 0.036);
    var zenith = mix(zenith_night, zenith_day, day);
    zenith = mix(zenith, zenith_sunset, sunset * 0.72);

    let horizon_day = vec3<f32>(0.44, 0.53, 0.64);
    let horizon_sunset = vec3<f32>(0.94, 0.36, 0.14);
    let horizon_night = vec3<f32>(0.020, 0.032, 0.062);
    var horizon = mix(horizon_night, horizon_day, day);
    horizon = mix(horizon, horizon_sunset, sunset * 0.85);

    // Directional sunset band warming around sun azimuth.
    let horiz_dir = normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(0.0, 0.0, 0.0001));
    let horiz_sun = normalize(vec3<f32>(sun.x, 0.0, sun.z) + vec3<f32>(0.0, 0.0, 0.0001));
    let sun_az = pow(max(dot(horiz_dir, horiz_sun), 0.0), 3.0);
    horizon += vec3<f32>(0.85, 0.28, 0.08) * sunset * sun_az * 0.9;

    let up = clamp(dir.y, 0.0, 1.0);
    let ground = mix(vec3<f32>(0.012, 0.016, 0.028), vec3<f32>(0.11, 0.11, 0.12), day);
    var sky = select(
        mix(ground, horizon, clamp(dir.y * 4.0 + 1.0, 0.0, 1.0)),
        mix(horizon, zenith, pow(up, 0.52)),
        dir.y >= 0.0
    );

    if draw_discs {
        let horizon_clip = smoothstep(-0.03, 0.035, dir.y);

        // Textured Solar Disc + Corona Halo
        let sun_vis = smoothstep(-0.05, 0.04, elev) * horizon_clip;
        let sun_dot = max(dot(dir, sun), 0.0);
        let sun_tint = mix(vec3<f32>(1.0, 0.48, 0.16), vec3<f32>(1.0, 0.94, 0.80), smoothstep(0.0, 0.38, elev));
        sky += sun_tint * pow(sun_dot, 24.0) * (0.30 + sunset * 0.50) * sun_vis;
        sky += sun_tint * pow(sun_dot, 220.0) * 0.55 * sun_vis;

        if sun_vis > 0.001 && sun_dot > 0.99 {
            let sun_ref = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(sun.y) > 0.98);
            let sun_r = normalize(cross(sun_ref, sun));
            let sun_u = cross(sun, sun_r);
            let sun_ang = 0.036;
            let sx = dot(dir, sun_r) / sun_ang;
            let sy = dot(dir, sun_u) / sun_ang;
            let sr = length(vec2<f32>(sx, sy));
            if sr < 1.0 {
                let swirl = vec2<f32>(
                    sin(g.sky.x * 0.6 + sy * 8.0),
                    cos(g.sky.x * 0.5 + sx * 8.0)
                ) * 0.0035;
                let suv = clamp(vec2<f32>(sx, sy) * 0.5 + 0.5 + swirl, vec2<f32>(0.01), vec2<f32>(0.99));
                let photosphere = textureSampleLevel(sun_tex, sky_sampler, suv, 0.0).rgb;
                let limb = pow(max(1.0 - sr * sr, 0.0), 0.45);
                let limb_tint = mix(vec3<f32>(1.0, 0.32, 0.06), vec3<f32>(1.0, 0.92, 0.72), limb);
                let edge_mask = 1.0 - smoothstep(0.92, 1.0, sr);
                let solar_disc = photosphere * limb_tint * (0.95 + 1.35 * limb) * sun_tint;
                sky = mix(sky, solar_disc, edge_mask * sun_vis);
            }
        }

        // Moon disc geometry (compute before stars so moon occludes background stars)
        let moon_ref = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(moon.y) > 0.98);
        let moon_r = normalize(cross(moon_ref, moon));
        let moon_u = cross(moon, moon_r);
        let moon_ang = 0.032;
        let moon_dot = max(dot(dir, moon), 0.0);
        let mx = dot(dir, moon_r) / moon_ang;
        let my = dot(dir, moon_u) / moon_ang;
        let mr = length(vec2<f32>(mx, my));
        let in_moon = (moon_dot > 0.99) && (mr < 1.0) && (moon.y > -0.05);
        let moon_mask = select(0.0, 1.0 - smoothstep(0.95, 1.0, mr), in_moon) * horizon_clip;

        // Crisp pinpoint stars at night (occluded by Moon disc)
        let star_night = 1.0 - smoothstep(-0.26, -0.06, elev);
        if star_night > 0.01 && dir.y > 0.02 && moon_mask < 0.95 {
            let star_pos = dir * 210.0;
            let star_cell = floor(star_pos);
            let h = hash31(star_cell);
            if h > 0.985 {
                let d = length(fract(star_pos) - vec3<f32>(0.5));
                let pinpoint = smoothstep(0.26, 0.03, d);
                let twinkle = 0.70 + 0.30 * sin(g.sky.x * 2.2 + h * 120.0);
                let star_fade = smoothstep(0.02, 0.25, dir.y) * star_night * (1.0 - moon_mask);
                sky += vec3<f32>(0.84, 0.90, 1.0) * pinpoint * ((h - 0.985) / 0.015) * 1.45 * twinkle * star_fade;
            }
        }

        // Textured Lunar Disc with 3D crater relief, solar phase lighting & earthshine
        let moon_sky_vis = smoothstep(-0.04, 0.05, moon.y) * horizon_clip;
        let moon_halo_vis = moon_sky_vis * (1.0 - smoothstep(-0.08, 0.12, elev) * 0.75);
        sky += vec3<f32>(0.24, 0.36, 0.58) * pow(moon_dot, 84.0) * 0.38 * moon_halo_vis;

        if moon_mask > 0.001 {
            let muv = clamp(vec2<f32>(mx, my) * 0.5 + 0.5, vec2<f32>(0.005), vec2<f32>(0.995));
            let moon_sample = textureSampleLevel(moon_tex, sky_sampler, muv, 0.0);
            let texel = 1.0 / 512.0;
            let hx = textureSampleLevel(moon_tex, sky_sampler, muv + vec2<f32>(texel, 0.0), 0.0).a
                - textureSampleLevel(moon_tex, sky_sampler, muv - vec2<f32>(texel, 0.0), 0.0).a;
            let hy = textureSampleLevel(moon_tex, sky_sampler, muv + vec2<f32>(0.0, texel), 0.0).a
                - textureSampleLevel(moon_tex, sky_sampler, muv - vec2<f32>(0.0, texel), 0.0).a;

            let mz = sqrt(max(1.0 - mr * mr, 0.0));
            let n_world = normalize(moon_r * (mx - hx * 2.2) + moon_u * (my - hy * 2.2) - moon * mz);
            let ndl = max(dot(n_world, sun), 0.0);
            let ndv = max(mz, 0.08);
            // Lommel-Seeliger dusty regolith diffuse falloff
            let regolith = (ndl / (ndl + ndv * 0.42 + 0.01)) * 1.55;
            let earthshine = vec3<f32>(0.032, 0.048, 0.085) * (1.0 - smoothstep(-0.08, 0.15, elev));
            let moon_lit = moon_sample.rgb * (vec3<f32>(0.96, 0.97, 1.02) * (regolith * 1.42) + earthshine);

            // Blend naturally with daytime atmospheric scattering in front of the moon
            let day_fade = mix(1.0, 0.55, day);
            sky = mix(sky, sky * day + moon_lit * day_fade, moon_mask * moon_sky_vis);
        }
    }

    return sky;
}

fn shade_gbuffer(pixel: vec2<i32>, dims: vec2<i32>, uv: vec2<f32>, depth: f32, sun: vec3<f32>, moon: vec3<f32>, is_viewmodel: bool) -> vec4<f32> {
    let albedo_metal = textureLoad(g_albedo_metal, pixel, 0);
    let normal_rough = textureLoad(g_normal_rough, pixel, 0);
    let emissive = textureLoad(g_emissive, pixel, 0);

    let albedo = albedo_metal.rgb;
    let metal = albedo_metal.a;
    let n = normalize(normal_rough.xyz);
    let rough = normal_rough.a;
    let world = reconstruct_world(uv, depth);
    let view = normalize(g.eye.xyz - world);

    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let night = 1.0 - smoothstep(-0.18, 0.05, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));

    let ao = compute_ssao(pixel, dims, uv, world, n);
    let shadow_dir = select(moon, sun, elev >= -0.02);
    let visibility = shadow_visibility(pixel, world, n, shadow_dir);

    var light = vec3<f32>(0.0);

    // Fallback hemisphere ambient when GI is disabled (or soft viewmodel GI fill)
    let sky_hemi_day = vec3<f32>(0.24, 0.30, 0.40);
    let sky_hemi_sunset = vec3<f32>(0.30, 0.18, 0.16);
    let sky_hemi_night = vec3<f32>(0.022, 0.032, 0.058);
    let sky_hemi = mix(mix(sky_hemi_night, sky_hemi_day, day), sky_hemi_sunset, sunset * 0.65);
    let gnd_hemi = mix(vec3<f32>(0.010, 0.013, 0.022), vec3<f32>(0.11, 0.10, 0.095), day);
    let hemi = mix(gnd_hemi, sky_hemi, n.y * 0.5 + 0.5);

    if g.gi_vol.x < 0.5 {
        light += albedo * hemi * (1.0 - metal * 0.68) * ao;
    } else if is_viewmodel {
        let vm_gi = textureSampleLevel(gi_tex, sky_sampler, uv, 0.0).rgb;
        light += albedo * mix(hemi, vm_gi, 0.72) * (1.0 - metal * 0.68) * ao;
    }

    // Directional sun (fades cleanly at horizon) + cool moonlight along moon trajectory.
    let sun_strength = smoothstep(-0.02, 0.09, elev);
    let sun_radiance = mix(vec3<f32>(3.1, 1.05, 0.32), vec3<f32>(3.35, 2.95, 2.42), smoothstep(0.02, 0.35, elev));
    light += brdf(n, view, sun, albedo, rough, metal) * sun_radiance * (sun_strength * visibility);

    let moon_strength = (1.0 - sun_strength) * smoothstep(-0.03, 0.10, moon.y);
    if moon_strength > 0.001 {
        let moon_radiance = vec3<f32>(0.24, 0.34, 0.56) * moon_strength;
        light += brdf(n, view, moon, albedo, rough, metal) * moon_radiance * visibility;
    }

    // Dynamic deferred point lights with smooth inverse-square windowing & omnidirectional shadows.
    let count = min(i32(g.sun.w + 0.5), 16);
    for (var i = 0; i < count; i++) {
        let l = g.lights[i];
        let delta = l.pos_radius.xyz - world;
        let dist2 = dot(delta, delta);
        let radius = max(l.pos_radius.w, 0.1);
        if dist2 >= radius * radius {
            continue;
        }
        let dist = sqrt(max(dist2, 0.0001));
        let ratio = dist / radius;
        let win = clamp(1.0 - ratio * ratio * ratio * ratio, 0.0, 1.0);
        let atten = (win * win) / (0.35 + dist2);
        let p_vis = point_shadow_visibility(pixel, i, l.pos_radius.xyz, radius, world, n);
        light += brdf(n, view, delta / dist, albedo, rough, metal) * l.color_power.rgb * l.color_power.w * (atten * p_vis);
    }

    // Specular environment reflection with horizon & AO attenuation.
    let reflected = reflect(-view, n);
    let env = sky_color(reflected, sun, moon, false);
    let f = fresnel(max(dot(n, view), 0.0), mix(vec3<f32>(0.04), albedo, metal));
    let spec_ao = mix(1.0, ao, 0.72);
    light += env * f * (1.0 - rough * 0.72) * mix(0.32, 0.62, day) * spec_ao;

    // Emissive surface contribution.
    light += emissive.rgb;

    if is_viewmodel {
        let fog_day = vec3<f32>(0.36, 0.44, 0.54);
        let fog_sunset = vec3<f32>(0.42, 0.24, 0.18);
        let fog_night = vec3<f32>(0.015, 0.022, 0.042);
        let fog_color = mix(mix(fog_night, fog_day, day), fog_sunset, sunset * 0.7) * (1.0 - night * 0.15);
        let fog = 1.0 - exp(-length(g.eye.xyz - world) * 0.004 * g.settings.y);
        return vec4<f32>(mix(light, fog_color, fog), 1.0);
    }

    // Store AO in alpha so composite.wgsl can modulate GI cleanly before applying aerial fog.
    return vec4<f32>(light, ao);
}

@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(g_depth));
    let pixel = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), dims - 1);
    let depth = textureLoad(g_depth, pixel, 0);
    let sun = normalize(g.sun.xyz);
    let moon = normalize(g.moon.xyz);

    if depth >= 0.999999 {
        let far_world = reconstruct_world(v.uv, 1.0);
        let ray = normalize(far_world - g.eye.xyz);
        var sky = sky_color(ray, sun, moon, true);

        var cloud_t = 1.0;
        if g.atmos.x > 0.5 {
            let clouds = sample_clouds_smooth(v.uv);
            sky = sky * clouds.a + clouds.rgb;
            cloud_t = clouds.a;
        }

        if g.atmos.z > 0.5 {
            let sun_arc = orbit_arc_intensity(ray, g.sky.z, g.sky.w);
            let lunar_tilt = clamp(g.sky.w + radians(8.5) + sin(g.moon.w * TAU) * radians(5.0), radians(14.0), radians(88.0));
            let lunar_az = g.sky.z - radians(14.0);
            let moon_arc = orbit_arc_intensity(ray, lunar_az, lunar_tilt);
            let arc_vis = mix(0.35, 0.78, cloud_t);
            let arc_alpha = clamp((sun_arc + moon_arc) * 0.35 * arc_vis, 0.0, 0.5);
            sky = mix(sky, sky * 0.55, arc_alpha);
            sky += vec3<f32>(1.0, 0.72, 0.24) * (sun_arc * 0.52 * arc_vis);
            sky += vec3<f32>(0.42, 0.78, 1.0) * (moon_arc * 0.48 * arc_vis);
        }

        return vec4<f32>(sky, 1.0);
    }

    return shade_gbuffer(pixel, dims, v.uv, depth, sun, moon, false);
}

@fragment fn fs_viewmodel(v: Out) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(g_depth));
    let pixel = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), dims - 1);
    let depth = textureLoad(g_depth, pixel, 0);
    if depth >= 0.999999 {
        discard;
    }
    let sun = normalize(g.sun.xyz);
    let moon = normalize(g.moon.xyz);
    return shade_gbuffer(pixel, dims, v.uv, depth, sun, moon, true);
}
