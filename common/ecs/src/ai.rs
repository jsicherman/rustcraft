use std::collections::VecDeque;

use bevy_ecs::component::Component;
use chunk::ChunkProvider;
use serde::{Deserialize, Serialize};
use spatial::vectors::Vec3fGlobal;

use crate::{EntityOrientation, EntityPosition, MovementIntent};

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub kind: GoalKind,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GoalKind {
    #[default]
    Idle,
    MoveTo(EntityPosition),
    Follow {
        entity: u32,
        min_distance: f32,
    },
    InteractWith {
        entity: u32,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathState {
    pub goal: Goal,
    pub waypoints: VecDeque<Vec3fGlobal>,
    pub next_replan_tick: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SteeringOutput {
    pub desired_velocity: Vec3fGlobal,
    pub target_look: Option<Vec3fGlobal>,
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
}

#[derive(Component, Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcController {
    pub path_state: PathState,
    pub steering: SteeringOutput,
}

impl SteeringOutput {
    pub fn to_intents(
        &self,
        current_orientation: EntityOrientation,
    ) -> (MovementIntent, Option<EntityOrientation>) {
        let look_dir = current_orientation.0.look_direction();
        let right = look_dir.cross(Vec3fGlobal::UP).normalize();

        let forward = self.desired_velocity.dot(look_dir);
        let strafe = self.desired_velocity.dot(right);

        let movement = MovementIntent {
            forward,
            strafe,
            jump: self.jump,
            sprint: self.sprint,
            sneak: self.sneak,
            fly: false,
        };

        /*TODOlet look = self
        .target_look
        .map(|target| EntityOrientation::looking_at(target));*/
        let look = None;

        (movement, look)
    }
}

impl NpcController {
    pub fn replan(&mut self, position: EntityPosition, world: &impl ChunkProvider, tick: u64) {}

    pub fn advance_waypoints(&mut self, position: EntityPosition) {
        // 0.5 blocks
        const WAYPOINT_REACHED_DISTANCE_SQ: f32 = 0.25;

        while let Some(next_waypoint) = self.path_state.waypoints.front() {
            let distance = (*next_waypoint - position.0).length_sq();
            if distance < WAYPOINT_REACHED_DISTANCE_SQ {
                self.path_state.waypoints.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn compute_steering(&self, position: EntityPosition) -> SteeringOutput {
        if let Some(next_waypoint) = self.path_state.waypoints.front() {
            let desired_velocity = (*next_waypoint - position.0).normalize() * 1.0; // TODO: max speed

            SteeringOutput {
                desired_velocity,
                target_look: Some(*next_waypoint),
                jump: false,
                sprint: false,
                sneak: false,
            }
        } else {
            SteeringOutput {
                desired_velocity: Vec3fGlobal::ZERO,
                target_look: None,
                jump: false,
                sprint: false,
                sneak: false,
            }
        }
    }
}
