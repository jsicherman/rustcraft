pub mod movement;

use bevy_ecs::{bundle::Bundle, component::Component};
use serde::{Deserialize, Serialize};
use spatial::vectors::{CoordSpace, Global};

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MovementIntent {
    forward: f32,
    strafe: f32,

    jump: bool,
    sprint: bool,
    sneak: bool,
}

#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxCollider(spatial::aabb::BoxCollider);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollisionStatus {
    Airborne,
    OnGround,
    InLiquid,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Orientation(pub spatial::orientation::Orientation);

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityPosition(pub spatial::vectors::Vec3f<Global>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityVelocity(pub spatial::vectors::Vec3f<Global>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct V2f<S: CoordSpace>(pub spatial::vectors::Vec2f<S>);

impl MovementIntent {
    pub fn new(forward: f32, strafe: f32, jump: bool, sprint: bool, sneak: bool) -> Self {
        Self {
            forward,
            strafe,
            jump,
            sprint,
            sneak,
        }
    }

    pub fn forward(&self) -> f32 {
        self.forward
    }

    pub fn strafe(&self) -> f32 {
        self.strafe
    }

    pub fn jump(&self) -> bool {
        self.jump
    }

    pub fn sprint(&self) -> bool {
        self.sprint
    }

    pub fn sneak(&self) -> bool {
        self.sneak
    }
}

pub type World = bevy_ecs::world::World;
pub type Entity = bevy_ecs::entity::Entity;

#[derive(Bundle, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimulatedEntityBundle {
    pub position: EntityPosition,
    pub orientation: Orientation,
    pub velocity: EntityVelocity,
    pub movement_intent: MovementIntent,
    pub collider: BoxCollider,
    pub collision_status: CollisionStatus,
}

impl Default for SimulatedEntityBundle {
    fn default() -> Self {
        Self {
            position: EntityPosition([0.0, 90.0, 0.0].into()),
            orientation: Orientation::default(),
            movement_intent: MovementIntent::default(),
            velocity: EntityVelocity::default(),
            collider: BoxCollider::default(),
            collision_status: CollisionStatus::Airborne,
        }
    }
}

#[derive(Component)]
pub struct LocalPlayer;
#[derive(Component)]
pub struct RemoteControlled;
#[derive(Component)]
pub struct AiControlled;
