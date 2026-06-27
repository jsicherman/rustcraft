use resources::ResourcePack;
use resources::block::BlockType;
use spatial::{
    aabb::AxisAlignedBoundingBox,
    orientation::Orientation,
    vectors::{Global, Vec3fGlobal, Vec3iGlobal},
};

use crate::{Block, ChunkProvider};

pub fn raycast<CP: ChunkProvider>(
    origin: Vec3fGlobal,
    orientation: Orientation,
    max_distance: f32,
    world: &CP,
    textures: &ResourcePack,
) -> Option<(Block<Global>, BlockType, Vec3iGlobal)> {
    let dir = orientation.look_direction();

    let step = Vec3iGlobal::new(
        dir.x().signum() as i32,
        dir.y().signum() as i32,
        dir.z().signum() as i32,
    );

    let t_delta: Vec3fGlobal = [
        if dir.x().abs() < f32::EPSILON {
            f32::INFINITY
        } else {
            1.0 / dir.x().abs()
        },
        if dir.y().abs() < f32::EPSILON {
            f32::INFINITY
        } else {
            1.0 / dir.y().abs()
        },
        if dir.z().abs() < f32::EPSILON {
            f32::INFINITY
        } else {
            1.0 / dir.z().abs()
        },
    ]
    .into();

    let t_max = {
        let frac = origin - origin.floor();
        Vec3fGlobal::new(
            if dir.x().abs() < f32::EPSILON {
                f32::INFINITY
            } else if dir.x() > 0.0 {
                (1.0 - frac.x()) / dir.x().abs()
            } else {
                frac.x() / dir.x().abs()
            },
            if dir.y().abs() < f32::EPSILON {
                f32::INFINITY
            } else if dir.y() > 0.0 {
                (1.0 - frac.y()) / dir.y().abs()
            } else {
                frac.y() / dir.y().abs()
            },
            if dir.z().abs() < f32::EPSILON {
                f32::INFINITY
            } else if dir.z() > 0.0 {
                (1.0 - frac.z()) / dir.z().abs()
            } else {
                frac.z() / dir.z().abs()
            },
        )
    };

    let mut t_max = t_max;
    let mut block = origin.floor();

    loop {
        let hit_block = world.block(block)?;
        let block_type = textures.get_block_type(hit_block.id());

        if block_type.solidity().is_solid() {
            let candidate_aabb = hit_block.aabb(block_type);
            if let Some(hit_face) = ray_aabb_face(origin, dir, candidate_aabb, max_distance) {
                return Some((hit_block, *block_type, hit_face));
            }
        }

        if t_max.x() < t_max.y() && t_max.x() < t_max.z() {
            if t_max.x() > max_distance {
                return None;
            }
            block[0] += step.x() as f32;
            t_max[0] += t_delta.x();
        } else if t_max.y() < t_max.z() {
            if t_max.y() > max_distance {
                return None;
            }
            block[1] += step.y() as f32;
            t_max[1] += t_delta.y();
        } else {
            if t_max.z() > max_distance {
                return None;
            }
            block[2] += step.z() as f32;
            t_max[2] += t_delta.z();
        }
    }
}

fn ray_aabb_face(
    origin: Vec3fGlobal,
    dir: Vec3fGlobal,
    aabb: AxisAlignedBoundingBox<Global>,
    max_distance: f32,
) -> Option<Vec3iGlobal> {
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    let mut hit_axis = 0;
    let mut hit_sign = 1;

    for axis in 0..3 {
        let d = dir[axis];
        let min = aabb.min()[axis];
        let max = aabb.max()[axis];

        if d.abs() < f32::EPSILON {
            if origin[axis] < min || origin[axis] > max {
                return None;
            }
        } else {
            let t1 = (min - origin[axis]) / d;
            let t2 = (max - origin[axis]) / d;

            let (t_enter, t_exit, sign) = if t1 < t2 { (t1, t2, -1) } else { (t2, t1, 1) };

            if t_enter > t_min {
                t_min = t_enter;
                hit_axis = axis;
                hit_sign = sign;
            }
            t_max = t_max.min(t_exit);
        }
    }

    if t_min > t_max || t_max < 0.0 || t_min > max_distance {
        return None;
    }

    let face = match hit_axis {
        0 => Vec3iGlobal::new(hit_sign, 0, 0),
        1 => Vec3iGlobal::new(0, hit_sign, 0),
        _ => Vec3iGlobal::new(0, 0, hit_sign),
    };

    Some(face)
}
