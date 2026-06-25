use anyhow::Error;
use bevy_ecs::{bundle::Bundle, world::EntityWorldMut};
use block::BlockId;
use chunk::{Chunk, ChunkMap, ChunkProvider, ChunkScratch, ChunkStore, WireChunk};
use ecs::{BoxCollider, Entity, EntityModel, EntityPosition, World};
use noise::{Fbm, NoiseFn, Perlin};
use protocol::{CHANNEL_ENTITIES, NetworkId, Packet, ServerMessage};
use renet::RenetServer;
use serde::Deserialize;
use spatial::{
    SEA_LEVEL, WORLD_HEIGHT,
    aabb::Aabb,
    vectors::{Vec2iChunk, Vec3fGlobal},
};

pub struct GameWorld {
    world: World,
    generator: WorldGeneration,
}

pub enum WorldGeneration {
    Default(Box<DefaultWorldGenerator>),
    Flat(FlatWorldGenerator),
}

#[derive(Default, Deserialize, Clone, Copy)]
pub enum WorldGeneratorType {
    #[default]
    Default,
    Flat,
}

impl WorldGeneration {
    pub fn new(generator_type: WorldGeneratorType, seed: u32) -> Self {
        match generator_type {
            WorldGeneratorType::Default => {
                Self::Default(Box::new(DefaultWorldGenerator::new(seed)))
            }
            WorldGeneratorType::Flat => Self::Flat(FlatWorldGenerator::new(seed)),
        }
    }

    pub fn seed(&self) -> u32 {
        match self {
            WorldGeneration::Default(generator) => generator.seed(),
            WorldGeneration::Flat(generator) => generator.seed(),
        }
    }

    pub fn generate(&self, chunk_store: &mut ChunkStore, coordinate: Vec2iChunk) -> Chunk {
        match self {
            WorldGeneration::Default(generator) => generator.generate(chunk_store, coordinate),
            WorldGeneration::Flat(generator) => generator.generate(chunk_store, coordinate),
        }
    }
}

