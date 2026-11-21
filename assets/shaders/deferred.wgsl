struct PointLight {
    pos_radius: vec4<f32>,   // xyz = world pos, w = radius
    color_power: vec4<f32>,  // rgb = color, w = intensity
};

struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sun: vec4<f32>,          // xyz = sun dir, w = active point light count
    settings: vec4<f32>,     // x = exposure, y = fog, z = shadow quality, w = ao enabled
    sky: vec4<f32>,          // x = time
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

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn compute_ssao(pixel: vec2<i32>, dims: vec2<i32>, uv: vec2<f32>, world: vec3<f32>, n: vec3<f32>) -> f32 {
    if g.settings.w < 0.5 {
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
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * 6.2831853;
    let c = cos(angle);
    let s = sin(angle);
    let rot = mat2x2<f32>(vec2<f32>(c, s), vec2<f32>(-s, c));

    let view_dist = length(g.eye.xyz - world);
    let screen_radius = clamp(24.0 / max(view_dist, 1.0), 4.0, 22.0);
    let inv_dims = 1.0 / vec2<f32>(dims);

    var occ = 0.0;
    for (var i = 0; i < 12; i++) {
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
    return clamp(1.0 - (occ / 12.0) * 2.1, 0.22, 1.0);
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
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * 6.2831853;
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
    let angle = interleaved_gradient_noise(vec2<f32>(pixel)) * 6.2831853;
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

fn moon_direction(sun: vec3<f32>) -> vec3<f32> {
    return normalize(vec3<f32>(-sun.x, max(-sun.y, 0.18), -sun.z));
}

fn sky_color(dir: vec3<f32>, sun: vec3<f32>, draw_discs: bool) -> vec3<f32> {
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
        // Sun atmospheric halo & solar disc (fades cleanly below horizon).
        let sun_vis = smoothstep(-0.04, 0.05, elev);
        let sun_dot = max(dot(dir, sun), 0.0);
        let sun_tint = mix(vec3<f32>(1.0, 0.42, 0.12), vec3<f32>(1.0, 0.92, 0.78), smoothstep(0.0, 0.38, elev));
        sky += sun_tint * pow(sun_dot, 28.0) * (0.35 + sunset * 0.55) * sun_vis;
        sky += sun_tint * 2.4 * smoothstep(0.9992, 0.99975, sun_dot) * sun_vis;

        // Crisp pinpoint stars & moon disc at night.
        let star_night = 1.0 - smoothstep(-0.26, -0.06, elev);
        if star_night > 0.01 && dir.y > 0.02 {
            let star_pos = dir * 210.0;
            let star_cell = floor(star_pos);
            let h = hash31(star_cell);
            if h > 0.985 {
                let d = length(fract(star_pos) - vec3<f32>(0.5));
                let pinpoint = smoothstep(0.26, 0.03, d);
                let twinkle = 0.70 + 0.30 * sin(g.sky.x * 2.2 + h * 120.0);
                let star_fade = smoothstep(0.02, 0.25, dir.y) * star_night;
                sky += vec3<f32>(0.84, 0.90, 1.0) * pinpoint * ((h - 0.985) / 0.015) * 1.45 * twinkle * star_fade;
            }
        }
        let moon = moon_direction(sun);
        let moon_dot = max(dot(dir, moon), 0.0);
        let moon_vis = smoothstep(-0.02, -0.14, elev);
        sky += vec3<f32>(0.24, 0.36, 0.58) * pow(moon_dot, 96.0) * 0.35 * moon_vis;
        sky += vec3<f32>(0.90, 0.94, 1.0) * 1.35 * smoothstep(0.99945, 0.99982, moon_dot) * moon_vis;
    }

    return sky;
}

@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(g_depth));
    let pixel = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), dims - 1);
    let depth = textureLoad(g_depth, pixel, 0);
    let sun = normalize(g.sun.xyz);

    if depth >= 0.999999 {
        let far_world = reconstruct_world(v.uv, 1.0);
        let ray = normalize(far_world - g.eye.xyz);
        return vec4<f32>(sky_color(ray, sun, true), 1.0);
    }

    let albedo_metal = textureLoad(g_albedo_metal, pixel, 0);
    let normal_rough = textureLoad(g_normal_rough, pixel, 0);
    let emissive = textureLoad(g_emissive, pixel, 0);

    let albedo = albedo_metal.rgb;
    let metal = albedo_metal.a;
    let n = normalize(normal_rough.xyz);
    let rough = normal_rough.a;
    let world = reconstruct_world(v.uv, depth);
    let view = normalize(g.eye.xyz - world);

    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let night = 1.0 - smoothstep(-0.18, 0.05, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));

    let ao = compute_ssao(pixel, dims, v.uv, world, n);
    let shadow_dir = select(moon_direction(sun), sun, elev >= -0.02);
    let visibility = shadow_visibility(pixel, world, n, shadow_dir);

    // Dynamic ambient hemisphere (day -> sunset -> night).
    let sky_hemi_day = vec3<f32>(0.24, 0.30, 0.40);
    let sky_hemi_sunset = vec3<f32>(0.30, 0.18, 0.16);
    let sky_hemi_night = vec3<f32>(0.022, 0.032, 0.058);
    let sky_hemi = mix(mix(sky_hemi_night, sky_hemi_day, day), sky_hemi_sunset, sunset * 0.65);
    let gnd_hemi = mix(vec3<f32>(0.010, 0.013, 0.022), vec3<f32>(0.11, 0.10, 0.095), day);
    let hemi = mix(gnd_hemi, sky_hemi, n.y * 0.5 + 0.5);
    var light = albedo * hemi * (1.0 - metal * 0.68) * ao;

    // Directional sun (fades cleanly at horizon) + cool moonlight at night.
    let sun_strength = smoothstep(-0.02, 0.09, elev);
    let sun_radiance = mix(vec3<f32>(3.1, 1.05, 0.32), vec3<f32>(3.35, 2.95, 2.42), smoothstep(0.02, 0.35, elev));
    light += brdf(n, view, sun, albedo, rough, metal) * sun_radiance * (sun_strength * visibility);

    let moon_strength = smoothstep(-0.02, -0.14, elev);
    if moon_strength > 0.001 {
        let moon = moon_direction(sun);
        let moon_radiance = vec3<f32>(0.22, 0.31, 0.52) * moon_strength;
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
    let env = sky_color(reflected, sun, false);
    let f = fresnel(max(dot(n, view), 0.0), mix(vec3<f32>(0.04), albedo, metal));
    let spec_ao = mix(1.0, ao, 0.72);
    light += env * f * (1.0 - rough * 0.72) * mix(0.32, 0.62, day) * spec_ao;

    // Emissive surface contribution.
    light += emissive.rgb;

    // Subtle aerial perspective fog synced with time of day.
    let fog_day = vec3<f32>(0.36, 0.44, 0.54);
    let fog_sunset = vec3<f32>(0.42, 0.24, 0.18);
    let fog_night = vec3<f32>(0.015, 0.022, 0.042);
    let fog_color = mix(mix(fog_night, fog_day, day), fog_sunset, sunset * 0.7) * (1.0 - night * 0.15);
    let fog = 1.0 - exp(-length(g.eye.xyz - world) * 0.004 * g.settings.y);
    return vec4<f32>(mix(light, fog_color, fog), 1.0);
}
