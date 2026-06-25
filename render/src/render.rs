use std::{collections::HashMap, num::NonZeroU64, sync::Arc};

use block::TexturePack;
use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use egui::{Context, ViewportId};
use egui_wgpu::ScreenDescriptor;
use egui_winit::State;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBinding,
    BufferBindingType, BufferDescriptor, BufferSlice, BufferUsages, Color, ColorTargetState,
    ColorWrites, CompareFunction, CurrentSurfaceTexture, DepthStencilState, Device,
    DeviceDescriptor, Extent3d, Face, FragmentState, IndexFormat, Instance, LoadOp, Operations,
    PipelineLayoutDescriptor, PowerPreference, PresentMode, PrimitiveState, PrimitiveTopology,
    Queue, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
};
use winit::{event::WindowEvent, window::Window};
use world::TimeOfDay;

use crate::{
    OverlayParticle,
    VoxelMesher,
    debug::{DebugOverlayData, draw as draw_debug_overlay},
    mesher::Vertex,
    model::{
        Asset, MeshAsset, MeshHandle, ModelAsset, ModelHandle, RenderCommandGpu, RenderHandle,
        RenderInstance,
    },
    overlay,
    texture::{MaterialTextures, build_texture_array},
};

struct Skybox {
    pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_count: u32,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    color_buffer: Buffer,
    bind_group: BindGroup,
}

struct Lighting {
    buffer: Buffer,
    bind_group: BindGroup,
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    skybox: Skybox,
    lighting: Lighting,
    texture_bind_group: BindGroup,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    object_buffer: Buffer,
    object_bind_group: BindGroup,
    object_uniform_stride: u64,
    depth_texture: Texture,
    pub meshes: HashMap<MeshHandle, MeshAsset>,
    pub models: HashMap<ModelHandle, ModelAsset>,
    mesh_handle_counter: u32,
    materials: Vec<MaterialTextures>,
    egui_ctx: Context,
    egui_renderer: egui_wgpu::Renderer,
    egui_state: State,
    pub chunk_builder: VoxelMesher,
}

pub struct MeshGpu {
    pub(crate) vertex_buffer: Buffer,
    pub(crate) index_buffer: Buffer,
    pub(crate) index_count: u32,
}

pub struct MeshCpu {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
}

const MAX_DRAW_OBJECTS: u64 = 16_384;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyboxUniform {
    sky_color: [f32; 4],
    sun_direction: [f32; 4],
    moon_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightingUniform {
    sun_direction_and_strength: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniform {
    transform: [[f32; 4]; 4],
    mat_layers_0: [u32; 4],
    mat_layers_1: [u32; 4],
}

fn lerp(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn calculate_sky_color(time_of_day: TimeOfDay) -> [f32; 3] {
    const NIGHT_COLOR: [f32; 3] = [0.02, 0.05, 0.08];
    const SUNRISE_COLOR: [f32; 3] = [1.00, 0.60, 0.20];
    const DAY_COLOR: [f32; 3] = [0.53, 0.81, 0.92];
    const SUNSET_COLOR: [f32; 3] = [1.00, 0.40, 0.10];

    let t = time_of_day.0;

    if t < 0.13 {
        NIGHT_COLOR
    } else if t < 0.25 {
        let progress = (t - 0.13) / 0.12;
        lerp(&NIGHT_COLOR, &SUNRISE_COLOR, progress)
    } else if t < 0.35 {
        let progress = (t - 0.25) / 0.10;
        lerp(&SUNRISE_COLOR, &DAY_COLOR, progress)
    } else if t < 0.65 {
        DAY_COLOR
    } else if t < 0.75 {
        let progress = (t - 0.65) / 0.10;
        lerp(&DAY_COLOR, &SUNSET_COLOR, progress)
    } else if t < 0.90 {
        let progress = (t - 0.75) / 0.15;
        lerp(&SUNSET_COLOR, &NIGHT_COLOR, progress)
    } else {
        NIGHT_COLOR
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        let inv_len = 1.0 / len_sq.sqrt();
        [v[0] * inv_len, v[1] * inv_len, v[2] * inv_len]
    }
}

fn calculate_sunlight_strength(sun_direction: [f32; 3]) -> f32 {
    sun_direction[1].max(0.0)
}

impl Vertex {
    fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: std::mem::offset_of!(Vertex, position) as u64,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::offset_of!(Vertex, normal) as u64,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::offset_of!(Vertex, uv) as u64,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: std::mem::offset_of!(Vertex, material) as u64,
                    format: VertexFormat::Uint32,
                    shader_location: 3,
                },
            ],
        }
    }
}

