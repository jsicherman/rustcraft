use bytemuck::{Pod, Zeroable};
use wgpu::{Buffer, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

use crate::texture::{BlockScale, MaterialTextures};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Face {
    Empty,
    Solid {
        material: Material,
        size: BlockScale,
    },
}

pub(crate) type Material = u32;

pub struct DirectionalQuad {
    direction: Direction,
    slice: usize,
    quad: Quad,
}

pub struct Quad {
    x: usize,
    y: usize,

    width: usize,
    height: usize,

    material: Material,
    size: BlockScale,
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

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) uv: [f32; 2],
    pub(crate) material: Material,
}

impl Vertex {
    pub fn desc() -> VertexBufferLayout<'static> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    PlusX,
    MinusX,
    PlusY,
    MinusY,
    PlusZ,
    MinusZ,
}
impl Direction {
    fn normal(self) -> [isize; 3] {
        match self {
            Self::PlusX => [1, 0, 0],
            Self::MinusX => [-1, 0, 0],
            Self::PlusY => [0, 1, 0],
            Self::MinusY => [0, -1, 0],
            Self::PlusZ => [0, 0, 1],
            Self::MinusZ => [0, 0, -1],
        }
    }

    fn slice_axis(self) -> usize {
        match self {
            Self::PlusX | Self::MinusX => 0,
            Self::PlusY | Self::MinusY => 1,
            Self::PlusZ | Self::MinusZ => 2,
        }
    }

    fn mask_axes(self) -> (usize, usize) {
        match self {
            Self::PlusX | Self::MinusX => (2, 1),
            Self::PlusY | Self::MinusY => (0, 2),
            Self::PlusZ | Self::MinusZ => (0, 1),
        }
    }
}

fn voxel_idx(x: usize, y: usize, z: usize, size: [usize; 3]) -> usize {
    x + z * size[0] + y * size[0] * size[2]
}

pub fn mesh_chunk(
    voxels: &[u8],
    scale_layers: &[BlockScale],
    size: [usize; 3],
) -> Vec<DirectionalQuad> {
    const DIRECTIONS: [Direction; 6] = [
        Direction::PlusX,
        Direction::MinusX,
        Direction::PlusY,
        Direction::MinusY,
        Direction::PlusZ,
        Direction::MinusZ,
    ];

    DIRECTIONS
        .into_iter()
        .flat_map(|direction| mesh_direction(voxels, size, scale_layers, direction))
        .collect()
}

pub(crate) fn build_mesh_geometry(
    voxels: &[u8],
    size: [usize; 3],
    material_layers: &[MaterialTextures],
    scale_layers: &[BlockScale],
) -> (Vec<Vertex>, Vec<u32>) {
    let quads = mesh_chunk(voxels, scale_layers, size);

    let mut vertices = Vec::with_capacity(quads.len() * 4);
    let mut indices = Vec::with_capacity(quads.len() * 6);

    for quad in &quads {
        let base = vertices.len() as u32;
        let (verts, idxs) = quad_to_vertices_with_layers(quad, material_layers);

        vertices.extend_from_slice(&verts);
        indices.extend(idxs.iter().map(|i| i + base));
    }

    (vertices, indices)
}

