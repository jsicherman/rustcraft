use std::{collections::HashMap, path::Path, sync::Arc};

use bytemuck::{bytes_of, cast_slice};
use egui_wgpu::ScreenDescriptor;
use wgpu::{
    BindGroup, BindGroupLayout, BlendState, BufferUsages, Color, ColorTargetState, ColorWrites,
    CompareFunction, CurrentSurfaceTexture, DepthStencilState, Device, DeviceDescriptor, Face,
    FragmentState, IndexFormat, Instance, LoadOp, Operations, PipelineLayoutDescriptor,
    PowerPreference, PresentMode, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModule, StoreOp,
    Surface, SurfaceConfiguration, Texture, TextureFormat, TextureUsages, TextureViewDescriptor,
    VertexState,
    util::{BufferInitDescriptor, DeviceExt},
};
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};
use world::TimeOfDay;

use crate::{
    DebugOverlayData, Mesh, OverlayParticle, VoxelMesher,
    block::{Object, ObjectUniform, configure_object},
    camera::{Camera, CameraUniform, configure_camera},
    lighting::{Lighting, Skybox, configure_lighting},
    mesher::{MeshCpu, Vertex},
    model::{
        Asset, MeshAsset, MeshHandle, ModelAsset, ModelHandle, RenderCommandGpu, RenderHandle,
        RenderInstance,
    },
    overlay::{Gui, configure_gui},
    shader::{configure_depth_shader, depth_texture},
    texture::{BlockScale, configure_textures},
};

struct RenderSurface {
    surface: Surface<'static>,
    config: SurfaceConfiguration,
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    surface: RenderSurface,
    pipeline: RenderPipeline,
    upload_buffer: Vec<u8>,

    skybox: Skybox,
    lighting: Lighting,

    camera: Camera,
    object: Object,

    depth_texture: Texture,
    texture_bind_group: BindGroup,

    gui: Gui,

    mesh_handle_counter: u32,

    pub meshes: HashMap<MeshHandle, MeshAsset>,
    pub models: HashMap<ModelHandle, ModelAsset>,
    pub voxel_mesher: VoxelMesher,
}

const MAX_DRAW_OBJECTS: u64 = 16_384;

async fn configure_window(
    window: &Arc<Window>,
) -> (
    Device,
    Surface<'static>,
    SurfaceConfiguration,
    Queue,
    PhysicalSize<u32>,
    TextureFormat,
) {
    let size = window.inner_size();

    let instance = Instance::default();
    let surface = instance.create_surface(Arc::clone(window)).unwrap();

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

    (device, surface, surface_config, queue, size, surface_format)
}

