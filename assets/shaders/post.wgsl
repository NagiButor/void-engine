struct Post { settings: vec4<f32> };
@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var<uniform> post: Post;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    var p = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    var o: Out; o.clip = vec4<f32>(p[i],0.0,1.0); o.uv = p[i] * vec2<f32>(0.5,-0.5) + 0.5; return o;
}
fn linear_depth(d: f32) -> f32 { return 0.06 * 120.0 / (120.0 - d * (120.0 - 0.06)); }
fn aces(x: vec3<f32>) -> vec3<f32> { return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14),vec3<f32>(0.0),vec3<f32>(1.0)); }
@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(hdr));
    let pixel = clamp(vec2<i32>(v.uv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);
    let d = linear_depth(textureLoad(depth,pixel,0));
    var occ = 0.0;
    var bloom = vec3<f32>(0.0);
    for (var i=0; i<8; i++) {
        let angle = f32(i) * 2.39996;
        let radius = 3.0 + f32(i % 3) * 3.0;
        let offset = vec2<f32>(cos(angle),sin(angle)) * radius;
        let sample_pixel = clamp(pixel + vec2<i32>(offset),vec2<i32>(0),dims-1);
        let sd = linear_depth(textureLoad(depth,sample_pixel,0));
        let diff = d - sd;
        occ += select(0.0,1.0,diff > 0.025 && diff < 0.9) * (1.0 - clamp(diff/0.9,0.0,1.0));
        let b = textureSample(hdr,linear_sampler,v.uv + offset * 2.5 / vec2<f32>(dims)).rgb;
        bloom += max(b - vec3<f32>(1.2),vec3<f32>(0.0));
    }
    let texel = 1.0 / vec2<f32>(dims);
    let center = textureSample(hdr,linear_sampler,v.uv).rgb;
    let north = textureSample(hdr,linear_sampler,v.uv + vec2<f32>(0.0,-texel.y)).rgb;
    let south = textureSample(hdr,linear_sampler,v.uv + vec2<f32>(0.0,texel.y)).rgb;
    let west = textureSample(hdr,linear_sampler,v.uv + vec2<f32>(-texel.x,0.0)).rgb;
    let east = textureSample(hdr,linear_sampler,v.uv + vec2<f32>(texel.x,0.0)).rgb;
    let luma = vec3<f32>(0.299,0.587,0.114);
    let range = max(max(dot(north,luma),dot(south,luma)),max(dot(west,luma),dot(east,luma))) - min(min(dot(north,luma),dot(south,luma)),min(dot(west,luma),dot(east,luma)));
    let aa = smoothstep(0.09,0.40,range) * 0.45;
    var color = mix(center,(north+south+west+east)*0.25,aa) * (1.0 - occ * 0.045 * post.settings.y);
    color += bloom * (0.025 * post.settings.z);
    color = aces(color * post.settings.x);
    let vignette = 1.0 - smoothstep(0.25,0.8,length(v.uv-0.5)) * 0.13;
    return vec4<f32>(color * vignette,1.0);
}
