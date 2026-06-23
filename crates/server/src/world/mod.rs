use anyhow::Error;
use bevy_ecs::{bundle::Bundle, world::EntityWorldMut};
use block::BlockId;
use chunk::{Chunk, ChunkMap};
use ecs::{Entity, EntityPosition, World};
use noise::{
    Fbm, Perlin,
    utils::{NoiseMapBuilder, PlaneMapBuilder},
};
use protocol::{CHANNEL_ENTITIES, NetworkId, Packet, ServerMessage};
use renet::RenetServer;
use spatial::{
    SEA_LEVEL, WORLD_HEIGHT,
    aabb::Aabb,
    vectors::{Vec2iChunk, Vec3fGlobal},
};

pub struct GameWorld<G: WorldGenerator> {
    world: World,
    generator: G,
}

impl<G: WorldGenerator> GameWorld<G> {
    pub fn new(generator: G) -> Self {
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
        observers: impl Iterator<Item = u64>,
        entity_id: NetworkId,
        bundle: B,
    ) -> Result<EntityWorldMut<'_>, Error> {
        let entity = self.world_mut().spawn(bundle);
        let Some(position) = entity.get::<EntityPosition>() else {
            anyhow::bail!("entity needs EntityPosition");
        };

        let msg = ServerMessage::EntitySpawn {
            entity_id,
            position: *position,
        }
        .encode()?;

        tracing::debug!("spawning entity {:?} at {:?}", entity_id, position);
        for observer in observers {
            tracing::debug!("outbound to {observer}");
            server.send_message(observer, CHANNEL_ENTITIES, msg.clone());
        }

        Ok(entity)
    }
    pub fn despawn(
        &mut self,
        server: &mut RenetServer,
        observers: impl Iterator<Item = u64>,
        entity_id: NetworkId,
        entity: Entity,
    ) -> bool {
        if !self.world_mut().despawn(entity) {
            return false;
        }

        let msg = ServerMessage::EntityDespawn(entity_id).encode().unwrap();
        for observer in observers {
            server.send_message(observer, CHANNEL_ENTITIES, msg.clone());
        }

        true
    }

    pub fn generate<'a>(
        &mut self,
        chunk_map: &'a mut ChunkMap,
        coordinate: Vec2iChunk,
    ) -> &'a Chunk {
        if !chunk_map.contains_chunk(coordinate) {
            let chunk = self.generator.generate(coordinate);
            chunk_map.insert_chunk(chunk);
        }
        chunk_map.chunk(coordinate).unwrap()
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
    fn generate(&self, coordinate: Vec2iChunk) -> Chunk;
}

pub struct FlatWorldGenerator(u32);

impl WorldGenerator for FlatWorldGenerator {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn seed(&self) -> u32 {
        self.0
    }

    fn generate(&self, coordinate: Vec2iChunk) -> Chunk {
        let mut chunk = Chunk::new(coordinate);

        for slice_idx in 0..(WORLD_HEIGHT / Chunk::CHUNK_SIZE) {
            let slice_y_start = (slice_idx * Chunk::CHUNK_SIZE) as i32;
            let slice_y_end = slice_y_start + Chunk::CHUNK_SIZE as i32 - 1;

            if slice_y_end < SEA_LEVEL as i32 - 3 {
                chunk.fill(slice_idx, BlockId::Stone);
            } else if slice_y_start > SEA_LEVEL as i32 {
                chunk.fill(slice_idx, BlockId::Air);
            } else {
                let slice = chunk.slice_mut(slice_idx);
                slice.promote(BlockId::Air);

                for local_x in 0..Chunk::CHUNK_SIZE {
                    for local_z in 0..Chunk::CHUNK_SIZE {
                        for local_y in 0..Chunk::CHUNK_SIZE {
                            let world_y = slice_y_start + local_y as i32;

                            let id = if world_y <= SEA_LEVEL as i32 - 3 {
                                BlockId::Stone
                            } else if world_y < SEA_LEVEL as i32 {
                                BlockId::Dirt
                            } else if world_y == SEA_LEVEL as i32 {
                                BlockId::Grass
                            } else {
                                BlockId::Air
                            };

                            slice.set([local_x as i32, local_y as i32, local_z as i32].into(), id);
                        }
                    }
                }
            }
        }

        chunk
    }
}

pub struct DefaultWorldGenerator {
    seed: u32,
    height_noise: Fbm<Perlin>,
}

impl WorldGenerator for DefaultWorldGenerator {
    fn new(seed: u32) -> Self {
        Self {
            seed,
            height_noise: Fbm::new(seed),
        }
    }

    fn seed(&self) -> u32 {
        self.seed
    }

    fn generate(&self, coordinate: Vec2iChunk) -> Chunk {
        const N_SLICES: usize = WORLD_HEIGHT / Chunk::CHUNK_SIZE;

        let aabb = coordinate.aabb(Vec3fGlobal::ZERO);
        let (min, max) = (aabb.min(), aabb.max());

        let height_map = PlaneMapBuilder::new(&self.height_noise)
            .set_size(Chunk::CHUNK_SIZE, Chunk::CHUNK_SIZE)
            .set_x_bounds(min.x() as f64, max.x() as f64)
            .set_y_bounds(min.z() as f64, max.z() as f64)
            .build();

        let mut surface_heights = [[0; Chunk::CHUNK_SIZE]; Chunk::CHUNK_SIZE];
        for (local_x, values) in surface_heights
            .iter_mut()
            .enumerate()
            .take(Chunk::CHUNK_SIZE)
        {
            for (local_z, surface_y) in values.iter_mut().enumerate().take(Chunk::CHUNK_SIZE) {
                *surface_y = (height_map.get_value(local_x, local_z) * 16.0 + SEA_LEVEL as f64)
                    .clamp(0.0, (WORLD_HEIGHT - 1) as f64) as i32;
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

        let mut chunk = Chunk::new(coordinate);

        for slice_idx in 0..N_SLICES {
            let slice_y_start = (slice_idx * Chunk::CHUNK_SIZE) as i32;
            let slice_y_end = slice_y_start + Chunk::CHUNK_SIZE as i32 - 1;

            if slice_y_end < min_surface - 3 {
                chunk.fill(slice_idx, BlockId::Stone);
                continue;
            }

            if slice_y_start > max_surface {
                continue;
            }

            let slice = chunk.slice_mut(slice_idx);
            slice.promote(BlockId::Air);

            for (local_x, values) in surface_heights.iter().enumerate().take(Chunk::CHUNK_SIZE) {
                for (local_z, &surface_y) in values.iter().enumerate().take(Chunk::CHUNK_SIZE) {
                    let stone_end = (surface_y - 3).max(0);

                    for local_y in 0..Chunk::CHUNK_SIZE as i32 {
                        let world_y = slice_y_start + local_y;

                        let id = if world_y <= stone_end {
                            BlockId::Stone
                        } else if world_y < surface_y {
                            BlockId::Dirt
                        } else if world_y == surface_y {
                            BlockId::Grass
                        } else {
                            BlockId::Air
                        };

                        slice.set([local_x as i32, local_y as i32, local_z as i32].into(), id);
                    }
                }
            }
        }

        chunk
    }
}
