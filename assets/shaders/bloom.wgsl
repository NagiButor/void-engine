@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;

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

fn prefilter_sample(c: vec3<f32>) -> vec3<f32> {
    let luma = max(c.r, max(c.g, c.b));
    let threshold = 1.05;
    let knee = 0.45;
    let soft = clamp(luma - threshold + knee, 0.0, 2.0 * knee);
    let contrib = max(soft * soft / (4.0 * knee + 0.0001), luma - threshold);
    let out = c * (max(contrib, 0.0) / max(luma, 0.0001));
    // Clamp extreme fireflies before downsampling so point sources bloom cleanly.
    return min(out, vec3<f32>(12.0));
}

fn karis_weight(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + max(c.r, max(c.g, c.b)));
}

@fragment fn fs_prefilter(v: Out) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let s0 = prefilter_sample(textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0, -1.0) * texel).rgb);
    let s1 = prefilter_sample(textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0, -1.0) * texel).rgb);
    let s2 = prefilter_sample(textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0,  1.0) * texel).rgb);
    let s3 = prefilter_sample(textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0,  1.0) * texel).rgb);
    let sc = prefilter_sample(textureSample(src_tex, linear_sampler, v.uv).rgb);

    let w0 = karis_weight(s0);
    let w1 = karis_weight(s1);
    let w2 = karis_weight(s2);
    let w3 = karis_weight(s3);
    let wc = karis_weight(sc) * 2.0;
    let color = (s0 * w0 + s1 * w1 + s2 * w2 + s3 * w3 + sc * wc) / (w0 + w1 + w2 + w3 + wc);
    return vec4<f32>(color, 1.0);
}

// 13-tap box downsample (Jimenez, Call of Duty: Advanced Warfare).
@fragment fn fs_downsample(v: Out) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let a = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-2.0, -2.0) * t).rgb;
    let b = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 0.0, -2.0) * t).rgb;
    let c = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 2.0, -2.0) * t).rgb;
    let d = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0, -1.0) * t).rgb;
    let e = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0, -1.0) * t).rgb;
    let f = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-2.0,  0.0) * t).rgb;
    let g = textureSample(src_tex, linear_sampler, v.uv).rgb;
    let h = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 2.0,  0.0) * t).rgb;
    let i = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0,  1.0) * t).rgb;
    let j = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0,  1.0) * t).rgb;
    let k = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-2.0,  2.0) * t).rgb;
    let l = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 0.0,  2.0) * t).rgb;
    let m = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 2.0,  2.0) * t).rgb;

    var out = (d + e + i + j) * 0.125;
    out += (a + b + f + g) * 0.03125;
    out += (b + c + g + h) * 0.03125;
    out += (f + g + k + l) * 0.03125;
    out += (g + h + l + m) * 0.03125;
    return vec4<f32>(out, 1.0);
}

// 9-tap tent filter for smooth mip upsampling.
@fragment fn fs_upsample(v: Out) -> @location(0) vec4<f32> {
    let t = (1.0 / vec2<f32>(textureDimensions(src_tex))) * 1.15;
    let a = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0, -1.0) * t).rgb;
    let b = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 0.0, -1.0) * t).rgb;
    let c = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0, -1.0) * t).rgb;
    let d = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0,  0.0) * t).rgb;
    let e = textureSample(src_tex, linear_sampler, v.uv).rgb;
    let f = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0,  0.0) * t).rgb;
    let g = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-1.0,  1.0) * t).rgb;
    let h = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 0.0,  1.0) * t).rgb;
    let i = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( 1.0,  1.0) * t).rgb;

    let out = (a + c + g + i) * 0.0625 + (b + d + f + h) * 0.125 + e * 0.25;
    return vec4<f32>(out * 0.72, 1.0);
}

// Fast 4-tap bilinear box downsample for direct HDR GI mip chain.
@fragment fn fs_box_downsample(v: Out) -> @location(0) vec4<f32> {
    let t = (1.0 / vec2<f32>(textureDimensions(src_tex))) * 0.5;
    let a = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-t.x, -t.y)).rgb;
    let b = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( t.x, -t.y)).rgb;
    let c = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>(-t.x,  t.y)).rgb;
    let d = textureSample(src_tex, linear_sampler, v.uv + vec2<f32>( t.x,  t.y)).rgb;
    return vec4<f32>((a + b + c + d) * 0.25, 1.0);
}
