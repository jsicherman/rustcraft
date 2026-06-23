use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};

use block::BlockRegistry;
use bytemuck::{Pod, Zeroable, cast_slice};
use egui::{Context, ViewportId};
use egui_wgpu::ScreenDescriptor;
use egui_winit::State;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferSlice, BufferUsages, Color, ColorTargetState, ColorWrites, CompareFunction,
    CurrentSurfaceTexture, DepthStencilState, Device, DeviceDescriptor, Extent3d, Face,
    FragmentState, IndexFormat, Instance, LoadOp, Operations, PipelineLayoutDescriptor,
    PowerPreference, PresentMode, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
};
use winit::{event::WindowEvent, window::Window};

use crate::{
    debug_overlay::{DebugOverlayData, draw as draw_debug_overlay},
    mesh::{Material, Vertex, build_mesh_geometry},
    texture::{MaterialTextures, build_texture_array},
};

pub struct Renderer {
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    texture_bind_group: BindGroup,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    depth_texture: Texture,
    materials: Vec<MaterialTextures>,
    egui_ctx: Context,
    egui_renderer: egui_wgpu::Renderer,
    egui_state: State,
    pub chunk_builder: MeshBuilder<(i32, i32)>,
    pub entity_builder: MeshBuilder<u64>,
}

pub struct MeshGpu {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_count: u32,
}

pub struct MeshCpu {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

struct MeshBuildJob<K: PartialEq + Send + Sync + 'static> {
    key: K,
    voxels: Vec<u32>,
    size_xyz: [usize; 3],
    offset: [f32; 3],
}

pub struct MeshBuildResult<K: PartialEq + Send + Sync + 'static> {
    pub key: K,
    pub mesh: MeshCpu,
}

pub struct MeshBuilder<K: PartialEq + Send + Sync + 'static> {
    job_tx: Sender<MeshBuildJob<K>>,
    result_rx: Receiver<MeshBuildResult<K>>,
}

impl MeshGpu {
    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}

impl MeshCpu {
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}

impl<K: PartialEq + Send + Sync + 'static> MeshBuilder<K> {
    pub fn new(material_layers: Vec<[u32; 6]>) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<MeshBuildJob<K>>();
        let (result_tx, result_rx) = mpsc::channel::<MeshBuildResult<K>>();

        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let cpu_mesh =
                    build_cpu_mesh(&job.voxels, job.size_xyz, job.offset, &material_layers);

                if result_tx
                    .send(MeshBuildResult {
                        key: job.key,
                        mesh: cpu_mesh,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self { job_tx, result_rx }
    }

    pub fn enqueue(&self, key: K, voxels: Vec<u32>, size_xyz: [usize; 3], offset: [f32; 3]) {
        let _ = self.job_tx.send(MeshBuildJob {
            key,
            voxels,
            size_xyz,
            offset,
        });
    }

    pub fn collect_results(&mut self) -> Vec<MeshBuildResult<K>> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }

        results
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
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

pub async fn init(window: Arc<Window>, block_registry: &BlockRegistry) -> Renderer {
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

    let depth_texture = make_depth_texture(&device, size.width, size.height);

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("voxel shader"),
        source: ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[Some(&camera_bgl), Some(&texture_bgl)],
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

    let entity_builder = MeshBuilder::new(material_layers.clone());
    let chunk_builder = MeshBuilder::new(material_layers);

    Renderer {
        device,
        queue,
        surface,
        surface_config,
        pipeline,
        camera_buffer,
        camera_bind_group,
        depth_texture,
        texture_bind_group,
        materials,
        egui_ctx,
        egui_renderer,
        egui_state,
        chunk_builder,
        entity_builder,
    }
}

impl Renderer {
    pub fn material_layers(&self) -> Vec<[u32; 6]> {
        self.materials.iter().map(|m| m.0).collect()
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

    pub fn render(
        &mut self,
        window: &Window,
        meshes: &[&MeshGpu],
        view_proj: [[f32; 4]; 4],
        debug_overlay: &DebugOverlayData,
    ) {
        let raw_input = self.egui_state.take_egui_input(window);
        let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
            draw_debug_overlay(ctx, debug_overlay);
        });
        self.egui_state
            .handle_platform_output(window, full_output.platform_output);
        let primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            cast_slice(&[CameraUniform { view_proj }]),
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

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_bind_group(1, &self.texture_bind_group, &[]);

            for mesh in meshes {
                if mesh.index_count == 0 {
                    continue;
                }

                rpass.set_vertex_buffer(0, self.vertex_buffer_slice(mesh));
                rpass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
                rpass.draw_indexed(0..mesh.index_count, 0, 0..1);
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

    /// Send voxel data to the GPU and get a handle to a renderable mesh.
    pub fn upload_voxels(&self, voxels: &[Material], size_xyz: [usize; 3]) -> MeshGpu {
        self.upload_voxels_with_offset(voxels, size_xyz, [0.0, 0.0, 0.0])
    }

    /// Send voxel data to the GPU and bake a world-space offset into all vertices.
    pub fn upload_voxels_with_offset(
        &self,
        voxels: &[Material],
        size_xyz: [usize; 3],
        offset: [f32; 3],
    ) -> MeshGpu {
        let cpu_mesh = build_cpu_mesh(voxels, size_xyz, offset, &self.material_layers());
        self.upload_cpu_mesh(cpu_mesh)
    }

    pub fn upload_cpu_mesh(&self, cpu_mesh: MeshCpu) -> MeshGpu {
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

        MeshGpu {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }
}

fn build_cpu_mesh(
    voxels: &[Material],
    size_xyz: [usize; 3],
    offset: [f32; 3],
    material_layers: &[[u32; 6]],
) -> MeshCpu {
    let (vertices, indices) = build_mesh_geometry(voxels, size_xyz, offset, material_layers);

    MeshCpu { vertices, indices }
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
