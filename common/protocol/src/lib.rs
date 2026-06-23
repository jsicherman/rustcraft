use anyhow::Error;
use chunk::Chunk;
use ecs::{CollisionStatus, EntityOrientation, EntityPosition, EntityVelocity, MovementIntent};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const PROTOCOL_ID: u64 = 0xABCDEF;
pub const CHANNEL_CHUNKS: u8 = 0;
pub const CHANNEL_ENTITIES: u8 = 1;

pub const RENDER_DISTANCE: i32 = 4;
pub const RENDER_DISTANCE_SQ: i32 = RENDER_DISTANCE * RENDER_DISTANCE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkId(pub u64);

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
    ChunkData(Box<Chunk>),
    EntitySpawn {
        entity_id: NetworkId,
        position: EntityPosition,
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
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Move(MovementIntent),
    Look(EntityOrientation),
}

impl Packet for ServerMessage {}
impl Packet for ClientMessage {}