fn quad_to_vertices_with_layers(
    quad: &DirectionalQuad,
    material_layers: &[MaterialTextures],
) -> ([Vertex; 4], MaterialTextures) {
    let (u_axis, v_axis) = quad.direction.mask_axes();

    let u0 = quad.quad.x as f32 + quad.quad.size[u_axis][1];
    let v0 = quad.quad.y as f32 + quad.quad.size[v_axis][1];

    let u1 = quad.quad.x as f32
        + (quad.quad.width - 1) as f32
        + quad.quad.size[u_axis][1]
        + quad.quad.size[u_axis][0];

    let v1 = quad.quad.y as f32
        + (quad.quad.height - 1) as f32
        + quad.quad.size[v_axis][1]
        + quad.quad.size[v_axis][0];
    let slice = quad.slice as f32;

    let material = texture_layer_from_layers(material_layers, quad.quad.material, quad.direction);

    let p0 = uvw_map(quad.direction, slice, u0, v0, quad.quad.size);
    let p1 = uvw_map(quad.direction, slice, u1, v0, quad.quad.size);
    let p2 = uvw_map(quad.direction, slice, u0, v1, quad.quad.size);
    let p3 = uvw_map(quad.direction, slice, u1, v1, quad.quad.size);

    let normal = quad.direction.normal();
    let normal = [normal[0] as f32, normal[1] as f32, normal[2] as f32];

    let (uv0, uv1, uv2, uv3) = match quad.direction {
        Direction::PlusX | Direction::MinusX | Direction::PlusZ | Direction::MinusZ => (
            [0.0, quad.quad.height as f32],
            [quad.quad.width as f32, quad.quad.height as f32],
            [0.0, 0.0],
            [quad.quad.width as f32, 0.0],
        ),
        Direction::PlusY | Direction::MinusY => (
            [0.0, 0.0],
            [quad.quad.width as f32, 0.0],
            [0.0, quad.quad.height as f32],
            [quad.quad.width as f32, quad.quad.height as f32],
        ),
    };

    let vertices = [
        Vertex {
            position: p0,
            normal,
            uv: uv0,
            material,
        },
        Vertex {
            position: p1,
            normal,
            uv: uv1,
            material,
        },
        Vertex {
            position: p2,
            normal,
            uv: uv2,
            material,
        },
        Vertex {
            position: p3,
            normal,
            uv: uv3,
            material,
        },
    ];

    let indices = match quad.direction {
        Direction::PlusX | Direction::PlusY | Direction::MinusZ => [0, 2, 1, 2, 3, 1],
        Direction::MinusX | Direction::MinusY | Direction::PlusZ => [0, 1, 2, 2, 1, 3],
    };

    (vertices, indices)
}

fn texture_layer_from_layers(
    material_layers: &[MaterialTextures],
    material: Material,
    direction: Direction,
) -> u32 {
    let layers = material_layers[material as usize];

    match direction {
        Direction::PlusX => layers[0],
        Direction::MinusX => layers[1],
        Direction::PlusY => layers[2],
        Direction::MinusY => layers[3],
        Direction::PlusZ => layers[4],
        Direction::MinusZ => layers[5],
    }
}

fn uvw_map(direction: Direction, slice: f32, u: f32, v: f32, scale: BlockScale) -> [f32; 3] {
    let slice_axis = direction.slice_axis();
    let (ua, va) = direction.mask_axes();

    let mut p = [0.0; 3];

    p[slice_axis] = if matches!(
        direction,
        Direction::PlusX | Direction::PlusY | Direction::PlusZ
    ) {
        slice + scale[slice_axis][1] + scale[slice_axis][0]
    } else {
        slice + scale[slice_axis][1]
    };

    p[ua] = u;
    p[va] = v;

    p
}

fn build_mask(
    voxels: &[u8],
    size: [usize; 3],
    scale_layers: &[BlockScale],
    slice: usize,
    direction: Direction,
) -> (Vec<Face>, usize, usize) {
    let slice_axis = direction.slice_axis();
    let (u_axis, v_axis) = direction.mask_axes();

    let width = size[u_axis];
    let height = size[v_axis];

    let mut mask = vec![Face::Empty; width * height];

    for v in 0..height {
        for u in 0..width {
            let mut pos = [0; 3];

            pos[slice_axis] = slice;
            pos[u_axis] = u;
            pos[v_axis] = v;

            let voxel = voxels[voxel_idx(pos[0], pos[1], pos[2], size)];

            if voxel == 0 {
                continue;
            }

            if is_face_exposed(voxels, pos, size, scale_layers, direction) {
                mask[v * width + u] = Face::Solid {
                    material: voxel as u32,
                    size: scale_layers[voxel as usize],
                };
            }
        }
    }

    (mask, width, height)
}

