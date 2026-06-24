pub mod movement;

use bevy_ecs::{bundle::Bundle, component::Component};
use model::ModelDefinition;
use render::model::ModelHandle;
use serde::{Deserialize, Serialize};
use spatial::{
    orientation::Orientation,
    vectors::{Global, Vec3f, Vec4f},
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MovementIntent {
    forward: f32,
    strafe: f32,

    jump: bool,
    fly: bool,
    sprint: bool,
    sneak: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityModel {
    model_id: ModelHandle,
    animation_state: AnimationState,
    transform: EntityTransform,
}

impl EntityModel {
    pub fn for_model(model: ModelDefinition) -> Self {
        Self {
            model_id: model.handle(),
            animation_state: AnimationState::Idle,
            transform: EntityTransform::default(),
        }
    }

    pub fn model(&self) -> ModelHandle {
        self.model_id
    }

    pub fn animation_state(&self) -> AnimationState {
        self.animation_state
    }

    pub fn transform(&self) -> EntityTransform {
        self.transform
    }
}

#[derive(Default, Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityTransform {
    translation: Vec3f<Global>,
    rotation: Vec4f<Global>,
    scale: Vec3f<Global>,
}

#[derive(Default, Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationState {
    #[default]
    Idle,
    Walking,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxCollider(pub spatial::aabb::BoxCollider);

#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollisionStatus {
    #[default]
    Airborne,
    OnGround,
    InLiquid,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityOrientation(pub Orientation);

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityPosition(pub Vec3f<Global>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityVelocity(pub Vec3f<Global>);

impl MovementIntent {
    pub fn new(
        forward: f32,
        strafe: f32,
        jump: bool,
        fly: bool,
        sprint: bool,
        sneak: bool,
    ) -> Self {
        Self {
            forward,
            strafe,
            jump,
            fly,
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
    pub fn fly(&self) -> bool {
        self.fly
    }
}

pub type World = bevy_ecs::world::World;
pub type Entity = bevy_ecs::entity::Entity;

#[derive(Bundle, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimulatedEntityBundle {
    pub position: EntityPosition,
    pub orientation: EntityOrientation,
    pub velocity: EntityVelocity,
    pub movement_intent: MovementIntent,
    pub collider: BoxCollider,
    pub model: EntityModel,
    pub collision_status: CollisionStatus,
}

impl SimulatedEntityBundle {
    pub fn new(
        position: EntityPosition,
        orientation: EntityOrientation,
        velocity: EntityVelocity,
        movement_intent: MovementIntent,
        collider: BoxCollider,
        model: EntityModel,
        collision_status: CollisionStatus,
    ) -> Self {
        Self {
            position,
            orientation,
            velocity,
            movement_intent,
            collider,
            model,
            collision_status,
        }
    }
}

#[derive(Component)]
pub struct RemoteControlled;
#[derive(Component)]
pub struct AiControlled;
