use std::{collections::HashMap, time::Duration};

use chunk::{Block, ChunkMap, ChunkProvider};
use resources::block::BlockType;
use resources::{ResourcePack, entity::ModelDefinition};
use smallvec::SmallVec;
use spatial::{
    aabb::{Aabb, AxisAlignedBoundingBox},
    orientation::{Direction, Orientation},
    vectors::{Global, Vec3fGlobal},
};

use crate::{
    BoxCollider, CollisionStatus, Entity, EntityModel, EntityOrientation, EntityPosition,
    EntityVelocity, MovementIntent, World,
};

const GRAVITY: f32 = -12.5;
const TERMINAL_VELOCITY: f32 = -50.0;

pub enum MoveBundle {
    Motion {
        position: EntityPosition,
        velocity: EntityVelocity,
        collision: CollisionStatus,
    },
    Orientation {
        position: EntityPosition,
        orientation: EntityOrientation,
    },
    Full {
        position: EntityPosition,
        velocity: EntityVelocity,
        orientation: EntityOrientation,
        collision: CollisionStatus,
    },
}

impl MoveBundle {
    pub fn position(&self) -> Vec3fGlobal {
        match self {
            Self::Motion { position, .. } => position.0,
            Self::Orientation { position, .. } => position.0,
            Self::Full { position, .. } => position.0,
        }
    }

    pub fn orientation(&self) -> Option<EntityOrientation> {
        match self {
            Self::Motion { .. } => None,
            Self::Orientation { orientation, .. } | Self::Full { orientation, .. } => {
                Some(*orientation)
            }
        }
    }

    pub fn collision(&self) -> Option<CollisionStatus> {
        match self {
            Self::Motion { collision, .. } | Self::Full { collision, .. } => Some(*collision),
            Self::Orientation { .. } => None,
        }
    }

