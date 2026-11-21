pub use crate::celestial::{moon_direction, sun_direction};
use crate::{
    celestial::{self, CELESTIAL_TEX_SIZE, CLOUD_NOISE_SIZE, WEATHER_TEX_SIZE},
    mesh::{MeshData, Vertex},
    textures,
};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub type MeshId = usize;
const MAX_INSTANCES: usize = 16384;
const SHADOW_SIZE: u32 = 4096;
const SKY_HEIGHT_SIZE: u32 = 256;
const DIRECT_MIPS: u32 = 4;
const POINT_SHADOW_SIZE: u32 = 512;
pub const MAX_POINT_LIGHTS: usize = 16;
const MAX_SHADOW_LIGHTS: usize = 8;
const POINT_UNIFORM_STRIDE: usize = 256;
const BLOOM_MIPS: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PointLight {
    pub pos_radius: [f32; 4],
    pub color_power: [f32; 4],
}

impl PointLight {
    pub fn new(pos: Vec3, radius: f32, color: [f32; 3], intensity: f32) -> Self {
        Self {
            pos_radius: [pos.x, pos.y, pos.z, radius],
            color_power: [color[0], color[1], color[2], intensity],
        }
    }
}

#[derive(Clone, Copy)]
pub struct Material {
    pub color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: f32,
    pub layer: u32,
    pub uv: [f32; 2],
}
impl Material {
    pub fn new(layer: u32, color: [f32; 3], roughness: f32, metallic: f32) -> Self {
        Self {
            color: [color[0], color[1], color[2], 1.0],
            roughness,
            metallic,
            emissive: 0.0,
            layer,
            uv: [1.0; 2],
        }
    }
    pub fn glow(color: [f32; 3], amount: f32) -> Self {
        Self {
            emissive: amount,
            ..Self::new(textures::WHITE, color, 0.5, 0.0)
        }
    }
}

#[derive(Clone)]
pub struct Draw {
    pub mesh: MeshId,
    pub transform: Mat4,
    pub material: Material,
    pub shadow: bool,
    pub viewmodel: bool,
}
impl Draw {
    pub fn new(mesh: MeshId, transform: Mat4, material: Material) -> Self {
        Self {
            mesh,
            transform,
            material,
            shadow: true,
            viewmodel: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    surface: [f32; 4],
    uv: [f32; 4],
}
impl From<&Draw> for Instance {
    fn from(d: &Draw) -> Self {
        let m = d.material;
        Self {
            model: d.transform.to_cols_array_2d(),
            color: m.color,
            surface: [m.roughness, m.metallic, m.emissive, m.layer as f32],
            uv: [m.uv[0], m.uv[1], 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    vp: [[f32; 4]; 4],
    light: [[f32; 4]; 4],
    inverse: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    prev_vp: [[f32; 4]; 4],
    sky_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    prev_eye: [f32; 4],
    proj_params: [f32; 4],
    sun: [f32; 4],
    moon: [f32; 4],
    settings: [f32; 4],
    sky: [f32; 4],
    atmos: [f32; 4],
    gi_vol: [f32; 4],
    lights: [PointLight; MAX_POINT_LIGHTS],
}

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// `Hor+` (`16:9` reference) horizontal field of view in degrees.
    pub fov: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(
                0.0,
                crate::units::human::EYE_HEIGHT + crate::units::human::SPAWN_CLEARANCE,
                10.5,
            ),
            yaw: 0.0,
            pitch: -0.07,
            roll: 0.0,
            fov: crate::units::camera::DEFAULT_HFOV_DEG,
        }
    }
}
impl Camera {
    pub fn forward(&self) -> Vec3 {
        crate::units::coord::forward_from_yaw_pitch(self.yaw, self.pitch)
    }
    pub fn right(&self) -> Vec3 {
        crate::units::coord::right_from_yaw(self.yaw)
    }
    pub fn world(&self) -> Mat4 {
        self.view().inverse()
    }
    pub fn view(&self) -> Mat4 {
        let forward = self.forward();
        let up = if self.roll.abs() > 1e-6 {
            glam::Quat::from_axis_angle(forward, self.roll) * Vec3::Y
        } else {
            Vec3::Y
        };
        Mat4::look_at_rh(self.position, self.position + forward, up)
    }
    pub fn vertical_fov_rad(&self) -> f32 {
        crate::units::camera::vfov_rad_from_hfov_16_9(self.fov)
    }
    pub fn projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(
            self.vertical_fov_rad(),
            aspect.max(0.1),
            crate::units::camera::NEAR_CLIP_M,
            crate::units::camera::FAR_CLIP_M,
        )
    }
}

pub struct RenderSettings {
    pub exposure: f32,
    pub fog: f32,
    pub shadows: u32,
    pub ao: bool,
    pub bloom: bool,
    pub gi: bool,
    pub gi_intensity: f32,
    pub clouds: bool,
    pub cloud_coverage: f32,
    pub volumetric: bool,
    pub volumetric_intensity: f32,
    pub time_of_day: f32,
    pub sun_azimuth: f32,
    pub sun_tilt: f32,
    pub moon_phase: f32,
    pub show_trajectory: bool,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            exposure: 1.05,
            fog: 0.6,
            shadows: 2,
            ao: true,
            bloom: true,
            gi: true,
            gi_intensity: 1.0,
            clouds: true,
            cloud_coverage: 0.52,
            volumetric: true,
            volumetric_intensity: 1.0,
            time_of_day: 13.5,
            sun_azimuth: -28.0,
            sun_tilt: 62.0,
            moon_phase: 0.42,
            show_trajectory: true,
        }
    }
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