pub async fn init(window: Arc<Window>, block_registry: &TexturePack) -> Renderer {
    let size = window.inner_size();

    let instance = Instance::default();
    let surface = instance.create_surface(Arc::clone(&window)).unwrap();

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("no adapter found");

    let (device, queue) = adapter
        .request_device(&DeviceDescriptor::default())
        .await
        .expect("no device found");

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);

    let surface_config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width,
        height: size.height,
        present_mode: PresentMode::AutoVsync,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &surface_config);

    let camera_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("camera"),
        contents: cast_slice(&[CameraUniform {
            view_proj: [[0.0; 4]; 4],
        }]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let camera_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("camera bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("camera bg"),
        layout: &camera_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let object_uniform_size = std::mem::size_of::<ObjectUniform>() as u64;
    let min_uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
    let object_uniform_stride =
        object_uniform_size.div_ceil(min_uniform_alignment) * min_uniform_alignment;

    let object_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("object buffer"),
        size: object_uniform_stride * MAX_DRAW_OBJECTS,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let object_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX_FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(object_uniform_size),
            },
            count: None,
        }],
    });

    let object_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("object bg"),
        layout: &object_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &object_buffer,
                offset: 0,
                size: NonZeroU64::new(object_uniform_size),
            }),
        }],
    });

    let texture_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("texture array bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let lighting_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("lighting bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(std::mem::size_of::<LightingUniform>() as u64),
            },
            count: None,
        }],
    });

    let lighting_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("lighting"),
        contents: cast_slice(&[LightingUniform {
            sun_direction_and_strength: [0.0, 1.0, 0.0, 1.0],
        }]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let lighting_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("lighting bg"),
        layout: &lighting_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: lighting_buffer.as_entire_binding(),
        }],
    });

    let depth_texture = make_depth_texture(&device, size.width, size.height);

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("voxel shader"),
        source: ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[
            Some(&camera_bgl),
            Some(&object_bgl),
            Some(&texture_bgl),
            Some(&lighting_bgl),
        ],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("voxel pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: surface_format,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            cull_mode: Some(Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let (textures, materials) = build_texture_array(&device, &queue, block_registry).unwrap();

    let texture_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("texture array bind group"),
        layout: &texture_bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(textures.view()),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(textures.sampler()),
            },
        ],
    });

    let egui_ctx = Context::default();
    let egui_state = State::new(
        egui_ctx.clone(),
        ViewportId::ROOT,
        &*window,
        None,
        None,
        None,
    );

    let egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, Default::default());

    let material_layers: Vec<_> = materials.iter().map(|m| m.0).collect();

    let chunk_builder = VoxelMesher::new(material_layers);

    let skybox_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("skybox shader"),
        source: ShaderSource::Wgsl(include_str!("../shaders/skybox.wgsl").into()),
    });

    let skybox_color_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("skybox color bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(std::mem::size_of::<SkyboxUniform>() as u64),
            },
            count: None,
        }],
    });

    let skybox_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("skybox pipeline layout"),
        bind_group_layouts: &[Some(&camera_bgl), Some(&skybox_color_bgl)],
        immediate_size: 0,
    });

    let skybox_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("skybox pipeline"),
        layout: Some(&skybox_pipeline_layout),
        vertex: VertexState {
            module: &skybox_shader,
            entry_point: Some("vs_main"),
            buffers: &[VertexBufferLayout {
                array_stride: std::mem::size_of::<[f32; 3]>() as u64,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                }],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &skybox_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: surface_format,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    const SKYBOX_VERTICES: [[f32; 3]; 24] = [
        // Back face
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        // Front face
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        // Left face
        [-1.0, -1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
        // Right face
        [1.0, -1.0, -1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, -1.0],
        // Bottom face
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, -1.0, 1.0],
        [-1.0, -1.0, 1.0],
        // Top face
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    const SKYBOX_INDICES: [u32; 36] = [
        // Back face
        0, 2, 1, 0, 3, 2, // Front face
        4, 5, 6, 4, 6, 7, // Left face
        8, 10, 9, 8, 11, 10, // Right face
        12, 13, 14, 12, 14, 15, // Bottom face
        16, 17, 18, 16, 18, 19, // Top face
        20, 22, 21, 20, 23, 22,
    ];

    let skybox_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("skybox vertex buffer"),
        contents: bytemuck::cast_slice(&SKYBOX_VERTICES),
        usage: BufferUsages::VERTEX,
    });

    let skybox_index_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("skybox index buffer"),
        contents: bytemuck::cast_slice(&SKYBOX_INDICES),
        usage: BufferUsages::INDEX,
    });

    let skybox_camera_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("skybox camera"),
        contents: cast_slice(&[CameraUniform {
            view_proj: [[0.0; 4]; 4],
        }]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let skybox_camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("skybox camera bg"),
        layout: &camera_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: skybox_camera_buffer.as_entire_binding(),
        }],
    });

    let initial_sky_color = calculate_sky_color(TimeOfDay::new(0.5));
    let skybox_color_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("skybox color buffer"),
        contents: cast_slice(&[SkyboxUniform {
            sky_color: [
                initial_sky_color[0],
                initial_sky_color[1],
                initial_sky_color[2],
                0.0,
            ],
            sun_direction: [0.0, 1.0, 0.0, 1.0],
            moon_params: [0.22, 0.0, 0.0, 0.0],
        }]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let skybox_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("skybox bind group"),
        layout: &skybox_color_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &skybox_color_buffer,
                offset: 0,
                size: NonZeroU64::new(std::mem::size_of::<SkyboxUniform>() as u64),
            }),
        }],
    });

    let skybox = Skybox {
        pipeline: skybox_pipeline,
        vertex_buffer: skybox_vertex_buffer,
        index_buffer: skybox_index_buffer,
        index_count: SKYBOX_INDICES.len() as u32,
        camera_buffer: skybox_camera_buffer,
        camera_bind_group: skybox_camera_bind_group,
        color_buffer: skybox_color_buffer,
        bind_group: skybox_bind_group,
    };

    let lighting = Lighting {
        buffer: lighting_buffer,
        bind_group: lighting_bind_group,
    };

    Renderer {
        device,
        queue,
        surface,
        surface_config,
        pipeline,
        skybox,
        lighting,
        camera_buffer,
        camera_bind_group,
        depth_texture,
        texture_bind_group,
        object_buffer,
        object_bind_group,
        object_uniform_stride,
        materials,
        egui_ctx,
        egui_renderer,
        egui_state,
        chunk_builder,
        meshes: Default::default(),
        models: Default::default(),
        mesh_handle_counter: Default::default(),
    }
}

