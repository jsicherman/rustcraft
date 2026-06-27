use chunk::WireChunk;
use resources::block::BlockId;
use serde::{Deserialize, Serialize};
use spatial::vectors::Vec3iGlobal;
use world::TimeOfDay;

use crate::particles::ParticleEmitter;

#[derive(Debug, Serialize, Deserialize)]
pub enum WorldMessage {
    ServerTime(TimeOfDay),

    ChunkData(WireChunk),
    BlockModification {
        position: Vec3iGlobal,
        before: BlockId,
        after: BlockId,
    },

    ParticleSpawn {
        emitter: ParticleEmitter,
    },
}