impl GameWorld {
    pub fn new(generator: WorldGeneration) -> Self {
        Self {
            generator,
            world: World::new(),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn spawn<B: Bundle>(
        &mut self,
        server: &mut RenetServer,
        observers: impl Iterator<Item = NetworkId>,
        entity_id: NetworkId,
        bundle: B,
    ) -> Result<(EntityWorldMut<'_>, EntityPosition), Error> {
        let entity = self.world_mut().spawn(bundle);

        let (&position, &bounding_box, &model) =
            entity.get_components::<(&EntityPosition, &BoxCollider, &EntityModel)>()?;

        let msg = ServerMessage::EntitySpawn {
            entity_id,
            position,
            bounding_box,
            model,
        }
        .encode()?;

        let mut observed = 0;
        for observer in observers {
            observed += 1;
            server.send_message(*observer, CHANNEL_ENTITIES, msg.clone());
        }

        tracing::debug!("Spawn: {entity_id:?} {model:?} ({observed} observers)");

        Ok((entity, position))
    }
    pub fn despawn(
        &mut self,
        server: &mut RenetServer,
        observers: impl Iterator<Item = NetworkId>,
        entity_id: NetworkId,
        entity: Entity,
    ) -> bool {
        if !self.world_mut().despawn(entity) {
            return false;
        }

        let msg = ServerMessage::EntityDespawn(entity_id).encode().unwrap();

        let mut observed = 0;
        for observer in observers {
            observed += 1;
            server.send_message(*observer, CHANNEL_ENTITIES, msg.clone());
        }

        tracing::debug!("Despawn: {entity_id:?} ({observed} observers)");

        true
    }

    pub fn get_block(&self, chunk_map: &ChunkMap, world_position: Vec3fGlobal) -> Option<BlockId> {
        chunk_map.block(world_position).map(|block| block.id())
    }

    pub fn set_block(
        &self,
        chunk_map: &mut ChunkMap,
        world_position: Vec3fGlobal,
        block_id: BlockId,
    ) -> Option<()> {
        chunk_map.set_block(world_position, block_id)
    }

    pub fn generate(
        &mut self,
        chunk_map: &mut ChunkMap,
        coordinate: Vec2iChunk,
    ) -> Result<Option<WireChunk>, Error> {
        chunk_map.get_or_generate_chunk(coordinate, |store, coord| {
            self.generator.generate(store, coord)
        })
    }

    /// Unloads chunks that are farther than `max_distance` from all given player positions.
    ///
    /// Returns the number of unloaded chunks.
    pub fn unload_distant_chunks(
        &mut self,
        chunk_map: &mut ChunkMap,
        player_positions: &[Vec2iChunk],
        max_distance: i32,
    ) -> usize {
        chunk_map.unload_distant_chunks(player_positions, max_distance)
    }
}

pub trait WorldGenerator: Send + Sync {
    fn new(seed: u32) -> Self;
    fn seed(&self) -> u32;
    fn generate(&self, chunk_store: &mut ChunkStore, coordinate: Vec2iChunk) -> Chunk;
}

pub struct FlatWorldGenerator(u32);

impl WorldGenerator for FlatWorldGenerator {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn seed(&self) -> u32 {
        self.0
    }

    fn generate(&self, chunk_store: &mut ChunkStore, coordinate: Vec2iChunk) -> Chunk {
        let mut chunk = ChunkScratch::new(coordinate);

        for slice_idx in 0..(WORLD_HEIGHT / Chunk::CHUNK_SIZE) {
            let slice_y_start = (slice_idx * Chunk::CHUNK_SIZE) as i32;
            let slice_y_end = slice_y_start + Chunk::CHUNK_SIZE as i32 - 1;

            if slice_y_end < SEA_LEVEL as i32 - 3 {
                chunk.fill(slice_idx, BlockId::STONE);
            } else if slice_y_start > SEA_LEVEL as i32 {
                chunk.fill(slice_idx, BlockId::AIR);
            } else {
                let slice = chunk.slice_mut(slice_idx);
                slice.set_many(
                    (0..Chunk::CHUNK_SIZE).flat_map(|local_x| {
                        (0..Chunk::CHUNK_SIZE).flat_map(move |local_z| {
                            (0..Chunk::CHUNK_SIZE).map(move |local_y| {
                                let world_y = slice_y_start + local_y as i32;

                                let id = if world_y <= SEA_LEVEL as i32 - 3 {
                                    BlockId::STONE
                                } else if world_y < SEA_LEVEL as i32 {
                                    BlockId::DIRT
                                } else if world_y == SEA_LEVEL as i32 {
                                    BlockId::GRASS
                                } else {
                                    BlockId::AIR
                                };

                                ([local_x as i32, local_y as i32, local_z as i32].into(), id)
                            })
                        })
                    }),
                    false,
                );
            }
        }

        chunk.to_chunk(chunk_store)
    }
}

pub struct DefaultWorldGenerator {
    seed: u32,
    continental_noise: Fbm<Perlin>,
    hill_noise: Fbm<Perlin>,
    detail_noise: Fbm<Perlin>,
}

impl WorldGenerator for DefaultWorldGenerator {
    fn new(seed: u32) -> Self {
        Self {
            seed,
            continental_noise: Fbm::new(seed),
            hill_noise: Fbm::new(seed.wrapping_add(1)),
            detail_noise: Fbm::new(seed.wrapping_add(2)),
        }
    }

    fn seed(&self) -> u32 {
        self.seed
    }

    fn generate(&self, chunk_store: &mut ChunkStore, coordinate: Vec2iChunk) -> Chunk {
        const N_SLICES: usize = WORLD_HEIGHT / Chunk::CHUNK_SIZE;

        let aabb = coordinate.aabb(Vec3fGlobal::ZERO);
        let (min, _max) = (aabb.min(), aabb.max());

        let mut surface_heights = [[0; Chunk::CHUNK_SIZE]; Chunk::CHUNK_SIZE];
        for (local_x, values) in surface_heights
            .iter_mut()
            .enumerate()
            .take(Chunk::CHUNK_SIZE)
        {
            for (local_z, surface_y) in values.iter_mut().enumerate().take(Chunk::CHUNK_SIZE) {
                let world_x = min.x() as f64 + local_x as f64;
                let world_z = min.z() as f64 + local_z as f64;

                let continental = self
                    .continental_noise
                    .get([world_x * 0.0035, world_z * 0.0035]);
                let hill_shape = self.hill_noise.get([world_x * 0.011, world_z * 0.011]);
                let detail = self.detail_noise.get([world_x * 0.028, world_z * 0.028]);

                let rounded_hills = 1.0 - hill_shape.abs();

                let height =
                    SEA_LEVEL as f64 + continental * 26.0 + rounded_hills * 11.0 + detail * 2.0;

                *surface_y = height.clamp(0.0, (WORLD_HEIGHT - 1) as f64) as i32;
            }
        }

        let min_surface = surface_heights
            .iter()
            .flatten()
            .copied()
            .min()
            .unwrap_or_default();
        let max_surface = surface_heights
            .iter()
            .flatten()
            .copied()
            .max()
            .unwrap_or_default();

        let mut chunk = ChunkScratch::new(coordinate);

        for slice_idx in 0..N_SLICES {
            let slice_y_start = (slice_idx * Chunk::CHUNK_SIZE) as i32;
            let slice_y_end = slice_y_start + Chunk::CHUNK_SIZE as i32 - 1;

            if slice_y_end < min_surface - 3 {
                chunk.fill(slice_idx, BlockId::STONE);
                continue;
            }

            if slice_y_start > max_surface {
                continue;
            }

            let slice = chunk.slice_mut(slice_idx);
            slice.set_many(
                (0..Chunk::CHUNK_SIZE).flat_map(|local_x| {
                    (0..Chunk::CHUNK_SIZE).flat_map(move |local_z| {
                        let surface_y = surface_heights[local_x][local_z];
                        let stone_end = (surface_y - 3).max(0);

                        (0..Chunk::CHUNK_SIZE).map(move |local_y| {
                            let world_y = slice_y_start + local_y as i32;

                            let id = if world_y <= stone_end {
                                BlockId::STONE
                            } else if world_y < surface_y {
                                BlockId::DIRT
                            } else if world_y == surface_y {
                                BlockId::GRASS
                            } else {
                                BlockId::AIR
                            };

                            ([local_x as i32, local_y as i32, local_z as i32].into(), id)
                        })
                    })
                }),
                false,
            );
        }

        chunk.to_chunk(chunk_store)
    }
}