impl Renderer {
    pub fn remove_mesh(&mut self, handle: &MeshHandle) {
        self.meshes.remove(handle);
    }

    pub fn contains_model(&self, handle: &ModelHandle) -> bool {
        self.models.contains_key(handle)
    }

    pub fn insert_model(&mut self, handle: ModelHandle, model: ModelAsset) {
        self.models.insert(handle, model);
    }

    pub fn material_layers(&self) -> Vec<[u32; 6]> {
        self.materials.iter().map(|m| m.0).collect()
    }

    fn build_commands(&self, instances: &[&RenderInstance], stack: &mut Vec<RenderCommandGpu>) {
        for instance in instances {
            match instance.handle() {
                RenderHandle::Mesh(mesh) => {
                    if let Some(mesh_asset) = self.meshes.get(&mesh) {
                        mesh_asset.build(instance, stack);
                    }
                }
                RenderHandle::Model(model) => {
                    if let Some(model_asset) = self.models.get(&model) {
                        model_asset.build(instance, stack);
                    }
                }
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_texture = make_depth_texture(&self.device, width, height);
    }

    pub fn handle_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.egui_state.on_window_event(window, event).consumed
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        stack: &mut Vec<RenderCommandGpu>,
        window: &Window,
        instances: &[&RenderInstance],
        view_matrices: ([[f32; 4]; 4], [[f32; 4]; 4]),
        target_position_normal: Option<([f32; 3], [f32; 3])>,
        overlay_particles: &[OverlayParticle],
        debug_overlay: &DebugOverlayData,
        time_of_day: TimeOfDay,
    ) {
        let (view_proj, skybox_view_proj) = view_matrices;

        stack.clear();

        self.build_commands(instances, stack);

        let raw_input = self.egui_state.take_egui_input(window);
        let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
            overlay::foreground_overlays(ctx, target_position_normal, view_proj, overlay_particles);
            draw_debug_overlay(ctx, debug_overlay);
        });
        self.egui_state
            .handle_platform_output(window, full_output.platform_output);
        let primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let sky_color = calculate_sky_color(time_of_day);
        let sun_direction = normalize3(time_of_day.sun_direction());
        let sunlight_strength = calculate_sunlight_strength(sun_direction);
        self.queue.write_buffer(
            &self.skybox.color_buffer,
            0,
            cast_slice(&[SkyboxUniform {
                sky_color: [sky_color[0], sky_color[1], sky_color[2], 0.0],
                sun_direction: [sun_direction[0], sun_direction[1], sun_direction[2], 1.0],
                moon_params: [0.22, 0.0, 0.0, 0.0],
            }]),
        );

