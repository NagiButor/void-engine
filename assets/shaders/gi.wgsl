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
@group(0) @binding(1) var g_normal_rough: texture_2d<f32>;
@group(0) @binding(2) var g_depth: texture_depth_2d;
@group(0) @binding(3) var direct_hdr: texture_2d<f32>;
@group(0) @binding(4) var gi_history: texture_2d<f32>;
@group(0) @binding(5) var sky_height_map: texture_depth_2d;
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
const HALF_PI: f32 = 1.57079633;

fn linear_z(ndc_z: f32) -> f32 {
    let near = g.proj_params.z;
    let far = g.proj_params.w;
    return (far * near) / max(far - ndc_z * (far - near), 0.0001);
}

fn view_pos_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let lz = linear_z(depth);
    let ndc_xy = vec2<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0);
    return vec3<f32>(
        ndc_xy.x * g.proj_params.x * lz,
        ndc_xy.y * g.proj_params.y * lz,
        -lz,
    );
}

fn reconstruct_world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let wp = g.inv_view_proj * ndc;
    return wp.xyz / wp.w;
}

fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn fast_acos(in_x: f32) -> f32 {
    let ax = abs(in_x);
    let r = (-0.156583 * ax + HALF_PI) * sqrt(max(1.0 - ax, 0.0));
    return select(PI - r, r, in_x >= 0.0);
}

fn fast_angle(y: f32, x: f32) -> f32 {
    let inv_len = inverseSqrt(max(x * x + y * y, 1e-8));
    let ang = fast_acos(clamp(x * inv_len, -1.0, 1.0));
    return select(-ang, ang, y >= 0.0);
}

fn sector_bitmask(a0: f32, a1: f32) -> u32 {
    let b0 = u32(clamp(floor(a0 * 32.0), 0.0, 31.0));
    let b1 = u32(clamp(ceil(a1 * 32.0), 0.0, 32.0));
    if b1 <= b0 {
        return 0u;
    }
    let len = b1 - b0;
    let mask = select((1u << min(len, 31u)) - 1u, 0xffffffffu, len >= 32u);
    return mask << b0;
}