pub async fn init(
    window: Arc<Window>,
    block_resources: Vec<([&'static Path; 6], BlockScale)>,
    texture_resources: Vec<[&'static Path; 6]>,
) -> Renderer {
    let (device, surface, surface_config, queue, size, surface_format) =
        configure_window(&window).await;

    let (camera, camera_bgl) = configure_camera(&device);

    let (object, object_bgl) = configure_object(&device, MAX_DRAW_OBJECTS);

    let (texture_bgl, texture_bg, material_layers, scale_layers) =
        configure_textures(&device, &queue, block_resources, texture_resources);
    let voxel_mesher = VoxelMesher::new(material_layers.clone(), scale_layers);

    let (skybox, lighting) = configure_lighting(&device, &camera_bgl, surface_format);

    let (depth_texture, shader) = configure_depth_shader(&device, size);

    let pipeline = configure_pipeline(
        &device,
        surface_format,
        &camera_bgl,
        &object_bgl,
        &texture_bgl,
        &lighting.bind_group_layout,
        &shader,
    );

    let gui = configure_gui(&device, &window, surface_format);

    Renderer {
        device,
        surface: RenderSurface {
            surface,
            config: surface_config,
        },
        queue,
        gui,
        pipeline,
        skybox,
        lighting,
        camera,
        depth_texture,
        texture_bind_group: texture_bg,
        object,
        voxel_mesher,
        upload_buffer: Default::default(),
        meshes: Default::default(),
        models: Default::default(),
        mesh_handle_counter: Default::default(),
    }
}

fn configure_pipeline(
    device: &Device,
    surface_format: TextureFormat,
    camera_bgl: &BindGroupLayout,
    object_bgl: &BindGroupLayout,
    texture_bgl: &BindGroupLayout,
    lighting_bgl: &BindGroupLayout,
    shader: &ShaderModule,
) -> RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[
            Some(camera_bgl),
            Some(object_bgl),
            Some(texture_bgl),
            Some(lighting_bgl),
        ],
        immediate_size: 0,
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("voxel pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: shader,
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
    })
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
        self.surface.config.width = width;
        self.surface.config.height = height;
        self.surface
            .surface
            .configure(&self.device, &self.surface.config);
        self.depth_texture = depth_texture(&self.device, width, height);
    }

    pub fn handle_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.gui.state.on_window_event(window, event).consumed
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        stack: &mut Vec<RenderCommandGpu>,
        window: &Window,
        instances: &[&RenderInstance],
        view_proj: [[f32; 4]; 4],
        skybox_view_proj: [[f32; 4]; 4],
        target: Option<[[f32; 3]; 3]>,
        overlay_particles: &[OverlayParticle],
        debug_overlay: &DebugOverlayData,
        time_of_day: TimeOfDay,
    ) {
        stack.clear();

        self.build_commands(instances, stack);

        let raw_input = self.gui.state.take_egui_input(window);
        let full_output = self.gui.context.run_ui(raw_input, |ctx| {
            Gui::render_overlay(ctx, target, view_proj, overlay_particles);
            debug_overlay.draw(ctx);
        });
        self.gui
            .state
            .handle_platform_output(window, full_output.platform_output);
        let primitives = self
            .gui
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        self.lighting.update(
            &self.skybox,
            time_of_day,
            &self.queue,
            &self.lighting.buffer,
        );

        self.queue.write_buffer(
            &self.camera.buffer,
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

        let frame = match self.surface.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            CurrentSurfaceTexture::Timeout
            | CurrentSurfaceTexture::Occluded
            | CurrentSurfaceTexture::Outdated
            | CurrentSurfaceTexture::Lost
            | CurrentSurfaceTexture::Validation => return,
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
            rpass.set_pipeline(&self.skybox.pipeline);
            rpass.set_bind_group(0, &self.skybox.camera_bind_group, &[]);
            rpass.set_bind_group(1, &self.skybox.bind_group, &[]);
            rpass.set_vertex_buffer(0, self.skybox.vertex_buffer.slice(..));
            rpass.set_index_buffer(self.skybox.index_buffer.slice(..), IndexFormat::Uint32);
            rpass.draw_indexed(0..self.skybox.index_count, 0, 0..1);

            // Render voxels
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.camera.bind_group, &[]);
            rpass.set_bind_group(2, &self.texture_bind_group, &[]);
            rpass.set_bind_group(3, &self.lighting.bind_group, &[]);

            let draw_count = stack.len().min(MAX_DRAW_OBJECTS as usize);
            if draw_count < stack.len() {
                tracing::warn!(
                    "draw truncated: {} commands, max {MAX_DRAW_OBJECTS}",
                    stack.len(),
                );
            }

            if draw_count > 0 {
                let stride = self.object.stride as usize;
                self.upload_buffer.clear();
                if self.upload_buffer.len() < draw_count * stride {
                    self.upload_buffer.resize(draw_count * stride, 0);
                }

                for (draw_idx, command) in stack.iter().take(draw_count).enumerate() {
                    let start = draw_idx * stride;
                    let object_uniform = ObjectUniform {
                        transform: command.transform,
                        scale: [command.scale[0], command.scale[1], command.scale[2], 0.0],
                        mat_layers_0: if let Some(mat) = command.material {
                            let mut layers = [0; 4];
                            layers[0] = 1;
                            layers[1..4].copy_from_slice(&mat[0..3]);
                            layers
                        } else {
                            [0; 4]
                        },
                        mat_layers_1: if let Some(mat) = command.material {
                            let mut layers = [0; 4];
                            layers[0..3].copy_from_slice(&mat[3..6]);
                            layers
                        } else {
                            [0; 4]
                        },
                    };

                    let uniform_bytes = bytes_of(&object_uniform);
                    self.upload_buffer[start..start + uniform_bytes.len()]
                        .copy_from_slice(uniform_bytes);
                }

                self.queue
                    .write_buffer(&self.object.buffer, 0, &self.upload_buffer);
            }

            for (draw_idx, command) in stack.iter().take(draw_count).enumerate() {
                let Some(asset) = self.meshes.get(&command.mesh) else {
                    continue;
                };

                if asset.mesh.index_count == 0 {
                    continue;
                }

                let object_offset = (draw_idx as u64 * self.object.stride) as u32;
                rpass.set_bind_group(1, &self.object.bind_group, &[object_offset]);

                rpass.set_vertex_buffer(0, asset.mesh.vertex_buffer.slice(..));
                rpass.set_index_buffer(asset.mesh.index_buffer.slice(..), IndexFormat::Uint32);
                rpass.draw_indexed(0..asset.mesh.index_count, 0, 0..1);
            }
        }

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.surface.config.width, self.surface.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        for (id, delta) in &full_output.textures_delta.set {
            self.gui
                .renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }

        self.gui.renderer.update_buffers(
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

            self.gui
                .renderer
                .render(&mut egui_pass, &primitives, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        for id in &full_output.textures_delta.free {
            self.gui.renderer.free_texture(id);
        }
    }

    pub fn upload(&mut self, cpu_mesh: MeshCpu) -> MeshHandle {
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
            mesh: Mesh {
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
}
