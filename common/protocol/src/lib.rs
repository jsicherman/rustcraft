use std::ops::Deref;

use anyhow::Error;
use block::BlockId;
use chunk::WireChunk;
use ecs::{
    BoxCollider, CollisionStatus, EntityModel, EntityOrientation, EntityPosition, EntityVelocity,
    InteractionIntent, MovementIntent,
};
pub use physics::ParticleEmitter;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use spatial::vectors::Vec3iGlobal;
use world::TimeOfDay;

pub const PROTOCOL_ID: u64 = 0xABCDEF;
pub const CHANNEL_CHUNKS: u8 = 0;
pub const CHANNEL_ENTITIES: u8 = 1;

pub const RENDER_DISTANCE: i32 = 8;
pub const RENDER_DISTANCE_SQ: i32 = RENDER_DISTANCE * RENDER_DISTANCE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkId(pub u64);

impl Deref for NetworkId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait Packet: Serialize + DeserializeOwned {
    fn encode(self) -> Result<Vec<u8>, Error> {
        let serialized = bincode::serde::encode_to_vec(self, bincode::config::standard())?;
        let compressed = zstd::encode_all(serialized.as_slice(), 0)?;

        Ok(compressed)
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let decompressed = zstd::decode_all(bytes)?;

        let (value, _) =
            bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())?;

        Ok(value)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    ClientSpawned(NetworkId),

    ServerTime(TimeOfDay),

    ChunkData(WireChunk),
    BlockEdit {
        position: Vec3iGlobal,
        before: BlockId,
        after: BlockId,
    },
    ParticleSpawn {
        emitter: ParticleEmitter,
    },

    EntitySpawn {
        entity_id: NetworkId,
        position: EntityPosition,
        bounding_box: BoxCollider,
        model: EntityModel,
    },
    EntityDespawn(NetworkId),

    EntityMove {
        entity_id: NetworkId,
        position: EntityPosition,
        velocity: EntityVelocity,
        collision_status: CollisionStatus,
    },
    EntityLook {
        entity_id: NetworkId,
        orientation: EntityOrientation,
    },
    EntityRemodel {
        entity_id: NetworkId,
        model: EntityModel,
        bounding_box: BoxCollider,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    EntityMove(MovementIntent),
    EntityLook(EntityOrientation),
    BlockInteract {
        intent: InteractionIntent,
        targeted_block: Option<(Vec3iGlobal, Vec3iGlobal)>,
    },
    EntityRemodel {
        model: EntityModel,
        bounding_box: BoxCollider,
    },
}

impl Packet for ServerMessage {
    fn encode(self) -> Result<Vec<u8>, Error> {
        let serialized = match self {
            Self::ChunkData(wire) => wire.into_bytes(),
            other => bincode::serde::encode_to_vec(other, bincode::config::standard())?,
        };

        let compressed = zstd::encode_all(serialized.as_slice(), 0)?;

        Ok(compressed)
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let decompressed = zstd::decode_all(bytes)?;

        if let Ok(wire_chunk) = WireChunk::from_bytes(&decompressed) {
            Ok(ServerMessage::ChunkData(wire_chunk))
        } else {
            let (value, _) =
                bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())?;

            Ok(value)
        }
    }
}
impl Packet for ClientMessage {}