fn mesh_direction(
    voxels: &[u8],
    size: [usize; 3],
    scale_layers: &[BlockScale],
    direction: Direction,
) -> Vec<DirectionalQuad> {
    let slice_count = size[direction.slice_axis()];
    let (u, v) = direction.mask_axes();

    (0..slice_count)
        .flat_map(|slice| {
            let (mut mask, width, height) =
                build_mask(voxels, size, scale_layers, slice, direction);

            let quads = greedy_mesh(&mut mask, width, height, u, v);

            quads.into_iter().map(move |q| DirectionalQuad {
                direction,
                slice,
                quad: Quad {
                    x: q.x,
                    y: q.y,
                    width: q.width,
                    height: q.height,
                    material: q.material,
                    size: q.size,
                },
            })
        })
        .collect()
}

fn greedy_mesh(
    mask: &mut [Face],
    width: usize,
    height: usize,
    u_axis: usize,
    v_axis: usize,
) -> Vec<Quad> {
    let mut quads = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            let Face::Solid { material, size } = mask[idx] else {
                continue;
            };

            let can_merge_u = size[u_axis] == [1.0, 0.0];
            let can_merge_v = size[v_axis] == [1.0, 0.0];

            let mut quad_width = 1;
            if can_merge_u {
                while x + quad_width < width && mask[idx + quad_width] == mask[idx] {
                    quad_width += 1;
                }
            }

            let mut quad_height = 1;
            if can_merge_v {
                'outer: loop {
                    if y + quad_height >= height {
                        break;
                    }
                    for dx in 0..quad_width {
                        if mask[idx + quad_height * width + dx] != mask[idx] {
                            break 'outer;
                        }
                    }
                    quad_height += 1;
                }
            }

            for dy in 0..quad_height {
                for dx in 0..quad_width {
                    mask[idx + dy * width + dx] = Face::Empty;
                }
            }

            quads.push(Quad {
                x,
                y,
                width: quad_width,
                height: quad_height,
                material,
                size,
            });
        }
    }

    quads
}

fn is_face_exposed(
    voxels: &[u8],
    location: [usize; 3],
    size: [usize; 3],
    scale_layers: &[BlockScale],
    direction: Direction,
) -> bool {
    let [dx, dy, dz] = direction.normal();

    let nx = location[0] as isize + dx;
    let ny = location[1] as isize + dy;
    let nz = location[2] as isize + dz;

    if nx < 0
        || ny < 0
        || nz < 0
        || nx >= size[0] as isize
        || ny >= size[1] as isize
        || nz >= size[2] as isize
    {
        return true;
    }

    let neighbor = voxels[voxel_idx(nx as usize, ny as usize, nz as usize, size)];
    if neighbor == 0 {
        return true;
    }

    let our_voxel = voxels[voxel_idx(location[0], location[1], location[2], size)];

    let our = scale_layers[our_voxel as usize];
    let neighbor = scale_layers[neighbor as usize];

    let slice_axis = direction.slice_axis();
    let (ua, va) = direction.mask_axes();

    let is_positive = matches!(
        direction,
        Direction::PlusX | Direction::PlusY | Direction::PlusZ
    );

    let our_touches_shared_face = if is_positive {
        our[slice_axis][1] + our[slice_axis][0] == 1.0
    } else {
        our[slice_axis][1] == 0.0
    };

    let neighbor_touches_shared_face = if is_positive {
        neighbor[slice_axis][1] == 0.0
    } else {
        neighbor[slice_axis][1] + neighbor[slice_axis][0] == 1.0
    };

    if !(our_touches_shared_face && neighbor_touches_shared_face) {
        return true;
    }

    let our_u_min = our[ua][1];
    let our_u_max = our_u_min + our[ua][0];

    let our_v_min = our[va][1];
    let our_v_max = our_v_min + our[va][0];

    let neighbor_u_min = neighbor[ua][1];
    let neighbor_u_max = neighbor_u_min + neighbor[ua][0];

    let neighbor_v_min = neighbor[va][1];
    let neighbor_v_max = neighbor_v_min + neighbor[va][0];

    let covers_u = neighbor_u_min <= our_u_min && neighbor_u_max >= our_u_max;
    let covers_v = neighbor_v_min <= our_v_min && neighbor_v_max >= our_v_max;

    !(covers_u && covers_v)
}