    pub fn velocity(&self) -> Option<EntityVelocity> {
        match self {
            Self::Motion { velocity, .. } | Self::Full { velocity, .. } => Some(*velocity),
            Self::Orientation { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CollisionEvent {
    toi: Duration,
    normal: Vec3fGlobal,
    penetration: f32,
}

/// Apply gravity/jump to the given velocity, based on their `collision_status`
pub fn apply_gravity(
    mut velocity: Vec3fGlobal,
    jump_strength: f32,
    intent: &MovementIntent,
    collision_status: CollisionStatus,
    dt: Duration,
) -> Vec3fGlobal {
    let is_grounded = collision_status == CollisionStatus::OnGround && velocity.y() <= 0.0;

    if intent.jump && (is_grounded || intent.fly) {
        velocity += [0.0, jump_strength, 0.0].into();
    } else if !is_grounded && !intent.fly {
        velocity += [0.0, GRAVITY * dt.as_secs_f32(), 0.0].into();
        velocity.clamp((.., TERMINAL_VELOCITY.., ..));
    }

    velocity
}

/// Swept AABB narrow-phase test against a static AABB.
///
/// Returns time-of-impact and collision normal when a collision occurs
fn narrow_phase_aabb(
    moving: AxisAlignedBoundingBox<Global>,
    target: AxisAlignedBoundingBox<Global>,
    velocity: Vec3fGlobal,
    dt: Duration,
) -> Option<CollisionEvent> {
    if moving.intersects_overlaps(&target) {
        let dd = [0, 1, 2].map(|idx| {
            (
                target.max()[idx] - moving.min()[idx],
                moving.max()[idx] - target.min()[idx],
            )
        });
        let d = dd.map(|(left, right)| left.min(right));
        let (axis, _) = d
            .iter()
            .copied()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((1, d[1]));

        let normal = match axis {
            0 => {
                if dd[0].0 < dd[0].1 {
                    Direction::PlusX
                } else {
                    Direction::MinusX
                }
            }
            1 => {
                if dd[1].0 < dd[1].1 {
                    Direction::PlusY
                } else {
                    Direction::MinusY
                }
            }
            _ => {
                if dd[2].0 < dd[2].1 {
                    Direction::PlusZ
                } else {
                    Direction::MinusZ
                }
            }
        }
        .into();

        return Some(CollisionEvent {
            toi: Duration::ZERO,
            normal,
            penetration: d[axis],
        });
    }

    let dt_secs = dt.as_secs_f32();
    if dt_secs <= f32::EPSILON {
        return None;
    }

    fn axis_entry_exit(
        moving_min: f32,
        moving_max: f32,
        target_min: f32,
        target_max: f32,
        velocity: f32,
    ) -> Option<(f32, f32)> {
        if velocity > 0.0 {
            Some((target_min - moving_max, target_max - moving_min))
        } else if velocity < 0.0 {
            Some((target_max - moving_min, target_min - moving_max))
        } else if moving_max <= target_min || moving_min >= target_max {
            None
        } else {
            Some((f32::NEG_INFINITY, f32::INFINITY))
        }
    }

    let entries = [0, 1, 2]
        .map(|idx| {
            let (moving_min, moving_max) = (moving.min()[idx], moving.max()[idx]);
            let (target_min, target_max) = (target.min()[idx], target.max()[idx]);
            axis_entry_exit(
                moving_min,
                moving_max,
                target_min,
                target_max,
                velocity[idx],
            )
            .map(|(entry_dist, exit_dist)| (idx, entry_dist, exit_dist))
            .unwrap_or((idx, f32::NAN, f32::NAN))
        })
        .map(|(idx, entry_dist, exit_dist)| {
            if entry_dist.is_nan() || exit_dist.is_nan() {
                return (f32::NAN, f32::NAN);
            }

            let vel = velocity[idx];
            let (entry_time, exit_time) = if vel == 0.0 {
                (f32::NEG_INFINITY, f32::INFINITY)
            } else {
                (entry_dist / vel, exit_dist / vel)
            };

            (entry_time, exit_time)
        });

    if entries
        .iter()
        .any(|(entry, exit)| entry.is_nan() || exit.is_nan())
    {
        return None;
    }

    let (entry_time, exit_time) = entries.iter().fold(
        (f32::NEG_INFINITY, f32::INFINITY),
        |(entry_time, exit_time), (entry_time_axis, exit_time_axis)| {
            (
                entry_time.max(*entry_time_axis),
                exit_time.min(*exit_time_axis),
            )
        },
    );

    if entry_time > exit_time || exit_time < 0.0 || entry_time > dt_secs {
        return None;
    }

    let axis = {
        let entry_times = [entries[0].0, entries[1].0, entries[2].0];
        let abs_vel = [velocity.x().abs(), velocity.y().abs(), velocity.z().abs()];
        let mut best = 0usize;
        for idx in 1..3 {
            let t_best = entry_times[best];
            let t_cur = entry_times[idx];
            if t_cur > t_best + VELOCITY_EPSILON
                || ((t_cur - t_best).abs() <= VELOCITY_EPSILON
                    && abs_vel[idx] > abs_vel[best] + VELOCITY_EPSILON)
            {
                best = idx;
            }
        }
        best
    };

    let normal = match axis {
        0 => Vec3fGlobal::from(if velocity.x() > 0.0 {
            Direction::MinusX
        } else {
            Direction::PlusX
        }),
        1 => Vec3fGlobal::from(if velocity.y() > 0.0 {
            Direction::MinusY
        } else {
            Direction::PlusY
        }),
        _ => Vec3fGlobal::from(if velocity.z() > 0.0 {
            Direction::MinusZ
        } else {
            Direction::PlusZ
        }),
    };

    Some(CollisionEvent {
        toi: Duration::from_secs_f32(entry_time.max(0.0)),
        normal,
        penetration: 0.0,
    })
}

const MOVE_EPSILON: f32 = 1e-4;
const FILTER_EPSILON: f32 = 1e-4;
const VELOCITY_EPSILON: f32 = 1e-6;
const TOI_EPSILON: f32 = 1e-5;
const GROUND_PROBE_EPSILON: f32 = 2e-4;
const GROUND_VELOCITY_EPSILON: f32 = 1e-4;

fn has_ground_support(
    candidate_blocks: &[(Block<Global>, &BlockType, AxisAlignedBoundingBox<Global>)],
    position: Vec3fGlobal,
    bounding_box: BoxCollider,
) -> bool {
    let aabb = bounding_box.0.aabb(position);

    candidate_blocks
        .iter()
        .any(|(_candidate, _block_type, candidate_aabb)| {
            let x_overlap = aabb.max().x() > candidate_aabb.min().x() + MOVE_EPSILON
                && aabb.min().x() < candidate_aabb.max().x() - MOVE_EPSILON;
            let z_overlap = aabb.max().z() > candidate_aabb.min().z() + MOVE_EPSILON
                && aabb.min().z() < candidate_aabb.max().z() - MOVE_EPSILON;

            if !x_overlap || !z_overlap {
                return false;
            }

            let foot_gap = aabb.min().y() - candidate_aabb.max().y();
            (-MOVE_EPSILON..=GROUND_PROBE_EPSILON).contains(&foot_gap)
        })
}

/// Collision detection and resolution for a single entity, based on its `Collider` and `Velocity`.
pub fn apply_collision_aabb<CP: ChunkProvider>(
    position: Vec3fGlobal,
    bounding_box: BoxCollider,
    previous_status: CollisionStatus,
    velocity: Vec3fGlobal,
    chunks: &CP,
    resource_pack: &ResourcePack,
    dt: Duration,
) -> (Vec3fGlobal, Vec3fGlobal, CollisionStatus) {
    const MAX_COLLISION_ITERATIONS: usize = 6;
    const GROUND_NORMAL_THRESHOLD: f32 = 0.7;
    const STEP_HEIGHT_RATIO: f32 = 0.5;

    let dt_secs = dt.as_secs_f32();
    if dt_secs <= f32::EPSILON {
        return (position, velocity, previous_status);
    }

    let step_height = bounding_box.0.height() * STEP_HEIGHT_RATIO;

    let mut position = position;
    let mut velocity = velocity;
    let mut remaining_dt = dt_secs;
    let mut consumed_step = false;
    let mut resolved_normals = SmallVec::<[_; 8]>::new();

    let aabb_swept_full = bounding_box.0.aabb_swept(position, velocity, dt);
    let candidate_blocks: Vec<_> = chunks
        .intersecting(&aabb_swept_full)
        .filter_map(|block| {
            let block_type = resource_pack.get_block_type(block.id());
            if block_type.solidity().is_solid() {
                let aabb = block.aabb(block_type);
                Some((block, block_type, aabb))
            } else {
                None
            }
        })
        .collect();

    'iterations: for _ in 0..MAX_COLLISION_ITERATIONS {
        if remaining_dt < f32::EPSILON {
            break;
        }

        let current_aabb = bounding_box.0.aabb(position);
        let aabb_swept =
            bounding_box
                .0
                .aabb_swept(position, velocity, Duration::from_secs_f32(remaining_dt));

        let mut collisions = SmallVec::<[_; 3]>::new();

        for (_candidate, _block_type, candidate_aabb) in &candidate_blocks {
            if !aabb_swept.intersects_epsilon(candidate_aabb, FILTER_EPSILON) {
                continue;
            }

            if let Some(event) = narrow_phase_aabb(
                current_aabb,
                *candidate_aabb,
                velocity,
                Duration::from_secs_f32(remaining_dt),
            ) {
                collisions.push((event, candidate_aabb));
            }
        }

        if collisions.is_empty() {
            position += velocity * remaining_dt;
            break 'iterations;
        }

        collisions.sort_by(|(a, _), (b, _)| {
            a.toi.cmp(&b.toi).then(
                b.penetration
                    .partial_cmp(&a.penetration)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        let intersecting: SmallVec<[_; 4]> = collisions
            .iter()
            .filter(|(e, _)| e.toi.as_secs_f32() <= TOI_EPSILON && e.penetration > 0.0)
            .collect();

        if !intersecting.is_empty() {
            let mut handled_normals = SmallVec::<[_; 4]>::new();
            for (event, _) in &intersecting {
                if !handled_normals.contains(&event.normal) {
                    position += event.normal * (event.penetration + MOVE_EPSILON);
                    let dot = velocity.dot(event.normal);
                    if dot < 0.0 {
                        velocity -= event.normal * dot;
                    }
                    if !resolved_normals.contains(&event.normal) {
                        resolved_normals.push(event.normal);
                    }
                    handled_normals.push(event.normal);
                }
            }
            continue;
        }

        let first_toi = collisions[0].0.toi.as_secs_f32().max(0.0);
        if first_toi > 0.0 {
            position += velocity * first_toi;
            remaining_dt -= first_toi;
            remaining_dt = remaining_dt.max(0.0);
        }

        let simultaneous: SmallVec<[_; 4]> = collisions
            .iter()
            .filter(|(event, _)| (event.toi.as_secs_f32() - first_toi).abs() <= TOI_EPSILON)
            .collect();

        let mut stepped_up = consumed_step;
        let mut handled_normals = SmallVec::<[_; 4]>::new();

        for (event, candidate_aabb) in &simultaneous {
            if handled_normals.contains(&event.normal) {
                continue;
            }

            // Check if this is a horizontal collision we can step up
            let is_horizontal = event.normal.y().abs() < GROUND_NORMAL_THRESHOLD;
            if is_horizontal && previous_status == CollisionStatus::OnGround {
                let step_up_amount = candidate_aabb.max().y() - current_aabb.min().y();

                if step_up_amount > 0.0 && step_up_amount <= step_height {
                    position[1] += step_up_amount + MOVE_EPSILON;
                    stepped_up = true;
                    consumed_step = true;
                    continue;
                }
            }

            position += event.normal * MOVE_EPSILON;

            let dot = velocity.dot(event.normal);
            if dot < 0.0 {
                velocity -= event.normal * dot;
            }

            if !resolved_normals.contains(&event.normal) {
                resolved_normals.push(event.normal);
            }

            handled_normals.push(event.normal);
        }

        // If we stepped up, retry the iteration without consuming remaining_dt
        if stepped_up {
            continue;
        }
    }

    let grounded_by_collision = resolved_normals
        .iter()
        .any(|n| n.y() > GROUND_NORMAL_THRESHOLD);
    let grounded_by_support = velocity.y() <= GROUND_VELOCITY_EPSILON
        && has_ground_support(&candidate_blocks, position, bounding_box);

    let status = if grounded_by_collision || grounded_by_support {
        CollisionStatus::OnGround
    } else {
        CollisionStatus::Airborne
    };

    (position, velocity, status)
}

/// Resolves the movement of a single entity based on its `MovementIntent` and `Orientation`,
/// and returns the new `EntityCoordinate`.
pub fn apply_intent(
    position: Vec3fGlobal,
    orientation: Orientation,
    intent: &MovementIntent,
    move_speed: f32,
    mut velocity: Vec3fGlobal,
    dt: Duration,
) -> (Vec3fGlobal, Vec3fGlobal) {
    let dt_secs = dt.as_secs_f32();
    if dt_secs <= f32::EPSILON {
        return (position, velocity);
    }

    let speed_multiplier = if intent.sneak {
        MovementIntent::SNEAK_MODIFIER
    } else if intent.sprint {
        MovementIntent::SPRINT_MODIFIER
    } else {
        1.0
    };

    let movement_offset = orientation.movement_offset(
        move_speed * speed_multiplier,
        dt,
        intent.forward,
        intent.strafe,
        intent.fly,
    );

    velocity[0] = movement_offset.x() / dt_secs;
    velocity[2] = movement_offset.z() / dt_secs;

    if intent.fly {
        velocity[1] = movement_offset.y() / dt_secs;
    }

    (position, velocity)
}

/// Resolve the movement of all entities in the world that have `EntityCoordinate`,
/// `Orientation`, and `MovementIntent` components, and emit all entities that moved.
pub fn apply_intent_all(
    world: &mut World,
    stack: &mut HashMap<Entity, MoveBundle>,
    chunk_map: &ChunkMap,
    block_registry: &ResourcePack,
    broadcast_tick: bool,
    dt: Duration,
) {
    const BROADCAST_POS_TOLERANCE_SQ: f32 = 1e-2 * 1e-2;
    const BROADCAST_VEL_TOLERANCE_SQ: f32 = 1e-1 * 1e-1;

    stack.clear();

    let mut query = world.query::<(
        Entity,
        &EntityModel,
        &mut EntityPosition,
        &mut EntityVelocity,
        &EntityOrientation,
        &MovementIntent,
        &BoxCollider,
        &mut CollisionStatus,
    )>();

    for (
        entity,
        model,
        mut position,
        mut velocity,
        orientation,
        intent,
        collider,
        mut collision_status,
    ) in query.iter_mut(world)
    {
        // FIXME: this feels hacky
        let base_properties = ModelDefinition::from_handle(model.model_id).properties();

        let new_velocity = apply_gravity(
            velocity.0,
            base_properties.jump_velocity,
            intent,
            *collision_status,
            dt,
        );

        let (new_position, new_velocity) = apply_intent(
            position.0,
            orientation.0,
            intent,
            base_properties.move_speed,
            new_velocity,
            dt,
        );

        let (final_position, final_velocity, new_status) = apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            chunk_map,
            block_registry,
            dt,
        );

        // FIXME: would be nice to quantize on broadcast ticks
        let broadcast_worthy = (final_position - position.0).length_sq()
            > BROADCAST_POS_TOLERANCE_SQ
            || (final_velocity - velocity.0).length_sq() > BROADCAST_VEL_TOLERANCE_SQ
            || *collision_status != new_status;

        *collision_status = new_status;
        *position = EntityPosition(final_position);
        *velocity = EntityVelocity(final_velocity);

        if broadcast_worthy {
            // TODO: remote entities will need to emit their orientation too
            stack.insert(
                entity,
                MoveBundle::Motion {
                    position: *position,
                    velocity: *velocity,
                    collision: *collision_status,
                },
            );
        }
    }
}
