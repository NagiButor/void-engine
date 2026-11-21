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
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var g_normal_rough: texture_2d<f32>;
@group(0) @binding(3) var g_depth: texture_depth_2d;
@group(0) @binding(4) var linear_sampler: sampler;

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

fn linearize_z(depth: f32) -> f32 {
    if depth >= 0.999999 {
        return g.proj_params.w;
    }
    let near = g.proj_params.z;
    let far = g.proj_params.w;
    return (far * near) / max(far - depth * (far - near), 0.0001);
}

@fragment fn fs_gi_blur(v: Out) -> @location(0) vec4<f32> {
    let src_dims = vec2<i32>(textureDimensions(src_tex));
    let full_dims = vec2<i32>(textureDimensions(g_depth));
    let center_sp = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), src_dims - 1);
    let center_fp = clamp(center_sp * 2, vec2<i32>(0), full_dims - 1);

    let center_d = textureLoad(g_depth, center_fp, 0);
    if center_d >= 0.999999 {
        return vec4<f32>(0.0);
    }

    let center_val = textureLoad(src_tex, center_sp, 0).rgb;
    let center_n = textureLoad(g_normal_rough, center_fp, 0).xyz;
    let center_z = linearize_z(center_d);

    var acc = center_val;
    var w_sum = 1.0;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if x == 0 && y == 0 {
                continue;
            }
            let sp = clamp(center_sp + vec2<i32>(x, y), vec2<i32>(0), src_dims - 1);
            let fp = clamp(sp * 2, vec2<i32>(0), full_dims - 1);
            let sd = textureLoad(g_depth, fp, 0);
            if sd >= 0.999999 {
                continue;
            }

            let sn = textureLoad(g_normal_rough, fp, 0).xyz;
            let sz = linearize_z(sd);

            let nd = max(dot(center_n, sn), 0.0);
            let nd2 = nd * nd;
            let nd4 = nd2 * nd2;
            let nd8 = nd4 * nd4;
            let n_w = nd8 * nd8;
            let z_diff = abs(center_z - sz) / max(center_z * 0.10, 0.10);
            let z_w = 1.0 / (1.0 + z_diff * z_diff);
            let spatial_w = select(0.70, 0.50, (x * x + y * y) > 1);
            let w = n_w * z_w * spatial_w;

            acc += textureLoad(src_tex, sp, 0).rgb * w;
            w_sum += w;
        }
    }

    return vec4<f32>(acc / max(w_sum, 0.0001), 1.0);
}

@fragment fn fs_vol_blur(v: Out) -> @location(0) vec4<f32> {
    let src_dims = vec2<i32>(textureDimensions(src_tex));
    let full_dims = vec2<i32>(textureDimensions(g_depth));
    let center_sp = clamp(vec2<i32>(v.clip.xy), vec2<i32>(0), src_dims - 1);
    let center_fp = clamp(vec2<i32>(v.uv * vec2<f32>(full_dims)), vec2<i32>(0), full_dims - 1);

    let center_d = textureLoad(g_depth, center_fp, 0);
    let center_z = linearize_z(center_d);
    let center_val = textureLoad(src_tex, center_sp, 0).rgb;

    var acc = center_val;
    var w_sum = 1.0;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if x == 0 && y == 0 {
                continue;
            }
            let sp = clamp(center_sp + vec2<i32>(x, y), vec2<i32>(0), src_dims - 1);
            let suv = (vec2<f32>(sp) + 0.5) / vec2<f32>(src_dims);
            let fp = clamp(vec2<i32>(suv * vec2<f32>(full_dims)), vec2<i32>(0), full_dims - 1);
            let sd = textureLoad(g_depth, fp, 0);
            let sz = linearize_z(sd);

            let z_diff = abs(center_z - sz) / max(center_z * 0.16, 0.30);
            let z_w = exp(-z_diff * z_diff);
            let spatial_w = exp(-f32(x * x + y * y) * 0.35);
            let w = z_w * spatial_w;

            acc += textureLoad(src_tex, sp, 0).rgb * w;
            w_sum += w;
        }
    }

    return vec4<f32>(acc / max(w_sum, 0.0001), 1.0);
}