struct Targets {
    _albedo_metal: wgpu::Texture,
    albedo_metal: wgpu::TextureView,
    _normal_rough: wgpu::Texture,
    normal_rough: wgpu::TextureView,
    _emissive: wgpu::Texture,
    emissive: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth: wgpu::TextureView,
    _direct_hdr: wgpu::Texture,
    direct_hdr_mips: Vec<wgpu::TextureView>,
    direct_down_bgs: Vec<wgpu::BindGroup>,
    _clouds_a: wgpu::Texture,
    clouds_a: wgpu::TextureView,
    _clouds_b: wgpu::Texture,
    clouds_b: wgpu::TextureView,
    _gi_a: wgpu::Texture,
    gi_a: wgpu::TextureView,
    _gi_b: wgpu::Texture,
    gi_b: wgpu::TextureView,
    _gi_filtered: wgpu::Texture,
    gi_filtered: wgpu::TextureView,
    _vol_raw: wgpu::Texture,
    vol_raw: wgpu::TextureView,
    _vol_filtered: wgpu::Texture,
    vol_filtered: wgpu::TextureView,
    _hdr: wgpu::Texture,
    hdr: wgpu::TextureView,
    _bloom: wgpu::Texture,
    bloom_mips: Vec<wgpu::TextureView>,
    bloom_down_bgs: Vec<wgpu::BindGroup>,
    bloom_up_bgs: Vec<wgpu::BindGroup>,
    clouds_bgs: [wgpu::BindGroup; 2],
    deferred: [wgpu::BindGroup; 2],
    gi_bgs: [wgpu::BindGroup; 2],
    gi_blur_bgs: [wgpu::BindGroup; 2],
    vol_bgs: [wgpu::BindGroup; 2],
    vol_blur_bg: wgpu::BindGroup,
    composite_bg: wgpu::BindGroup,
    post: wgpu::BindGroup,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub adapter_name: String,
    meshes: Vec<GpuMesh>,
    instances: wgpu::Buffer,
    globals: wgpu::Buffer,
    gbuffer_bindings: wgpu::BindGroup,
    shadow_bindings: wgpu::BindGroup,
    point_shadow_uniforms: wgpu::Buffer,
    point_shadow_bindings: wgpu::BindGroup,
    gbuffer_pipeline: wgpu::RenderPipeline,
    sky_height_pipeline: wgpu::RenderPipeline,
    clouds_pipeline: wgpu::RenderPipeline,
    deferred_pipeline: wgpu::RenderPipeline,
    deferred_viewmodel_pipeline: wgpu::RenderPipeline,
    gi_pipeline: wgpu::RenderPipeline,
    gi_blur_pipeline: wgpu::RenderPipeline,
    volumetric_pipeline: wgpu::RenderPipeline,
    vol_blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    point_shadow_pipeline: wgpu::RenderPipeline,
    bloom_prefilter_pipeline: wgpu::RenderPipeline,
    bloom_downsample_pipeline: wgpu::RenderPipeline,
    direct_down_pipeline: wgpu::RenderPipeline,
    bloom_upsample_pipeline: wgpu::RenderPipeline,
    post_pipeline: wgpu::RenderPipeline,
    _shadow: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    _sky_height: wgpu::Texture,
    sky_height_view: wgpu::TextureView,
    _point_shadow: wgpu::Texture,
    point_shadow_array_view: wgpu::TextureView,
    point_shadow_face_views: Vec<wgpu::TextureView>,
    shadow_sampler: wgpu::Sampler,
    _sun_tex: wgpu::Texture,
    sun_view: wgpu::TextureView,
    _moon_tex: wgpu::Texture,
    moon_view: wgpu::TextureView,
    _cloud_noise_3d: wgpu::Texture,
    cloud_noise_view: wgpu::TextureView,
    _weather_tex: wgpu::Texture,
    weather_view: wgpu::TextureView,
    cloud_sampler: wgpu::Sampler,
    sky_sampler: wgpu::Sampler,
    clouds_layout: wgpu::BindGroupLayout,
    deferred_layout: wgpu::BindGroupLayout,
    gi_layout: wgpu::BindGroupLayout,
    bilateral_layout: wgpu::BindGroupLayout,
    volumetric_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    bloom_layout: wgpu::BindGroupLayout,
    targets: Targets,
    post_layout: wgpu::BindGroupLayout,
    post_uniform: wgpu::Buffer,
    post_sampler: wgpu::Sampler,
    prev_vp: Mat4,
    prev_eye: Vec3,
    frame_index: u32,
}

fn target(
    device: &wgpu::Device,
    label: &str,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    samples: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

struct TargetContext<'a> {
    globals: &'a wgpu::Buffer,
    shadow_view: &'a wgpu::TextureView,
    point_shadow_array_view: &'a wgpu::TextureView,
    shadow_sampler: &'a wgpu::Sampler,
    sky_height_view: &'a wgpu::TextureView,
    sun_view: &'a wgpu::TextureView,
    moon_view: &'a wgpu::TextureView,
    cloud_noise_view: &'a wgpu::TextureView,
    weather_view: &'a wgpu::TextureView,
    cloud_sampler: &'a wgpu::Sampler,
    sky_sampler: &'a wgpu::Sampler,
    clouds_layout: &'a wgpu::BindGroupLayout,
    deferred_layout: &'a wgpu::BindGroupLayout,
    gi_layout: &'a wgpu::BindGroupLayout,
    bilateral_layout: &'a wgpu::BindGroupLayout,
    volumetric_layout: &'a wgpu::BindGroupLayout,
    composite_layout: &'a wgpu::BindGroupLayout,
    bloom_layout: &'a wgpu::BindGroupLayout,
    post_layout: &'a wgpu::BindGroupLayout,
    post_uniform: &'a wgpu::Buffer,
    post_sampler: &'a wgpu::Sampler,
}

