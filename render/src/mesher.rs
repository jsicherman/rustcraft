use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    Empty,
    Solid(Material),
}

pub(crate) type Material = u32;

pub struct Quad {
    direction: Direction,

    slice: usize,

    x: usize,
    y: usize,

    width: usize,
    height: usize,

    material: Material,
}

pub struct RawQuad {
    x: usize,
    y: usize,

    width: usize,
    height: usize,

    material: Material,
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

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) uv: [f32; 2],
    pub(crate) material: Material,
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

pub fn mesh_chunk(voxels: &[Material], size: [usize; 3]) -> Vec<Quad> {
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
        .flat_map(|direction| mesh_direction(voxels, size, direction))
        .collect()
}

pub(crate) fn build_mesh_geometry(
    voxels: &[Material],
    size: [usize; 3],
    material_layers: &[[u32; 6]],
) -> (Vec<Vertex>, Vec<u32>) {
    let quads = mesh_chunk(voxels, size);

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
    quad: &Quad,
    material_layers: &[[u32; 6]],
) -> ([Vertex; 4], [u32; 6]) {
    let u0 = quad.x as f32;
    let v0 = quad.y as f32;
    let u1 = (quad.x + quad.width) as f32;
    let v1 = (quad.y + quad.height) as f32;
    let slice = quad.slice as f32;
    let material = texture_layer_from_layers(material_layers, quad.material, quad.direction);

    let p0 = map_position(quad.direction, slice, u0, v0);
    let p1 = map_position(quad.direction, slice, u1, v0);
    let p2 = map_position(quad.direction, slice, u0, v1);
    let p3 = map_position(quad.direction, slice, u1, v1);

    let normal = quad.direction.normal();
    let normal = [normal[0] as f32, normal[1] as f32, normal[2] as f32];

    let (uv0, uv1, uv2, uv3) = match quad.direction {
        Direction::PlusX | Direction::MinusX | Direction::PlusZ | Direction::MinusZ => (
            [0.0, quad.height as f32],
            [quad.width as f32, quad.height as f32],
            [0.0, 0.0],
            [quad.width as f32, 0.0],
        ),
        Direction::PlusY | Direction::MinusY => (
            [0.0, 0.0],
            [quad.width as f32, 0.0],
            [0.0, quad.height as f32],
            [quad.width as f32, quad.height as f32],
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
    material_layers: &[[u32; 6]],
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

fn map_position(direction: Direction, slice: f32, u: f32, v: f32) -> [f32; 3] {
    fn axis_add(p: &mut [f32; 3], axis: usize, value: f32) {
        p[axis] = value;
    }

    let mut p = [0.0; 3];

    let slice_axis = direction.slice_axis();
    let (ua, va) = direction.mask_axes();

    let is_positive = matches!(
        direction,
        Direction::PlusX | Direction::PlusY | Direction::PlusZ
    );
    let slice_pos = if is_positive { slice + 1.0 } else { slice };

    axis_add(&mut p, slice_axis, slice_pos);
    axis_add(&mut p, ua, u);
    axis_add(&mut p, va, v);

    p
}

fn build_mask(
    voxels: &[Material],
    size: [usize; 3],
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

            if is_face_exposed(voxels, pos, size, direction) {
                mask[v * width + u] = Face::Solid(voxel);
            }
        }
    }

    (mask, width, height)
}

fn mesh_direction(voxels: &[Material], size: [usize; 3], direction: Direction) -> Vec<Quad> {
    let slice_count = size[direction.slice_axis()];

    (0..slice_count)
        .flat_map(|slice| {
            let (mut mask, width, height) = build_mask(voxels, size, slice, direction);

            let quads = greedy_mesh(&mut mask, width, height);

            quads.into_iter().map(move |q| Quad {
                direction,
                slice,
                x: q.x,
                y: q.y,
                width: q.width,
                height: q.height,
                material: q.material,
            })
        })
        .collect()
}

fn greedy_mesh(mask: &mut [Face], width: usize, height: usize) -> Vec<RawQuad> {
    let mut quads = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            let Face::Solid(material) = mask[idx] else {
                continue;
            };

            let mut quad_width = 1;
            while x + quad_width < width && mask[idx + quad_width] == Face::Solid(material) {
                quad_width += 1;
            }

            let mut quad_height = 1;
            'outer: loop {
                if y + quad_height >= height {
                    break;
                }

                for dx in 0..quad_width {
                    if mask[idx + quad_height * width + dx] != Face::Solid(material) {
                        break 'outer;
                    }
                }

                quad_height += 1;
            }

            for dy in 0..quad_height {
                for dx in 0..quad_width {
                    mask[idx + dy * width + dx] = Face::Empty;
                }
            }

            quads.push(RawQuad {
                x,
                y,
                width: quad_width,
                height: quad_height,
                material,
            });
        }
    }

    quads
}

fn is_face_exposed(
    voxels: &[Material],
    location: [usize; 3],
    size: [usize; 3],
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

    voxels[voxel_idx(nx as usize, ny as usize, nz as usize, size)] == 0
}
