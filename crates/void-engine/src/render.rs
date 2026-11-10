use crate::{
    mesh::{MeshData, Vertex},
    textures,
};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::{path::Path, sync::Arc};
use wgpu::util::DeviceExt;
use winit::window::Window;

pub type MeshId = usize;
const MAX_INSTANCES: usize = 16384;
const SHADOW_SIZE: u32 = 2048;

#[derive(Clone, Copy)]
pub struct Material {
    pub color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: f32,
    pub layer: u32,
    pub uv: [f32; 2],
    pub baked: bool,
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
            baked: false,
        }
    }
    pub fn glow(color: [f32; 3], amount: f32) -> Self {
        Self {
            emissive: amount,
            ..Self::new(0, color, 0.5, 0.0)
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
            uv: [m.uv[0], m.uv[1], if m.baked { 1.0 } else { 0.0 }, 0.0],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    vp: [[f32; 4]; 4],
    light: [[f32; 4]; 4],
    inverse: [[f32; 4]; 4],
    eye: [f32; 4],
    sun: [f32; 4],
    settings: [f32; 4],
    lights: [[f32; 4]; 4],
}

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 1.68, 9.6),
            yaw: 0.0,
            pitch: -0.07,
            fov: 78.0,
        }
    }
}
impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            -self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }
    pub fn world(&self) -> Mat4 {
        self.view().inverse()
    }
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
    }
}
pub struct RenderSettings {
    pub exposure: f32,
    pub fog: f32,
    pub shadows: u32,
    pub ao: bool,
    pub bloom: bool,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            exposure: 1.05,
            fog: 0.6,
            shadows: 2,
            ao: true,
            bloom: true,
        }
    }
}
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}
struct Targets {
    _hdr: wgpu::Texture,
    hdr: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth: wgpu::TextureView,
    _view_depth: wgpu::Texture,
    view_depth: wgpu::TextureView,
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
    view_globals: wgpu::Buffer,
    bindings: wgpu::BindGroup,
    view_bindings: wgpu::BindGroup,
    shadow_bindings: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    post_pipeline: wgpu::RenderPipeline,
    _shadow: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    targets: Targets,
    post_layout: wgpu::BindGroupLayout,
    post_uniform: wgpu::Buffer,
    post_sampler: wgpu::Sampler,
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
fn targets(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
) -> Targets {
    let (hdr, hdr_view) = target(
        device,
        "HDR scene",
        w,
        h,
        wgpu::TextureFormat::Rgba16Float,
        1,
    );
    let (depth, depth_view) = target(
        device,
        "World depth",
        w,
        h,
        wgpu::TextureFormat::Depth32Float,
        1,
    );
    let (vd, vd_view) = target(
        device,
        "Viewmodel depth",
        w,
        h,
        wgpu::TextureFormat::Depth32Float,
        1,
    );
    let post = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Post bindings"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&hdr_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&depth_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    Targets {
        _hdr: hdr,
        hdr: hdr_view,
        _depth: depth,
        depth: depth_view,
        _view_depth: vd,
        view_depth: vd_view,
        post,
    }
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        backend: &str,
        adapter_index: Option<usize>,
        assets: &Path,
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
        let uniform = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = uniform("World camera");
        let view_globals = uniform("Independent 62 degree viewmodel camera");
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PBR layout"),
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
                texture_entry(
                    4,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(5, true),
                texture_entry(
                    6,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
            ],
        });
        let atlas = textures::generate();
        let color = upload_array(&device, &queue, "Albedo + roughness", &atlas.albedo, true);
        let normal = upload_array(&device, &queue, "Tangent normals", &atlas.normal, false);
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
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (shadow, shadow_view) = target(
            &device,
            "2048 PCF sun shadow",
            SHADOW_SIZE,
            SHADOW_SIZE,
            wgpu::TextureFormat::Depth32Float,
            1,
        );
        let image = crate::glb::load(&assets.join("models/void-manipulator.glb"))?
            .image
            .context("Manipulator missing embedded image")?;
        let tool = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("2048 tool baked colour"),
                size: wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            image.as_raw(),
        );
        let tool_view = tool.create_view(&Default::default());
        let bind = |buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("PBR bindings"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
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
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&tool_view),
                    },
                ],
            })
        };
        let bindings = bind(&globals);
        let view_bindings = bind(&view_globals);
        // The shadow pass has a uniform-only layout: never bind a depth attachment as its own sampled texture.
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GGX PBR"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/pbr.wgsl").into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PBR"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow"),
            bind_group_layouts: &[&shadow_layout],
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PBR forward"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                // The authored GLB is double-sided and the left glove is mirrored.
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
                module: &shader,
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
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Post layout"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(1, false),
                texture_entry(
                    2,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                uniform_entry(3),
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
        let targets = targets(
            &device,
            config.width,
            config.height,
            &post_layout,
            &post_uniform,
            &post_sampler,
        );
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSAO bloom ACES"),
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
            view_globals,
            bindings,
            view_bindings,
            shadow_bindings,
            pipeline,
            shadow_pipeline,
            post_pipeline,
            _shadow: shadow,
            shadow_view,
            targets,
            post_layout,
            post_uniform,
            post_sampler,
        })
    }
    pub fn mesh(&mut self, data: &MeshData) -> MeshId {
        let id = self.meshes.len();
        self.meshes.push(GpuMesh {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh vertices"),
                    contents: bytemuck::cast_slice(&data.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
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
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.targets = targets(
            &self.device,
            width,
            height,
            &self.post_layout,
            &self.post_uniform,
            &self.post_sampler,
        );
    }
    pub fn vsync(&mut self, on: bool) {
        self.config.present_mode = if on {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        self.surface.configure(&self.device, &self.config);
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        camera: &Camera,
        draws: &[Draw],
        settings: &RenderSettings,
        time: f32,
    ) {
        assert!(draws.len() < MAX_INSTANCES, "Scene exceeds instance budget");
        let mut sorted = draws.to_vec();
        sorted.sort_by_key(|d| (d.viewmodel, d.shadow, d.mesh));
        let draws = sorted.as_slice();
        let instances: Vec<Instance> = draws.iter().map(Instance::from).collect();
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        let aspect = self.config.width as f32 / self.config.height as f32;
        let sun = Vec3::new(-0.45, 0.82, 0.28).normalize();
        let light = Mat4::orthographic_rh(-22.0, 22.0, -22.0, 22.0, 0.1, 85.0)
            * Mat4::look_at_rh(
                Vec3::new(0.0, 1.0, 0.0) + sun * 42.0,
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::Y,
            );
        let vp = Mat4::perspective_rh(camera.fov.to_radians(), aspect, 0.06, 120.0) * camera.view();
        let mut globals = Globals {
            vp: vp.to_cols_array_2d(),
            light: light.to_cols_array_2d(),
            inverse: vp.inverse().to_cols_array_2d(),
            eye: camera.position.extend(1.0).to_array(),
            sun: sun.extend(1.0).to_array(),
            settings: [
                settings.exposure,
                settings.fog,
                settings.shadows as f32,
                time,
            ],
            lights: [
                [-9.0, 5.8, 1.0, 65.0],
                [7.0, 5.8, -5.0, 85.0],
                [0.0, 7.5, -8.0, 55.0],
                [3.0, 5.0, 7.0, 40.0],
            ],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        globals.vp = (Mat4::perspective_rh(62.0f32.to_radians(), aspect, 0.025, 120.0)
            * camera.view())
        .to_cols_array_2d();
        self.queue
            .write_buffer(&self.view_globals, 0, bytemuck::bytes_of(&globals));
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
            self.draw_pass(&mut pass, draws, |d| d.shadow && !d.viewmodel);
        }
        for viewmodel in [false, true] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if viewmodel {
                    "Viewmodel pass"
                } else {
                    "World HDR pass"
                }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if viewmodel {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.23,
                                g: 0.30,
                                b: 0.39,
                                a: 1.0,
                            })
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if viewmodel {
                        &self.targets.view_depth
                    } else {
                        &self.targets.depth
                    },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(
                0,
                if viewmodel {
                    &self.view_bindings
                } else {
                    &self.bindings
                },
                &[],
            );
            self.draw_pass(&mut pass, draws, |d| d.viewmodel == viewmodel);
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
fn upload_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    data: &[u8],
    srgb: bool,
) -> wgpu::Texture {
    let levels = textures::SIZE.ilog2() + 1;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: textures::SIZE,
            height: textures::SIZE,
            depth_or_array_layers: textures::LAYERS,
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
    for layer in 0..textures::LAYERS {
        let bytes = (textures::SIZE * textures::SIZE * 4) as usize;
        let mut img = image::RgbaImage::from_raw(
            textures::SIZE,
            textures::SIZE,
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
