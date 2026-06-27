use chunk::{ChunkProvider, block_entity::BlockEntityUpdate};
use ecs::{
    BoxCollider, CollisionStatus, EntityModel, EntityOrientation, EntityPosition, EntityVelocity,
    InteractionIntent, MovementIntent, ai::NpcController,
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use spatial::vectors::Vec3iGlobal;

use crate::NetworkId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EntityMessage {
    /// Emitted to the client when it connects
    ClientConnect {
        entity_id: NetworkId,
        position: EntityPosition,
        bounding_box: BoxCollider,
        model: EntityModel,
    },
    /// Emitted to all nearby clients when any entity spawns
    Spawn {
        entity_id: NetworkId,
        position: EntityPosition,
        bounding_box: BoxCollider,
        model: EntityModel,
    },
    /// Emitted to all nearby clients when any entity despawns
    Despawn(NetworkId),
    /// Emitted to all nearby clients when any entity moves
    Move {
        entity_id: NetworkId,
        position: EntityPosition,
        velocity: EntityVelocity,
        collision_status: CollisionStatus,
    },
    /// Emitted to all nearby clients when any entity updates its orientation
    Look {
        entity_id: NetworkId,
        orientation: EntityOrientation,
    },
    /// Emitted to all nearby clients when any entity model changes
    Remodel {
        entity_id: NetworkId,
        model: EntityModel,
        bounding_box: BoxCollider,
    },
    /// Emitted to all observing clients when a BlockEntity updates
    BlockEntityUpdate {
        position: Vec3iGlobal,
        data: BlockEntityUpdate,
    },
    GuidedMove {
        entity_id: NetworkId,
        movement: MovementIntent,
    },
    GuidedLook {
        entity_id: NetworkId,
        orientation: EntityOrientation,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Move(MovementIntent),
    Look(EntityOrientation),
    InteractBlock {
        intent: InteractionIntent,
        targeted_block: Option<(Vec3iGlobal, Vec3iGlobal)>,
    },
    RemodelEntity {
        model: EntityModel,
        bounding_box: BoxCollider,
    },
}

pub fn pathfinding_tick(
    entity_id: NetworkId,
    controller: &mut NpcController,
    position: EntityPosition,
    orientation: EntityOrientation,
    tick: u64,
    world: &impl ChunkProvider,
) -> SmallVec<[EntityMessage; 2]> {
    if tick >= controller.path_state.next_replan_tick {
        controller.replan(position, world, tick);
    }

    controller.advance_waypoints(position);

    controller.steering = controller.compute_steering(position);

    let (movement, look) = controller.steering.to_intents(orientation);
    let mut msgs = SmallVec::new();

    msgs.push(EntityMessage::GuidedMove {
        entity_id,
        movement,
    });

    if let Some(orientation) = look {
        msgs.push(EntityMessage::GuidedLook {
            entity_id,
            orientation,
        });
    }

    msgs
}
