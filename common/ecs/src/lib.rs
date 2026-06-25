pub mod movement;

use bevy_ecs::{bundle::Bundle, component::Component};
use model::ModelDefinition;
use render::model::ModelHandle;
use serde::{Deserialize, Serialize};
use spatial::{
    orientation::Orientation,
    vectors::{Global, Vec3f, Vec4f},
};

pub fn eye_position(base_position: Vec3f<Global>, eye_height: f32) -> Vec3f<Global> {
    base_position + [0.0, eye_height, 0.0].into()
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MovementIntent {
    pub forward: f32,
    pub strafe: f32,

    pub jump: bool,
    pub fly: bool,
    pub sprint: bool,
    pub sneak: bool,
}

impl MovementIntent {
    pub const SNEAK_MODIFIER: f32 = 0.4;
    pub const SPRINT_MODIFIER: f32 = 1.3;
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct InteractionIntent {
    pub attack: bool,
    pub interact: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityModel {
    pub model_id: ModelHandle,
    pub eye_height: f32,
    pub animation_state: AnimationState,
    pub transform: EntityTransform,
}

impl EntityModel {
    pub fn for_model(model: ModelDefinition) -> Self {
        Self {
            model_id: model.handle(),
            eye_height: model.eye_height(),
            animation_state: AnimationState::Idle,
            transform: EntityTransform::default(),
        }
    }
}

#[derive(Default, Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityTransform {
    pub translation: Vec3f<Global>,
    pub rotation: Vec4f<Global>,
    pub scale: Vec3f<Global>,
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

pub type World = bevy_ecs::world::World;
pub type Entity = bevy_ecs::entity::Entity;

#[derive(Bundle, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimulatedEntityBundle {
    pub position: EntityPosition,
    pub orientation: EntityOrientation,
    pub velocity: EntityVelocity,
    pub movement_intent: MovementIntent,
    pub interaction_intent: InteractionIntent,
    pub collider: BoxCollider,
    pub model: EntityModel,
    pub collision_status: CollisionStatus,
}

impl SimulatedEntityBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        position: EntityPosition,
        orientation: EntityOrientation,
        velocity: EntityVelocity,
        movement_intent: MovementIntent,
        interaction_intent: InteractionIntent,
        collider: BoxCollider,
        model: EntityModel,
        collision_status: CollisionStatus,
    ) -> Self {
        Self {
            position,
            orientation,
            velocity,
            movement_intent,
            interaction_intent,
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