fn compute_sky_open(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let probe_w = world + vec3<f32>(n.x * 0.25, 0.0, n.z * 0.25);
    let sp = g.sky_proj * vec4<f32>(probe_w, 1.0);
    let suv = sp.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if any(suv <= vec2<f32>(0.01)) || any(suv >= vec2<f32>(0.99)) {
        return 1.0;
    }
    let dims = vec2<i32>(textureDimensions(sky_height_map));
    let tp = clamp(vec2<i32>(suv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);
    let sd = textureLoad(sky_height_map, tp, 0);
    if sd >= 0.999999 {
        return 1.0;
    }
    let roof_y = 20.0 - (0.1 + sd * 24.9);
    return smoothstep(-0.22, -0.02, world.y - roof_y);
}

fn sky_irradiance(dir: vec3<f32>, sun: vec3<f32>, moon: vec3<f32>) -> vec3<f32> {
    let elev = sun.y;
    let day = smoothstep(-0.08, 0.28, elev);
    let sunset = exp(-pow((elev - 0.02) / 0.15, 2.0));
    let zenith = mix(
        mix(vec3<f32>(0.012, 0.020, 0.044), vec3<f32>(0.18, 0.30, 0.48), day),
        vec3<f32>(0.12, 0.13, 0.28),
        sunset * 0.72,
    );
    let horizon = mix(
        mix(vec3<f32>(0.022, 0.036, 0.068), vec3<f32>(0.44, 0.54, 0.66), day),
        vec3<f32>(0.88, 0.38, 0.16),
        sunset * 0.82,
    );
    let ground = mix(vec3<f32>(0.010, 0.013, 0.022), vec3<f32>(0.10, 0.095, 0.09), day);
    let up = clamp(dir.y, 0.0, 1.0);
    var col = select(
        mix(ground, horizon, clamp(dir.y * 4.0 + 1.0, 0.0, 1.0)),
        mix(horizon, zenith, pow(up, 0.55)),
        dir.y >= 0.0
    );
    if dir.y > 0.0 && g.atmos.x > 0.5 {
        let cov = clamp(g.atmos.y, 0.0, 1.0);
        let cloud_amb = mix(vec3<f32>(0.024, 0.036, 0.062), vec3<f32>(0.42, 0.48, 0.58), day);
        col = mix(col, cloud_amb, cov * 0.25 * smoothstep(0.0, 0.3, dir.y));
    }
    let moon_vis = smoothstep(-0.02, 0.15, moon.y) * (1.0 - day);
    col += vec3<f32>(0.035, 0.052, 0.095) * moon_vis * pow(max(dot(dir, moon), 0.0), 4.0);
    return col;
}

@fragment fn fs_gi(v: Out) -> @location(0) vec4<f32> {
    if g.gi_vol.x < 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 1000.0);
    }

    let full_dims = vec2<i32>(textureDimensions(g_depth));
    let fp = clamp(vec2<i32>(v.clip.xy) * 2, vec2<i32>(0), full_dims - 1);
    let depth = textureLoad(g_depth, fp, 0);
    if depth >= 0.999999 {
        return vec4<f32>(0.0, 0.0, 0.0, 1000.0);
    }

    let fuv = (vec2<f32>(fp) + 0.5) / vec2<f32>(full_dims);
    let P = view_pos_from_depth(fuv, depth);
    let view_dist = length(P);
    let V = normalize(-P);

    let n_world = textureLoad(g_normal_rough, fp, 0).xyz;
    let N = normalize((g.view * vec4<f32>(n_world, 0.0)).xyz);

    let world = reconstruct_world(fuv, depth);
    let sky_open = compute_sky_open(world, n_world);

    let noise_a = fract(ign(v.clip.xy) + g.prev_eye.w * 0.61803398875);
    let noise_b = fract(ign(v.clip.xy + vec2<f32>(17.0, 31.0)) + g.prev_eye.w * 0.38196601125);

    let aspect_inv = vec2<f32>(1.0 / max(g.atmos.w, 0.1), 1.0);
    let radius_uv = clamp(0.32 / max(-P.z * 0.14, 0.30), 0.05, 0.36);

    var ssgi_acc = vec3<f32>(0.0);
    var vis_acc = 0.0;

    let slices = 1;
    let steps = 5;

    for (var slice = 0; slice < slices; slice++) {
        let phi = (f32(slice) + noise_a) * PI;
        let omega = vec2<f32>(cos(phi), sin(phi));
        let uv_dir = vec2<f32>(omega.x, -omega.y) * aspect_inv;

        let slice_dir_v = vec3<f32>(omega.x, omega.y, 0.0);
        let ortho_raw = slice_dir_v - V * dot(slice_dir_v, V);
        let T_slice = normalize(ortho_raw + vec3<f32>(0.00001));
        let slice_n = normalize(cross(V, T_slice));
        let proj_n = N - slice_n * dot(N, slice_n);
        let proj_n_len = max(length(proj_n), 0.001);
        let n_dir = proj_n / proj_n_len;
        let n_angle = clamp(fast_angle(dot(n_dir, T_slice), dot(n_dir, V)), -HALF_PI, HALF_PI);

        var bitmask: u32 = 0u;

        for (var dir_i = 0; dir_i < 2; dir_i++) {
            let side = select(-1.0, 1.0, dir_i == 1);
            let jitter = fract(noise_b + f32(slice) * 0.37 + f32(dir_i) * 0.5);

            for (var s = 0; s < steps; s++) {
                let u = (f32(s) + 0.18 + 0.82 * jitter) * 0.20;
                let t = u * u * 0.8 + u * 0.2;
                let suv = fuv + uv_dir * (side * t * radius_uv);
                if any(suv <= vec2<f32>(0.002)) || any(suv >= vec2<f32>(0.998)) {
                    break;
                }

                let sp = clamp(vec2<i32>(suv * vec2<f32>(full_dims)), vec2<i32>(0), full_dims - 1);
                let sd = textureLoad(g_depth, sp, 0);
                if sd >= 0.999999 {
                    continue;
                }

                let S = view_pos_from_depth(suv, sd);
                let delta = S - P;
                let dist2 = dot(delta, delta);
                if dist2 < 0.0009 || dist2 > 42.25 {
                    continue;
                }
                let dist = sqrt(dist2);
                let front_vec = delta / dist;
                let thickness = 0.28 + 0.14 * dist;
                let back_raw = delta - V * thickness;

                let theta_f = fast_angle(dot(front_vec, T_slice), dot(front_vec, V));
                let theta_b = fast_angle(dot(back_raw, T_slice), dot(back_raw, V));

                let a0 = clamp((min(theta_f, theta_b) - n_angle) / PI + 0.5, 0.0, 1.0);
                let a1 = clamp((max(theta_f, theta_b) - n_angle) / PI + 0.5, 0.0, 1.0);
                let mask = sector_bitmask(a0, a1);
                let new_mask = mask & (~bitmask);
                let new_bits = countOneBits(new_mask);

                if new_bits > 0u {
                    let hit_n_world = textureLoad(g_normal_rough, sp, 0).xyz;
                    let hit_n_view = (g.view * vec4<f32>(hit_n_world, 0.0)).xyz;
                    let hit_facing = smoothstep(-0.15, 0.25, dot(hit_n_view, -front_vec));
                    if hit_facing > 0.001 {
                        let mip = clamp(t * 3.0, 0.0, 3.0);
                        let rad = min(textureSampleLevel(direct_hdr, linear_sampler, suv, mip).rgb, vec3<f32>(4.5));
                        let a_sum = a0 + a1;
                        let mid_cos = max(a_sum * (2.0 - a_sum), 0.06);
                        let dist_fade = 1.0 - smoothstep(2.5, 6.5, dist);
                        let w = (f32(new_bits) / 32.0) * proj_n_len * mid_cos * hit_facing * dist_fade;
                        ssgi_acc += rad * w;
                    }
                    bitmask |= mask;
                }
            }
        }

        let occ_frac = f32(countOneBits(bitmask)) / 32.0;
        vis_acc += (1.0 - occ_frac) * proj_n_len;
    }

    let ssgi = (ssgi_acc / f32(slices)) * 2.1;
    let ao_vis = clamp(vis_acc / f32(slices), 0.06, 1.0);
    let sky_col = sky_irradiance(n_world, normalize(g.sun.xyz), normalize(g.moon.xyz));
    var indirect = ssgi + sky_col * (ao_vis * mix(0.05, 1.0, sky_open));

    // Temporal accumulation with strict disocclusion check
    let prev_clip = g.prev_view_proj * vec4<f32>(world, 1.0);
    if prev_clip.w > 0.05 && g.prev_eye.w > 1.5 {
        let prev_ndc = prev_clip.xyz / prev_clip.w;
        let prev_uv = prev_ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
        if all(prev_uv > vec2<f32>(0.005)) && all(prev_uv < vec2<f32>(0.995)) {
            let hist = textureSampleLevel(gi_history, linear_sampler, prev_uv, 0.0);
            let expected_prev_dist = length(world - g.prev_eye.xyz);
            let rel_err = abs(hist.a - expected_prev_dist) / max(expected_prev_dist, 0.5);
            if rel_err < 0.035 {
                indirect = mix(hist.rgb, indirect, 0.16);
            }
        }
    }

    return vec4<f32>(indirect * g.gi_vol.y, view_dist);
}