fn create_targets(device: &wgpu::Device, w: u32, h: u32, ctx: &TargetContext<'_>) -> Targets {
    let (albedo_metal_tex, albedo_metal) = target(
        device,
        "GBuffer albedo + metallic",
        w,
        h,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
    );
    let (normal_rough_tex, normal_rough) = target(
        device,
        "GBuffer normal + roughness",
        w,
        h,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (emissive_tex, emissive) = target(
        device,
        "GBuffer emissive",
        w,
        h,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (depth_tex, depth) = target(
        device,
        "World depth",
        w,
        h,
        wgpu::TextureFormat::Depth32Float,
        1,
    );
    let direct_mip_count = w.max(h).ilog2().saturating_add(1).clamp(1, DIRECT_MIPS);
    let direct_hdr_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Direct HDR + Sky"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: direct_mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let direct_hdr = direct_hdr_tex.create_view(&Default::default());
    let direct_hdr_mips: Vec<wgpu::TextureView> = (0..direct_mip_count)
        .map(|mip| {
            direct_hdr_tex.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let (hdr_tex, hdr) = target(
        device,
        "HDR scene",
        w,
        h,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let bw = (w / 2).max(1);
    let bh = (h / 2).max(1);
    let qw = (w / 4).max(1);
    let qh = (h / 4).max(1);
    let (clouds_a_tex, clouds_a) = target(
        device,
        "Quarter-res volumetric clouds ping",
        qw,
        qh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (clouds_b_tex, clouds_b) = target(
        device,
        "Quarter-res volumetric clouds pong",
        qw,
        qh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (gi_a_tex, gi_a) = target(
        device,
        "Half-res GI ping",
        bw,
        bh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (gi_b_tex, gi_b) = target(
        device,
        "Half-res GI pong",
        bw,
        bh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (gi_filtered_tex, gi_filtered) = target(
        device,
        "Half-res GI filtered",
        bw,
        bh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (vol_raw_tex, vol_raw) = target(
        device,
        "Quarter-res volumetric light",
        qw,
        qh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (vol_filtered_tex, vol_filtered) = target(
        device,
        "Quarter-res volumetric filtered",
        qw,
        qh,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let mip_count = bw.max(bh).ilog2().saturating_add(1).clamp(1, BLOOM_MIPS);
    let bloom_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Bloom mip chain"),
        size: wgpu::Extent3d {
            width: bw,
            height: bh,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let bloom_mips: Vec<wgpu::TextureView> = (0..mip_count)
        .map(|mip| {
            bloom_tex.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let bloom_bg = |view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom pass bindings"),
            layout: ctx.bloom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
                },
            ],
        })
    };
    let mut bloom_down_bgs = Vec::with_capacity(mip_count as usize);
    bloom_down_bgs.push(bloom_bg(&hdr));
    for view in bloom_mips.iter().take(mip_count as usize - 1) {
        bloom_down_bgs.push(bloom_bg(view));
    }
    let bloom_up_bgs: Vec<wgpu::BindGroup> = bloom_mips.iter().skip(1).map(&bloom_bg).collect();
    let direct_down_bgs: Vec<wgpu::BindGroup> = direct_hdr_mips
        .iter()
        .take(direct_mip_count as usize - 1)
        .map(&bloom_bg)
        .collect();
    let make_clouds_bg = |history: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Clouds bindings"),
            layout: ctx.clouds_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ctx.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ctx.cloud_noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ctx.weather_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(history),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(ctx.cloud_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(ctx.sky_sampler),
                },
            ],
        })
    };
    let clouds_bgs = [make_clouds_bg(&clouds_b), make_clouds_bg(&clouds_a)];

    let make_deferred_bg = |clouds_cur: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Deferred lighting bindings"),
            layout: ctx.deferred_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ctx.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&albedo_metal),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_rough),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&emissive),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(ctx.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(ctx.shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(ctx.point_shadow_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(ctx.sun_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(ctx.moon_view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(clouds_cur),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::Sampler(ctx.sky_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(&gi_filtered),
                },
            ],
        })
    };
    let deferred = [make_deferred_bg(&clouds_a), make_deferred_bg(&clouds_b)];

    let make_gi_bg = |history: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GI bindings"),
            layout: ctx.gi_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ctx.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal_rough),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&direct_hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(history),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(ctx.sky_height_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
                },
            ],
        })
    };
    let make_bilateral_bg = |src: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bilateral blur bindings"),
            layout: ctx.bilateral_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ctx.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_rough),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
                },
            ],
        })
    };
    let gi_bgs = [make_gi_bg(&gi_b), make_gi_bg(&gi_a)];
    let gi_blur_bgs = [make_bilateral_bg(&gi_a), make_bilateral_bg(&gi_b)];
    let make_vol_bg = |clouds_cur: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric light bindings"),
            layout: ctx.volumetric_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ctx.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ctx.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(ctx.shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(clouds_cur),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(ctx.cloud_noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(ctx.cloud_sampler),
                },
            ],
        })
    };
    let vol_bgs = [make_vol_bg(&clouds_a), make_vol_bg(&clouds_b)];
    let vol_blur_bg = make_bilateral_bg(&vol_raw);
    let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Composite bindings"),
        layout: ctx.composite_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: ctx.globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&direct_hdr),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&gi_filtered),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&vol_filtered),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&albedo_metal),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&normal_rough),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&depth),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
            },
        ],
    });
    let post = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Post bindings"),
        layout: ctx.post_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&hdr),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(ctx.post_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: ctx.post_uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&bloom_mips[0]),
            },
        ],
    });
    Targets {
        _albedo_metal: albedo_metal_tex,
        albedo_metal,
        _normal_rough: normal_rough_tex,
        normal_rough,
        _emissive: emissive_tex,
        emissive,
        _depth: depth_tex,
        depth,
        _direct_hdr: direct_hdr_tex,
        direct_hdr_mips,
        direct_down_bgs,
        _clouds_a: clouds_a_tex,
        clouds_a,
        _clouds_b: clouds_b_tex,
        clouds_b,
        _gi_a: gi_a_tex,
        gi_a,
        _gi_b: gi_b_tex,
        gi_b,
        _gi_filtered: gi_filtered_tex,
        gi_filtered,
        _vol_raw: vol_raw_tex,
        vol_raw,
        _vol_filtered: vol_filtered_tex,
        vol_filtered,
        _hdr: hdr_tex,
        hdr,
        _bloom: bloom_tex,
        bloom_mips,
        bloom_down_bgs,
        bloom_up_bgs,
        clouds_bgs,
        deferred,
        gi_bgs,
        gi_blur_bgs,
        vol_bgs,
        vol_blur_bg,
        composite_bg,
        post,
    }
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        assets: &std::path::Path,
        backend: &str,
        adapter_index: Option<usize>,
        vsync: bool,
        validation: bool,
    ) -> Result<Self> {
        let backends = match backend {
            "vulkan" => wgpu::Backends::VULKAN,
            "dx12" => wgpu::Backends::DX12,
            _ => wgpu::Backends::PRIMARY,
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: if validation {
                wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG
            } else {
                wgpu::InstanceFlags::default()
            },
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = if let Some(index) = adapter_index {
            instance
                .enumerate_adapters(backends)
                .into_iter()
                .filter(|a| a.is_surface_supported(&surface))
                .nth(index)
                .context("Adapter index out of range")?
        } else {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .context("No compatible graphics adapter")?
        };
        let info = adapter.get_info();
        let adapter_name = format!("{} / {:?}", info.name, info.backend);
        println!("V.O.I.D. Rust — {adapter_name}");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("V.O.I.D."),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .context("No sRGB surface")?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: if vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("World globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let texture_entry = |binding, dimension, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let uniform_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sampler_entry = |binding, comparison| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(if comparison {
                wgpu::SamplerBindingType::Comparison
            } else {
                wgpu::SamplerBindingType::Filtering
            }),
            count: None,
        };

        let gbuffer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GBuffer layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2Array,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2Array,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(3, false),
            ],
        });
        let atlas = textures::load(assets)?;
        let color = upload_texture_mips(
            &device,
            &queue,
            "Albedo + roughness",
            textures::SIZE,
            textures::SIZE,
            textures::LAYERS,
            &atlas.albedo,
            true,
        );
        let normal = upload_texture_mips(
            &device,
            &queue,
            "Tangent normals",
            textures::SIZE,
            textures::SIZE,
            textures::LAYERS,
            &atlas.normal,
            false,
        );
        let color_view = color.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let normal_view = normal.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Anisotropic material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let gbuffer_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer bindings"),
            layout: &gbuffer_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (shadow, shadow_view) = target(
            &device,
            "4096 Poisson PCF sun shadow",
            SHADOW_SIZE,
            SHADOW_SIZE,
            wgpu::TextureFormat::Depth32Float,
            1,
        );
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow layout"),
            entries: &[uniform_entry(0)],
        });
        let shadow_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow globals"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });

        let point_shadow_layers = (MAX_SHADOW_LIGHTS * 6) as u32;
        let point_shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Point light shadow array"),
            size: wgpu::Extent3d {
                width: POINT_SHADOW_SIZE,
                height: POINT_SHADOW_SIZE,
                depth_or_array_layers: point_shadow_layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let point_shadow_array_view = point_shadow.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let point_shadow_face_views: Vec<wgpu::TextureView> = (0..point_shadow_layers)
            .map(|layer| {
                point_shadow.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let point_shadow_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Point shadow matrices"),
            size: (MAX_SHADOW_LIGHTS * 6 * POINT_UNIFORM_STRIDE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let point_shadow_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Point shadow layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(POINT_UNIFORM_STRIDE as u64),
                    },
                    count: None,
                }],
            });
        let point_shadow_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Point shadow bindings"),
            layout: &point_shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &point_shadow_uniforms,
                    offset: 0,
                    size: std::num::NonZeroU64::new(POINT_UNIFORM_STRIDE as u64),
                }),
            }],
        });

        let gbuffer_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GBuffer geometry"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/gbuffer.wgsl").into(),
            ),
        });
        let gbuffer_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GBuffer"),
            bind_group_layouts: &[&gbuffer_layout],
            push_constant_ranges: &[],
        });
        let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow"),
            bind_group_layouts: &[&shadow_layout],
            push_constant_ranges: &[],
        });
        let point_shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Point shadow"),
            bind_group_layouts: &[&point_shadow_layout],
            push_constant_ranges: &[],
        });
        let buffers = vertex_layouts();
        let depth_state = |bias| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias,
        };
        let gbuffer_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GBuffer MRT"),
            layout: Some(&gbuffer_pl),
            vertex: wgpu::VertexState {
                module: &gbuffer_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &gbuffer_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(depth_state(Default::default())),
            multisample: Default::default(),
            multiview: None,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sun shadows"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &gbuffer_shader,
                entry_point: "vs_shadow",
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(depth_state(wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            })),
            multisample: Default::default(),
            multiview: None,
        });
        let point_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Point shadow"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/point_shadow.wgsl").into(),
            ),
        });
        let point_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Point shadows"),
                layout: Some(&point_shadow_pl),
                vertex: wgpu::VertexState {
                    module: &point_shadow_shader,
                    entry_point: "vs_point_shadow",
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                fragment: None,
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(depth_state(wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                })),
                multisample: Default::default(),
                multiview: None,
            });

        let (sky_height, sky_height_view) = target(
            &device,
            "Top-down sky height",
            SKY_HEIGHT_SIZE,
            SKY_HEIGHT_SIZE,
            wgpu::TextureFormat::Depth32Float,
            1,
        );

        let sun_data = celestial::generate_sun_texture(CELESTIAL_TEX_SIZE);
        let moon_data = celestial::generate_moon_texture(CELESTIAL_TEX_SIZE);
        let sun_tex = upload_texture_mips(
            &device,
            &queue,
            "Procedural Sun",
            CELESTIAL_TEX_SIZE,
            CELESTIAL_TEX_SIZE,
            1,
            &sun_data,
            true,
        );
        let moon_tex = upload_texture_mips(
            &device,
            &queue,
            "Procedural Moon",
            CELESTIAL_TEX_SIZE,
            CELESTIAL_TEX_SIZE,
            1,
            &moon_data,
            false,
        );
        let sun_view = sun_tex.create_view(&Default::default());
        let moon_view = moon_tex.create_view(&Default::default());
        let sky_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Celestial & cloud sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let cloud_noise_data = celestial::generate_cloud_noise_3d(CLOUD_NOISE_SIZE);
        let cloud_noise_3d = upload_texture_3d(
            &device,
            &queue,
            "3D cloud Perlin-Worley noise",
            CLOUD_NOISE_SIZE,
            &cloud_noise_data,
        );
        let cloud_noise_view = cloud_noise_3d.create_view(&Default::default());

        let weather_data = celestial::generate_weather_texture(WEATHER_TEX_SIZE);
        let weather_tex = upload_texture_mips(
            &device,
            &queue,
            "Cloud weather LUT",
            WEATHER_TEX_SIZE,
            WEATHER_TEX_SIZE,
            1,
            &weather_data,
            false,
        );
        let weather_view = weather_tex.create_view(&Default::default());
        let cloud_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Repeating 3D/2D cloud noise sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let clouds_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Clouds layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D3,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(5, false),
                sampler_entry(6, false),
            ],
        });

        let sky_height_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky height"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &gbuffer_shader,
                entry_point: "vs_sky_height",
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(depth_state(Default::default())),
            multisample: Default::default(),
            multiview: None,
        });

        let deferred_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Deferred lighting layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                texture_entry(
                    5,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(6, true),
                texture_entry(
                    7,
                    wgpu::TextureViewDimension::D2Array,
                    wgpu::TextureSampleType::Depth,
                ),
                texture_entry(
                    8,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    9,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    10,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(11, false),
                texture_entry(
                    12,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
            ],
        });
        let gi_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GI layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    5,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(6, false),
            ],
        });
        let bilateral_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bilateral filter layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(4, false),
            ],
        });
        let volumetric_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Volumetric light layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(3, true),
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(5, false),
                texture_entry(
                    6,
                    wgpu::TextureViewDimension::D3,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(7, false),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite layout"),
            entries: &[
                uniform_entry(0),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    5,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                texture_entry(
                    6,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(7, false),
            ],
        });

        let deferred_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Deferred PBR + SSAO + Sky"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/deferred.wgsl").into(),
            ),
        });
        let clouds_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Procedural clouds"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/clouds.wgsl").into(),
            ),
        });
        let gi_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Global illumination"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/gi.wgsl").into(),
            ),
        });
        let bilateral_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Joint bilateral denoiser"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/bilateral.wgsl").into(),
            ),
        });
        let volumetric_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric light"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/volumetric.wgsl").into(),
            ),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HDR composite"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/composite.wgsl").into(),
            ),
        });

        let make_fullscreen = |label, layout: &wgpu::BindGroupLayout, module, entry| {
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: "vs_main",
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: entry,
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
            })
        };

        let clouds_pipeline = make_fullscreen(
            "Volumetric clouds",
            &clouds_layout,
            &clouds_shader,
            "fs_main",
        );
        let deferred_pipeline = make_fullscreen(
            "Deferred lighting",
            &deferred_layout,
            &deferred_shader,
            "fs_main",
        );
        let deferred_viewmodel_pipeline = make_fullscreen(
            "Deferred viewmodel lighting",
            &deferred_layout,
            &deferred_shader,
            "fs_viewmodel",
        );
        let gi_pipeline = make_fullscreen("Global illumination", &gi_layout, &gi_shader, "fs_gi");
        let gi_blur_pipeline = make_fullscreen(
            "GI bilateral blur",
            &bilateral_layout,
            &bilateral_shader,
            "fs_gi_blur",
        );
        let volumetric_pipeline = make_fullscreen(
            "Volumetric shafts",
            &volumetric_layout,
            &volumetric_shader,
            "fs_main",
        );
        let vol_blur_pipeline = make_fullscreen(
            "Volumetric bilateral blur",
            &bilateral_layout,
            &bilateral_shader,
            "fs_vol_blur",
        );
        let composite_pipeline = make_fullscreen(
            "HDR composite",
            &composite_layout,
            &composite_shader,
            "fs_main",
        );

        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom layout"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(1, false),
            ],
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Post layout"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(1, false),
                uniform_entry(2),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
            ],
        });
        let post_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Post settings"),
            contents: bytemuck::cast_slice(&[1.0f32, 1.0, 1.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let post_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let target_ctx = TargetContext {
            globals: &globals,
            shadow_view: &shadow_view,
            point_shadow_array_view: &point_shadow_array_view,
            shadow_sampler: &shadow_sampler,
            sky_height_view: &sky_height_view,
            sun_view: &sun_view,
            moon_view: &moon_view,
            cloud_noise_view: &cloud_noise_view,
            weather_view: &weather_view,
            cloud_sampler: &cloud_sampler,
            sky_sampler: &sky_sampler,
            clouds_layout: &clouds_layout,
            deferred_layout: &deferred_layout,
            gi_layout: &gi_layout,
            bilateral_layout: &bilateral_layout,
            volumetric_layout: &volumetric_layout,
            composite_layout: &composite_layout,
            bloom_layout: &bloom_layout,
            post_layout: &post_layout,
            post_uniform: &post_uniform,
            post_sampler: &post_sampler,
        };
        let targets = create_targets(&device, config.width, config.height, &target_ctx);
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mip bloom"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/bloom.wgsl").into(),
            ),
        });
        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom"),
            bind_group_layouts: &[&bloom_layout],
            push_constant_ranges: &[],
        });
        let make_bloom_pipeline = |label, entry, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&bloom_pl),
                vertex: wgpu::VertexState {
                    module: &bloom_shader,
                    entry_point: "vs_main",
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &bloom_shader,
                    entry_point: entry,
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
            })
        };
        let bloom_prefilter_pipeline = make_bloom_pipeline("Bloom prefilter", "fs_prefilter", None);
        let bloom_downsample_pipeline =
            make_bloom_pipeline("Bloom downsample", "fs_downsample", None);
        let direct_down_pipeline =
            make_bloom_pipeline("Direct HDR downsample", "fs_box_downsample", None);
        let bloom_upsample_pipeline = make_bloom_pipeline(
            "Bloom upsample",
            "fs_upsample",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
        );
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom ACES"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/post.wgsl").into(),
            ),
        });
        let post_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&post_layout],
            push_constant_ranges: &[],
        });
        let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Post"),
            layout: Some(&post_pl),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
        });
        Ok(Self {
            device,
            queue,
            surface,
            config,
            adapter_name,
            meshes: Vec::new(),
            instances,
            globals,
            gbuffer_bindings,
            shadow_bindings,
            point_shadow_uniforms,
            point_shadow_bindings,
            gbuffer_pipeline,
            sky_height_pipeline,
            clouds_pipeline,
            deferred_pipeline,
            deferred_viewmodel_pipeline,
            gi_pipeline,
            gi_blur_pipeline,
            volumetric_pipeline,
            vol_blur_pipeline,
            composite_pipeline,
            shadow_pipeline,
            point_shadow_pipeline,
            bloom_prefilter_pipeline,
            bloom_downsample_pipeline,
            direct_down_pipeline,
            bloom_upsample_pipeline,
            post_pipeline,
            _shadow: shadow,
            shadow_view,
            _sky_height: sky_height,
            sky_height_view,
            _point_shadow: point_shadow,
            point_shadow_array_view,
            point_shadow_face_views,
            shadow_sampler,
            _sun_tex: sun_tex,
            sun_view,
            _moon_tex: moon_tex,
            moon_view,
            _cloud_noise_3d: cloud_noise_3d,
            cloud_noise_view,
            _weather_tex: weather_tex,
            weather_view,
            cloud_sampler,
            sky_sampler,
            clouds_layout,
            deferred_layout,
            gi_layout,
            bilateral_layout,
            volumetric_layout,
            composite_layout,
            bloom_layout,
            targets,
            post_layout,
            post_uniform,
            post_sampler,
            prev_vp: Mat4::IDENTITY,
            prev_eye: Vec3::ZERO,
            frame_index: 0,
        })
    }

    pub fn mesh(&mut self, data: &MeshData) -> MeshId {
        self.upload_mesh(data, false)
    }

    pub fn dynamic_mesh(&mut self, data: &MeshData) -> MeshId {
        self.upload_mesh(data, true)
    }

    fn upload_mesh(&mut self, data: &MeshData, dynamic: bool) -> MeshId {
        let id = self.meshes.len();
        let usage = if dynamic {
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
        } else {
            wgpu::BufferUsages::VERTEX
        };
        self.meshes.push(GpuMesh {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh vertices"),
                    contents: bytemuck::cast_slice(&data.vertices),
                    usage,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh indices"),
                    contents: bytemuck::cast_slice(&data.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            count: data.indices.len() as u32,
        });
        id
    }

    pub fn update_mesh(&self, id: MeshId, data: &MeshData) {
        self.queue.write_buffer(
            &self.meshes[id].vertices,
            0,
            bytemuck::cast_slice(&data.vertices),
        );
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.frame_index = 0;
        let ctx = TargetContext {
            globals: &self.globals,
            shadow_view: &self.shadow_view,
            point_shadow_array_view: &self.point_shadow_array_view,
            shadow_sampler: &self.shadow_sampler,
            sky_height_view: &self.sky_height_view,
            sun_view: &self.sun_view,
            moon_view: &self.moon_view,
            cloud_noise_view: &self.cloud_noise_view,
            weather_view: &self.weather_view,
            cloud_sampler: &self.cloud_sampler,
            sky_sampler: &self.sky_sampler,
            clouds_layout: &self.clouds_layout,
            deferred_layout: &self.deferred_layout,
            gi_layout: &self.gi_layout,
            bilateral_layout: &self.bilateral_layout,
            volumetric_layout: &self.volumetric_layout,
            composite_layout: &self.composite_layout,
            bloom_layout: &self.bloom_layout,
            post_layout: &self.post_layout,
            post_uniform: &self.post_uniform,
            post_sampler: &self.post_sampler,
        };
        self.targets = create_targets(&self.device, width, height, &ctx);
    }

    pub fn vsync(&mut self, on: bool) {
        self.config.present_mode = if on {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        self.surface.configure(&self.device, &self.config);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        camera: &Camera,
        draws: &[Draw],
        point_lights: &[PointLight],
        settings: &RenderSettings,
        time: f32,
    ) {
        assert!(draws.len() < MAX_INSTANCES, "Scene exceeds instance budget");
        let mut sorted = draws.to_vec();
        sorted.sort_by_key(|d| (d.viewmodel, d.shadow, d.mesh));
        let draws = sorted.as_slice();
        let has_viewmodel = draws.iter().any(|d| d.viewmodel);
        let instances: Vec<Instance> = draws.iter().map(Instance::from).collect();
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));

        let aspect = self.config.width as f32 / self.config.height as f32;
        let sun = sun_direction(
            settings.time_of_day,
            settings.sun_azimuth,
            settings.sun_tilt,
        );
        let moon = moon_direction(
            settings.time_of_day,
            settings.sun_azimuth,
            settings.sun_tilt,
            settings.moon_phase,
        );
        let (shadow_dir, _, _) = celestial::dominant_celestial_light(sun, moon);
        let shadow_ext = crate::units::world::SHADOW_EXTENT;
        let light =
            Mat4::orthographic_rh(-shadow_ext, shadow_ext, -shadow_ext, shadow_ext, 0.1, 100.0)
                * Mat4::look_at_rh(
                    Vec3::new(0.0, 1.0, 0.0) + shadow_dir * 50.0,
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::Y,
                );
        let view = camera.view();
        let proj = camera.projection(aspect);
        let vp = proj * view;
        let sky_proj = Mat4::orthographic_rh(-16.0, 16.0, -16.0, 16.0, 0.1, 25.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 20.0, 0.0), Vec3::ZERO, Vec3::NEG_Z);
        if self.frame_index == 0 {
            self.prev_vp = vp;
            self.prev_eye = camera.position;
        }

        let count = point_lights.len().min(MAX_POINT_LIGHTS);
        let mut lights_arr = [PointLight::new(Vec3::ZERO, 1.0, [0.0; 3], 0.0); MAX_POINT_LIGHTS];
        lights_arr[..count].copy_from_slice(&point_lights[..count]);

        let globals = Globals {
            vp: vp.to_cols_array_2d(),
            light: light.to_cols_array_2d(),
            inverse: vp.inverse().to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            prev_vp: self.prev_vp.to_cols_array_2d(),
            sky_proj: sky_proj.to_cols_array_2d(),
            eye: camera.position.extend(1.0).to_array(),
            prev_eye: [
                self.prev_eye.x,
                self.prev_eye.y,
                self.prev_eye.z,
                self.frame_index as f32,
            ],
            proj_params: [
                1.0 / proj.x_axis.x,
                1.0 / proj.y_axis.y,
                crate::units::camera::NEAR_CLIP_M,
                crate::units::camera::FAR_CLIP_M,
            ],
            sun: [sun.x, sun.y, sun.z, count as f32],
            moon: [moon.x, moon.y, moon.z, settings.moon_phase],
            settings: [
                settings.exposure,
                settings.fog,
                settings.shadows as f32,
                if settings.ao { 1.0 } else { 0.0 },
            ],
            sky: [
                time,
                settings.time_of_day,
                settings.sun_azimuth.to_radians(),
                settings.sun_tilt.clamp(10.0, 89.0).to_radians(),
            ],
            atmos: [
                if settings.clouds { 1.0 } else { 0.0 },
                settings.cloud_coverage,
                if settings.show_trajectory { 1.0 } else { 0.0 },
                aspect,
            ],
            gi_vol: [
                if settings.gi { 1.0 } else { 0.0 },
                settings.gi_intensity,
                if settings.volumetric { 1.0 } else { 0.0 },
                settings.volumetric_intensity,
            ],
            lights: lights_arr,
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(
            &self.post_uniform,
            0,
            bytemuck::cast_slice(&[
                settings.exposure,
                if settings.ao { 1.0 } else { 0.0 },
                if settings.bloom { 1.0 } else { 0.0 },
                time,
            ]),
        );

        if settings.shadows > 0 {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Sun shadow pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadow_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.shadow_pipeline);
                pass.set_bind_group(0, &self.shadow_bindings, &[]);
                self.draw_pass(&mut pass, draws, |d| d.shadow);
            }

            let shadow_lights = count.min(MAX_SHADOW_LIGHTS);
            if shadow_lights > 0 {
                let face_dirs = [
                    (Vec3::X, -Vec3::Y),
                    (-Vec3::X, -Vec3::Y),
                    (Vec3::Y, Vec3::Z),
                    (-Vec3::Y, -Vec3::Z),
                    (Vec3::Z, -Vec3::Y),
                    (-Vec3::Z, -Vec3::Y),
                ];
                let mut buf = vec![0u8; shadow_lights * 6 * POINT_UNIFORM_STRIDE];
                for (li, l) in point_lights[..shadow_lights].iter().enumerate() {
                    let pos = Vec3::new(l.pos_radius[0], l.pos_radius[1], l.pos_radius[2]);
                    let far = l.pos_radius[3].max(0.5);
                    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.08, far);
                    for (fi, &(dir, up)) in face_dirs.iter().enumerate() {
                        let face_vp = proj * Mat4::look_at_rh(pos, pos + dir, up);
                        let cols = face_vp.to_cols_array_2d();
                        let offset = (li * 6 + fi) * POINT_UNIFORM_STRIDE;
                        buf[offset..offset + 64].copy_from_slice(bytemuck::bytes_of(&cols));
                    }
                }
                self.queue
                    .write_buffer(&self.point_shadow_uniforms, 0, &buf);

                for slot in 0..(shadow_lights * 6) {
                    let mut ppass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Point shadow face pass"),
                        color_attachments: &[],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.point_shadow_face_views[slot],
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    ppass.set_pipeline(&self.point_shadow_pipeline);
                    ppass.set_bind_group(
                        0,
                        &self.point_shadow_bindings,
                        &[(slot * POINT_UNIFORM_STRIDE) as u32],
                    );
                    self.draw_pass(&mut ppass, draws, |d| d.shadow);
                }
            }
        }

        if settings.gi {
            let mut sky_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sky height pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.sky_height_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            sky_pass.set_pipeline(&self.sky_height_pipeline);
            sky_pass.set_bind_group(0, &self.shadow_bindings, &[]);
            self.draw_pass(&mut sky_pass, draws, |d| !d.viewmodel);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GBuffer pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.albedo_metal,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.normal_rough,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.emissive,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.gbuffer_pipeline);
            pass.set_bind_group(0, &self.gbuffer_bindings, &[]);
            self.draw_pass(&mut pass, draws, |d| !d.viewmodel);
        }

        let slot = (self.frame_index & 1) as usize;
        if settings.clouds {
            let clouds_target = if slot == 0 {
                &self.targets.clouds_a
            } else {
                &self.targets.clouds_b
            };
            fullscreen_pass(
                encoder,
                "Volumetric clouds pass",
                clouds_target,
                &self.clouds_pipeline,
                &self.targets.clouds_bgs[slot],
            );
        }

        fullscreen_pass(
            encoder,
            "Deferred direct + sky pass",
            &self.targets.direct_hdr_mips[0],
            &self.deferred_pipeline,
            &self.targets.deferred[slot],
        );

        if settings.gi {
            for mip in 1..self.targets.direct_hdr_mips.len() {
                fullscreen_pass(
                    encoder,
                    "Direct HDR mip downsample",
                    &self.targets.direct_hdr_mips[mip],
                    &self.direct_down_pipeline,
                    &self.targets.direct_down_bgs[mip - 1],
                );
            }
            let gi_target = if slot == 0 {
                &self.targets.gi_a
            } else {
                &self.targets.gi_b
            };
            fullscreen_pass(
                encoder,
                "Global illumination pass",
                gi_target,
                &self.gi_pipeline,
                &self.targets.gi_bgs[slot],
            );
            fullscreen_pass(
                encoder,
                "GI bilateral blur pass",
                &self.targets.gi_filtered,
                &self.gi_blur_pipeline,
                &self.targets.gi_blur_bgs[slot],
            );
        }

        if settings.volumetric {
            fullscreen_pass(
                encoder,
                "Volumetric light pass",
                &self.targets.vol_raw,
                &self.volumetric_pipeline,
                &self.targets.vol_bgs[slot],
            );
            fullscreen_pass(
                encoder,
                "Volumetric bilateral blur pass",
                &self.targets.vol_filtered,
                &self.vol_blur_pipeline,
                &self.targets.vol_blur_bg,
            );
        }

        fullscreen_pass(
            encoder,
            "HDR composite pass",
            &self.targets.hdr,
            &self.composite_pipeline,
            &self.targets.composite_bg,
        );

        self.prev_vp = vp;
        self.prev_eye = camera.position;
        self.frame_index = self.frame_index.wrapping_add(1);

        if has_viewmodel {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("GBuffer viewmodel pass"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.targets.albedo_metal,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.targets.normal_rough,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.targets.emissive,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.targets.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.gbuffer_pipeline);
                pass.set_bind_group(0, &self.gbuffer_bindings, &[]);
                self.draw_pass(&mut pass, draws, |d| d.viewmodel);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Deferred viewmodel lighting pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.hdr,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.deferred_viewmodel_pipeline);
                pass.set_bind_group(0, &self.targets.deferred[slot], &[]);
                pass.draw(0..3, 0..1);
            }
        }

        if settings.bloom {
            let mips = self.targets.bloom_mips.len();
            for i in 0..mips {
                let mut bpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(if i == 0 {
                        "Bloom prefilter"
                    } else {
                        "Bloom downsample"
                    }),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.bloom_mips[i],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                bpass.set_pipeline(if i == 0 {
                    &self.bloom_prefilter_pipeline
                } else {
                    &self.bloom_downsample_pipeline
                });
                bpass.set_bind_group(0, &self.targets.bloom_down_bgs[i], &[]);
                bpass.draw(0..3, 0..1);
            }
            for i in (0..mips.saturating_sub(1)).rev() {
                let mut upass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Bloom upsample"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.targets.bloom_mips[i],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                upass.set_pipeline(&self.bloom_upsample_pipeline);
                upass.set_bind_group(0, &self.targets.bloom_up_bgs[i], &[]);
                upass.draw(0..3, 0..1);
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tonemap"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.post_pipeline);
        pass.set_bind_group(0, &self.targets.post, &[]);
        pass.draw(0..3, 0..1);
    }

    fn draw_pass<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        draws: &[Draw],
        filter: impl Fn(&Draw) -> bool,
    ) {
        pass.set_vertex_buffer(1, self.instances.slice(..));
        let mut i = 0;
        while i < draws.len() {
            let d = &draws[i];
            let mut end = i + 1;
            while end < draws.len()
                && draws[end].mesh == d.mesh
                && draws[end].shadow == d.shadow
                && draws[end].viewmodel == d.viewmodel
            {
                end += 1;
            }
            if !filter(d) {
                i = end;
                continue;
            }
            let mesh = &self.meshes[d.mesh];
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, i as u32..end as u32);
            i = end;
        }
    }
}

fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const V: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2];
    const I: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4];
    [
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &V,
        },
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &I,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn upload_texture_mips(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    layers: u32,
    data: &[u8],
    srgb: bool,
) -> wgpu::Texture {
    let levels = width.max(height).ilog2() + 1;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes = (width * height * 4) as usize;
    for layer in 0..layers {
        let mut img = image::RgbaImage::from_raw(
            width,
            height,
            data[layer as usize * bytes..(layer as usize + 1) * bytes].to_vec(),
        )
        .unwrap();
        for mip in 0..levels {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &tex,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                img.as_raw(),
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(img.width() * 4),
                    rows_per_image: Some(img.height()),
                },
                wgpu::Extent3d {
                    width: img.width(),
                    height: img.height(),
                    depth_or_array_layers: 1,
                },
            );
            if mip + 1 < levels {
                img = image::imageops::resize(
                    &img,
                    (img.width() / 2).max(1),
                    (img.height() / 2).max(1),
                    image::imageops::FilterType::Triangle,
                );
            }
        }
    }
    tex
}

fn upload_texture_3d(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: u32,
    data: &[u8],
) -> wgpu::Texture {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
    );
    tex
}
