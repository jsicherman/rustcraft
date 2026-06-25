use block::TexturePack;
use spatial::{
    orientation::Orientation,
    vectors::{Global, Vec3fGlobal, Vec3iGlobal},
};

use crate::{Block, ChunkProvider};

/// Raycasts a line from the given origin in the given direction,
/// returning the first solid block hit and the face normal of that block
pub fn raycast<CP: ChunkProvider>(
    origin: Vec3fGlobal,
    orientation: Orientation,
    max_distance: f32,
    world: &CP,
    textures: &TexturePack,
) -> Option<(Block<Global>, Vec3iGlobal)> {
    let dir = orientation.look_direction();

    let signum = |x: f32| {
        if x > 0.0 {
            1
        } else if x < 0.0 {
            -1
        } else {
            0
        }
    };

    let step = Vec3iGlobal::new(signum(dir.x()), signum(dir.y()), signum(dir.z()));

    let t_delta: Vec3fGlobal = [
        1.0 / dir.x().abs(),
        1.0 / dir.y().abs(),
        1.0 / dir.z().abs(),
    ]
    .into();

    let t_max = {
        let frac = origin - origin.floor();
        let frac = Vec3fGlobal::new(
            if dir.x() > 0.0 {
                1.0 - frac.x()
            } else {
                frac.x()
            },
            if dir.y() > 0.0 {
                1.0 - frac.y()
            } else {
                frac.y()
            },
            if dir.z() > 0.0 {
                1.0 - frac.z()
            } else {
                frac.z()
            },
        );
        t_delta * frac
    };

    let mut t_max = t_max;
    let mut face = Vec3iGlobal::ZERO;

    let mut block = origin.floor();
    loop {
        let hit_block = world.block(block)?;

        if textures
            .get_block_type(hit_block.id())
            .solidity()
            .is_solid()
        {
            return Some((hit_block, face));
        }

        if t_max.x() < t_max.y() && t_max.x() < t_max.z() {
            if t_max.x() > max_distance {
                return None;
            }
            block[0] += step.x() as f32;
            face = Vec3iGlobal::new(-step.x(), 0, 0);
            t_max[0] += t_delta.x();
        } else if t_max.y() < t_max.z() {
            if t_max.y() > max_distance {
                return None;
            }
            block[1] += step.y() as f32;
            face = Vec3iGlobal::new(0, -step.y(), 0);
            t_max[1] += t_delta.y();
        } else {
            if t_max.z() > max_distance {
                return None;
            }
            block[2] += step.z() as f32;
            face = Vec3iGlobal::new(0, 0, -step.z());
            t_max[2] += t_delta.z();
        }
    }
}