        self.queue.write_buffer(
            &self.lighting.buffer,
            0,
            cast_slice(&[LightingUniform {
                sun_direction_and_strength: [
                    sun_direction[0],
                    sun_direction[1],
                    sun_direction[2],
                    sunlight_strength,
                ],
            }]),
        );

        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            cast_slice(&[CameraUniform { view_proj }]),
        );

        self.queue.write_buffer(
            &self.skybox.camera_buffer,
            0,
            cast_slice(&[CameraUniform {
                view_proj: skybox_view_proj,
            }]),
        );

        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            CurrentSurfaceTexture::Timeout => return,
            CurrentSurfaceTexture::Occluded => return,
            CurrentSurfaceTexture::Outdated => return,
            CurrentSurfaceTexture::Lost => return,
            CurrentSurfaceTexture::Validation => return,
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let depth_view = self
            .depth_texture
            .create_view(&TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Render skybox
            {
                rpass.set_pipeline(&self.skybox.pipeline);
                rpass.set_bind_group(0, &self.skybox.camera_bind_group, &[]);
                rpass.set_bind_group(1, &self.skybox.bind_group, &[]);
                rpass.set_vertex_buffer(0, self.skybox.vertex_buffer.slice(..));
                rpass.set_index_buffer(self.skybox.index_buffer.slice(..), IndexFormat::Uint32);
                rpass.draw_indexed(0..self.skybox.index_count, 0, 0..1);
            }

            // Render voxels
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_bind_group(2, &self.texture_bind_group, &[]);
            rpass.set_bind_group(3, &self.lighting.bind_group, &[]);

            let draw_count = stack.len().min(MAX_DRAW_OBJECTS as usize);
            if draw_count < stack.len() {
                tracing::warn!(
                    "draw list truncated: {} commands, max {}",
                    stack.len(),
                    MAX_DRAW_OBJECTS
                );
            }

            if draw_count != 0 {
                let stride = self.object_uniform_stride as usize;
                let mut object_upload = vec![0u8; draw_count * stride];

                for (draw_idx, command) in stack.iter().take(draw_count).enumerate() {
                    let start = draw_idx * stride;
                    let object_uniform = ObjectUniform {
                        transform: command.transform,
                        mat_layers_0: if let Some(mat) = command.material {
                            let mut layers = [0u32; 4];
                            layers[0] = 1;
                            layers[1..4].copy_from_slice(&mat.0[0..3]);
                            layers
                        } else {
                            [0u32; 4]
                        },
                        mat_layers_1: if let Some(mat) = command.material {
                            let mut layers = [0u32; 4];
                            layers[0..3].copy_from_slice(&mat.0[3..6]);
                            layers
                        } else {
                            [0u32; 4]
                        },
                    };

                    let uniform_bytes = bytes_of(&object_uniform);
                    object_upload[start..start + uniform_bytes.len()]
                        .copy_from_slice(uniform_bytes);
                }

                self.queue
                    .write_buffer(&self.object_buffer, 0, &object_upload);
            }

            for (draw_idx, command) in stack.iter().take(draw_count).enumerate() {
                let Some(asset) = self.meshes.get(&command.mesh) else {
                    continue;
                };

                if asset.mesh.index_count == 0 {
                    continue;
                }

                let object_offset = (draw_idx as u64 * self.object_uniform_stride) as u32;
                rpass.set_bind_group(1, &self.object_bind_group, &[object_offset]);

                rpass.set_vertex_buffer(0, self.vertex_buffer_slice(&asset.mesh));
                rpass.set_index_buffer(asset.mesh.index_buffer.slice(..), IndexFormat::Uint32);
                rpass.draw_indexed(0..asset.mesh.index_count, 0, 0..1);
            }
        }

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );

        {
            let egui_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut egui_pass = egui_pass.forget_lifetime();

            self.egui_renderer
                .render(&mut egui_pass, &primitives, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }

    fn vertex_buffer_slice<'a>(&self, mesh: &'a MeshGpu) -> BufferSlice<'a> {
        mesh.vertex_buffer.slice(..)
    }

    pub fn upload_mesh(&mut self, cpu_mesh: MeshCpu) -> MeshHandle {
        let MeshCpu { vertices, indices } = cpu_mesh;

        let vertex_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("index buffer"),
            contents: cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        let mesh_asset = MeshAsset {
            mesh: MeshGpu {
                vertex_buffer,
                index_buffer,
                index_count: indices.len() as u32,
            },
            material: None,
        };

        let handle = MeshHandle::from(self.mesh_handle_counter);
        self.mesh_handle_counter += 1;

        self.meshes.insert(handle, mesh_asset);

        handle
    }

    pub fn cube(&mut self) -> MeshHandle {
        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);

        let mut push_face =
            |positions: [[f32; 3]; 4], normal: [f32; 3], material: u32, plus: bool| {
                let base = vertices.len() as u32;

                let uvs = [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
                for i in 0..4 {
                    vertices.push(Vertex {
                        position: positions[i],
                        normal,
                        uv: uvs[i],
                        material,
                    });
                }

                if plus {
                    indices.extend_from_slice(&[
                        base,
                        base + 2,
                        base + 1,
                        base + 2,
                        base + 3,
                        base + 1,
                    ]);
                } else {
                    indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base + 2,
                        base + 1,
                        base + 3,
                    ]);
                }
            };

        push_face(
            [
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
            ],
            [1.0, 0.0, 0.0],
            0,
            true,
        );
        push_face(
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
            ],
            [-1.0, 0.0, 0.0],
            0,
            false,
        );
        push_face(
            [
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            [0.0, 1.0, 0.0],
            0,
            true,
        );
        push_face(
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
            [0.0, -1.0, 0.0],
            0,
            false,
        );
        push_face(
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            [0.0, 0.0, 1.0],
            0,
            false,
        );
        push_face(
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            [0.0, 0.0, -1.0],
            0,
            true,
        );

        self.upload_mesh(MeshCpu { vertices, indices })
    }
}

fn make_depth_texture(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("depth"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
