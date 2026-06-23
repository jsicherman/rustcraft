use std::{cmp::Reverse, collections::HashMap, time::Duration};

use block::BlockRegistry;
use chunk::{ChunkMap, ChunkProvider};
use ordered_float::OrderedFloat;
use smallvec::SmallVec;
use spatial::{
    aabb::{Aabb, AxisAlignedBoundingBox},
    orientation::{Direction, Orientation},
    vectors::{Global, Vec3fGlobal},
};

use crate::{
    BoxCollider, CollisionStatus, Entity, EntityOrientation, EntityPosition, EntityVelocity,
    MovementIntent, World,
};

const MOVE_SPEED: f32 = 5.0;
const JUMP_VELOCITY: f32 = 6.3;
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
pub struct CollisionEvent {
    pub toi: Duration,
    pub normal: Vec3fGlobal,
    pub penetration: f32,
}

/// Apply gravity/jump to the given velocity, based on their `collision_status`
pub fn apply_gravity(
    mut velocity: Vec3fGlobal,
    intent: &MovementIntent,
    collision_status: CollisionStatus,
    dt: Duration,
) -> Vec3fGlobal {
    let is_grounded = collision_status == CollisionStatus::OnGround && velocity.y() <= 0.0;

    if intent.jump() && (is_grounded || intent.fly()) {
        velocity += [0.0, JUMP_VELOCITY, 0.0].into();
    } else if !is_grounded && !intent.fly() {
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
    if moving.intersects(&target) {
        let dx_left = target.max().x() - moving.min().x();
        let dx_right = moving.max().x() - target.min().x();
        let dy_top = target.max().y() - moving.min().y();
        let dy_bottom = moving.max().y() - target.min().y();
        let dz_front = target.max().z() - moving.min().z();
        let dz_back = moving.max().z() - target.min().z();

        let dx = dx_left.min(dx_right);
        let dy = dy_top.min(dy_bottom);
        let dz = dz_front.min(dz_back);

        let normal = if dx <= dy && dx <= dz {
            if dx_left < dx_right {
                Direction::PlusX
            } else {
                Direction::MinusX
            }
        } else if dy <= dz {
            if dy_top < dy_bottom {
                Direction::PlusY
            } else {
                Direction::MinusY
            }
        } else if dz_front < dz_back {
            Direction::PlusZ
        } else {
            Direction::MinusZ
        }
        .into();

        return Some(CollisionEvent {
            toi: Duration::ZERO,
            normal,
            penetration: dx.min(dy).min(dz),
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
            if !entry_dist.is_finite() && !exit_dist.is_finite() {
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
        .any(|(entry, exit)| !entry.is_finite() && !exit.is_finite())
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

    if entry_time > exit_time || entry_time < 0.0 || entry_time > dt_secs {
        return None;
    }

    let normal = if entries[0].0 > entries[1].0 && entries[0].0 > entries[2].0 {
        Vec3fGlobal::from(if velocity.x() > 0.0 {
            Direction::MinusX
        } else {
            Direction::PlusX
        })
    } else if entries[1].0 > entries[2].0 {
        Vec3fGlobal::from(if velocity.y() > 0.0 {
            Direction::MinusY
        } else {
            Direction::PlusY
        })
    } else {
        Vec3fGlobal::from(if velocity.z() > 0.0 {
            Direction::MinusZ
        } else {
            Direction::PlusZ
        })
    };

    Some(CollisionEvent {
        toi: Duration::from_secs_f32(entry_time.max(0.0)),
        normal,
        penetration: 0.0,
    })
}

const MOVE_TOLERANCE: f32 = 1e-4;

/// Collision detection and resolution for a single entity, based on its `Collider` and `Velocity`.
pub fn apply_collision_aabb<CP: ChunkProvider>(
    position: Vec3fGlobal,
    bounding_box: BoxCollider,
    previous_status: CollisionStatus,
    velocity: Vec3fGlobal,
    chunks: &CP,
    block_registry: &BlockRegistry,
    dt: Duration,
) -> (Vec3fGlobal, Vec3fGlobal, CollisionStatus) {
    const MAX_COLLISION_ITERATIONS: usize = 5;
    const GROUND_NORMAL_THRESHOLD: f32 = 0.7;

    let dt_secs = dt.as_secs_f32();
    if dt_secs <= f32::EPSILON {
        return (position, velocity, previous_status);
    }

    let mut position = position;
    let mut velocity = velocity;
    let mut remaining_dt = dt_secs;
    let mut resolved_normals = SmallVec::<[Vec3fGlobal; 8]>::new();

    'iterations: for _ in 0..MAX_COLLISION_ITERATIONS {
        if remaining_dt < f32::EPSILON {
            break;
        }

        let current_aabb = bounding_box.0.aabb(position);
        let aabb_swept =
            bounding_box
                .0
                .aabb_swept(position, velocity, Duration::from_secs_f32(remaining_dt));

        let mut collisions = SmallVec::<[CollisionEvent; 3]>::new();

        for candidate in chunks.intersecting(&aabb_swept) {
            if !block_registry
                .get_block_type(candidate.id())
                .solidity()
                .is_solid()
            {
                continue;
            }

            let candidate_aabb = candidate.aabb();
            if !aabb_swept.intersects_epsilon(&candidate_aabb, -MOVE_TOLERANCE) {
                continue;
            }

            if let Some(event) = narrow_phase_aabb(
                current_aabb,
                candidate_aabb,
                velocity,
                Duration::from_secs_f32(remaining_dt),
            ) {
                collisions.push(event);
            }
        }

        if collisions.is_empty() {
            position += velocity * remaining_dt;
            break 'iterations;
        }

        collisions.sort_by_key(|e| (e.toi, Reverse(OrderedFloat(e.penetration))));
        let event = collisions[0];

        let is_already_intersecting =
            event.toi == Duration::ZERO && event.penetration > MOVE_TOLERANCE;

        if is_already_intersecting {
            position += event.normal * event.penetration;
        } else {
            let toi_secs = event.toi.as_secs_f32();
            position += velocity * (toi_secs - MOVE_TOLERANCE).max(0.0);
            remaining_dt -= toi_secs;
        }

        if !resolved_normals.contains(&event.normal) {
            let dot = velocity.dot(event.normal);
            if dot < 0.0 {
                velocity -= event.normal * dot;
            }
            resolved_normals.push(event.normal);
        }
    }

    let status = if resolved_normals
        .iter()
        .any(|n| n.y() > GROUND_NORMAL_THRESHOLD)
    {
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
    mut velocity: Vec3fGlobal,
    dt: Duration,
) -> (Vec3fGlobal, Vec3fGlobal) {
    let dt_secs = dt.as_secs_f32();
    if dt_secs <= f32::EPSILON {
        return (position, velocity);
    }

    let movement_offset = orientation.movement_offset(
        MOVE_SPEED,
        dt,
        intent.forward(),
        intent.strafe(),
        intent.fly(),
    );

    velocity[0] = movement_offset.x() / dt_secs;
    velocity[2] = movement_offset.z() / dt_secs;

    if intent.fly() {
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
    block_registry: &BlockRegistry,
    dt: Duration,
) {
    const BROADCAST_POS_TOLERANCE_SQ: f32 = 1e-2 * 1e-2;

    stack.clear();

    let mut query = world.query::<(
        Entity,
        &mut EntityPosition,
        &mut EntityVelocity,
        &EntityOrientation,
        &MovementIntent,
        &BoxCollider,
        &mut CollisionStatus,
    )>();

    for (entity, mut position, mut velocity, orientation, intent, collider, mut collision_status) in
        query.iter_mut(world)
    {
        let new_velocity = apply_gravity(velocity.0, intent, *collision_status, dt);

        let (new_position, new_velocity) =
            apply_intent(position.0, orientation.0, intent, new_velocity, dt);

        let (final_position, final_velocity, new_status) = apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            chunk_map,
            block_registry,
            dt,
        );

        let broadcast_worthy =
            (final_position - position.0).length_sq() > BROADCAST_POS_TOLERANCE_SQ;

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
