use std::num::NonZeroU64;

use crate::{
    Renderer,
    mesher::{MeshCpu, Vertex},
    model::MeshHandle,
};

use bytemuck::{Pod, Zeroable};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBinding, BufferBindingType,
    BufferDescriptor, BufferUsages, Device, ShaderStages,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ObjectUniform {
    pub transform: [[f32; 4]; 4],
    pub scale: [f32; 4],
    pub mat_layers_0: [u32; 4],
    pub mat_layers_1: [u32; 4],
}

pub struct Object {
    pub buffer: Buffer,
    pub bind_group: BindGroup,
    pub stride: u64,
}

pub fn cube(renderer: &mut Renderer) -> MeshHandle {
    let faces: [([[f32; 3]; 4], [f32; 3], bool); 6] = [
        (
            [[1., 0., 0.], [1., 0., 1.], [1., 1., 0.], [1., 1., 1.]],
            [1., 0., 0.],
            true,
        ),
        (
            [[0., 0., 0.], [0., 0., 1.], [0., 1., 0.], [0., 1., 1.]],
            [-1., 0., 0.],
            false,
        ),
        (
            [[0., 1., 0.], [1., 1., 0.], [0., 1., 1.], [1., 1., 1.]],
            [0., 1., 0.],
            true,
        ),
        (
            [[0., 0., 0.], [1., 0., 0.], [0., 0., 1.], [1., 0., 1.]],
            [0., -1., 0.],
            false,
        ),
        (
            [[0., 0., 1.], [1., 0., 1.], [0., 1., 1.], [1., 1., 1.]],
            [0., 0., 1.],
            false,
        ),
        (
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
            [0., 0., -1.],
            true,
        ),
    ];

    let uvs = [[0., 1.], [1., 1.], [0., 0.], [1., 0.]];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (positions, normal, plus) in faces {
        let base = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(Vertex {
                position: positions[i],
                normal,
                uv: uvs[i],
                material: 0,
            });
        }

        if plus {
            indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base + 3, base + 1]);
        } else {
            indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
    }

    renderer.upload(MeshCpu { vertices, indices })
}

pub fn configure_object(device: &Device, max_objects: u64) -> (Object, BindGroupLayout) {
    let stride = (std::mem::size_of::<ObjectUniform>() as u64)
        .div_ceil(device.limits().min_uniform_buffer_offset_alignment as u64)
        * device.limits().min_uniform_buffer_offset_alignment as u64;

    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("object buffer"),
        size: stride * max_objects,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX_FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(stride),
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("object bg"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: NonZeroU64::new(stride),
            }),
        }],
    });

    (
        Object {
            buffer,
            bind_group,
            stride,
        },
        bind_group_layout,
    )
}
