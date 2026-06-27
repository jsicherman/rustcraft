use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable, cast_slice};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBinding,
    BufferBindingType, BufferUsages, ColorTargetState, ColorWrites, CompareFunction,
    DepthStencilState, Device, FragmentState, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, Queue, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
};
use world::TimeOfDay;

use crate::{camera::CameraUniform, math::normalize3};

pub const SKYBOX_VERTICES: [[f32; 3]; 24] = [
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

pub const SKYBOX_INDICES: [u32; 36] = [
    // Back face
    0, 2, 1, 0, 3, 2, // Front face
    4, 5, 6, 4, 6, 7, // Left face
    8, 10, 9, 8, 11, 10, // Right face
    12, 13, 14, 12, 14, 15, // Bottom face
    16, 17, 18, 16, 18, 19, // Top face
    20, 22, 21, 20, 23, 22,
];

pub struct Skybox {
    pub camera_buffer: Buffer,
    pub vertex_buffer: Buffer,
    pub color_buffer: Buffer,
    pub index_buffer: Buffer,
    pub pipeline: RenderPipeline,
    pub index_count: u32,
    pub bind_group: BindGroup,
    pub camera_bind_group: BindGroup,
}

pub struct Lighting {
    pub buffer: Buffer,
    pub bind_group: BindGroup,
    pub bind_group_layout: BindGroupLayout,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SkyboxUniform {
    pub sky_color: [f32; 4],
    pub sun_direction: [f32; 4],
    pub moon_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LightingUniform {
    pub sun_direction_and_strength: [f32; 4],
}

impl Lighting {
    pub fn update(&self, skybox: &Skybox, time_of_day: TimeOfDay, queue: &Queue, buffer: &Buffer) {
        let sky_color = calculate_sky_color(time_of_day);
        let sun_direction = normalize3(time_of_day.sun_direction());
        let sunlight_strength = calculate_sunlight_strength(sun_direction);
        queue.write_buffer(
            &skybox.color_buffer,
            0,
            cast_slice(&[SkyboxUniform {
                sky_color: [sky_color[0], sky_color[1], sky_color[2], 0.0],
                sun_direction: [sun_direction[0], sun_direction[1], sun_direction[2], 1.0],
                moon_params: [0.22, 0.0, 0.0, 0.0],
            }]),
        );

        queue.write_buffer(
            buffer,
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
    }
}

fn lerp(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

const NIGHT_COLOR: [f32; 3] = [0.02, 0.05, 0.08];
const SUNRISE_COLOR: [f32; 3] = [1.00, 0.60, 0.20];
const DAY_COLOR: [f32; 3] = [0.53, 0.81, 0.92];
const SUNSET_COLOR: [f32; 3] = [1.00, 0.40, 0.10];

fn calculate_sky_color(time_of_day: TimeOfDay) -> [f32; 3] {
    let t = time_of_day.to_fraction();

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

fn calculate_sunlight_strength(sun_direction: [f32; 3]) -> f32 {
    sun_direction[1].max(0.0)
}

pub fn configure_lighting(
    device: &Device,
    camera_bgl: &BindGroupLayout,
    surface_format: TextureFormat,
) -> (Skybox, Lighting) {
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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

    let buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("lighting"),
        contents: cast_slice(&[LightingUniform {
            sun_direction_and_strength: [0.0, 1.0, 0.0, 1.0],
        }]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("lighting bg"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });

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
        bind_group_layouts: &[Some(camera_bgl), Some(&skybox_color_bgl)],
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
        layout: camera_bgl,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: skybox_camera_buffer.as_entire_binding(),
        }],
    });

    let skybox_color_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("skybox color buffer"),
        contents: cast_slice(&[SkyboxUniform {
            sky_color: [DAY_COLOR[0], DAY_COLOR[1], DAY_COLOR[2], 0.0],
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
        buffer,
        bind_group,
        bind_group_layout,
    };

    (skybox, lighting)
}
