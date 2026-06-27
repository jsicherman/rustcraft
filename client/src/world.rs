use std::collections::HashMap;

use chunk::{Block, Chunk, ChunkProvider, ChunkStore, WireChunk};
use render::Renderer;
use resources::block::BlockId;
use spatial::{
    CHUNK_SIZE,
    aabb::AxisAlignedBoundingBox,
    vectors::{Global, IntoSpace, Vec2iChunk, Vec3iGlobal, Vec3iLocal, local_to_global},
};

use crate::chunk::ClientChunk;

#[derive(Default)]
pub struct ChunkCache {
    pub chunks: HashMap<Vec2iChunk, ClientChunk>,
    pub chunk_store: ChunkStore,
}

impl ChunkProvider for ChunkCache {
    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a> {
        let mut blocks = Vec::new();

        let min_x = aabb.min().x().floor() as i32;
        let min_y = aabb.min().y().floor() as i32;
        let min_z = aabb.min().z().floor() as i32;

        let max_x = aabb.max().x().ceil() as i32;
        let max_y = aabb.max().y().ceil() as i32;
        let max_z = aabb.max().z().ceil() as i32;

        let chunk_size = Chunk::CHUNK_SIZE as i32;

        for y in min_y.max(0)..max_y.min(spatial::WORLD_HEIGHT as i32) {
            for z in min_z..max_z {
                for x in min_x..max_x {
                    let coordinate =
                        Vec2iChunk::from([x.div_euclid(chunk_size), z.div_euclid(chunk_size)]);

                    let Some(client_chunk) = self.chunks.get(&coordinate) else {
                        continue;
                    };

                    let local =
                        Vec3iLocal::from([x.rem_euclid(chunk_size), y, z.rem_euclid(chunk_size)]);

                    let Some(id) = client_chunk.chunk().get(&self.chunk_store, local) else {
                        continue;
                    };

                    if id == BlockId::AIR {
                        continue;
                    }

                    blocks.push(Block::new(id, local_to_global(local, coordinate)));
                }
            }
        }

        Box::new(blocks.into_iter())
    }

    fn chunk(&self, coordinate: Vec2iChunk) -> Option<&Chunk> {
        self.chunks
            .get(&coordinate)
            .map(|client_chunk| client_chunk.chunk())
    }

    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk> {
        self.chunks
            .get_mut(&coordinate)
            .map(|client_chunk| &mut client_chunk.chunk)
    }

    fn chunk_and_store_mut(&mut self, coord: Vec2iChunk) -> (Option<&mut Chunk>, &mut ChunkStore) {
        let chunk = self.chunks.get_mut(&coord);
        (chunk.map(|c| &mut c.chunk), &mut self.chunk_store)
    }

    fn insert_chunk(&mut self, chunk: Chunk) {
        self.insert_or_replace(chunk);
    }

    fn store(&self) -> &ChunkStore {
        &self.chunk_store
    }

    fn store_mut(&mut self) -> &mut ChunkStore {
        &mut self.chunk_store
    }
}

impl ChunkCache {
    pub fn chunks_and_store_mut(
        &mut self,
    ) -> (&mut HashMap<Vec2iChunk, ClientChunk>, &mut ChunkStore) {
        (&mut self.chunks, &mut self.chunk_store)
    }

    pub fn insert_or_replace(&mut self, chunk: Chunk) {
        let coordinate = chunk.coordinate();
        if let Some(previous) = self.chunks.insert(coordinate, ClientChunk::new(chunk)) {
            self.chunk_store.untrack_chunk(previous.chunk());
        }

        if let Some(inserted) = self.chunks.get(&coordinate) {
            self.chunk_store.track_chunk(inserted.chunk());
        }
    }

    pub fn insert_wire_chunk(&mut self, wire_chunk: WireChunk) {
        let chunk = wire_chunk.into_chunk(&mut self.chunk_store);
        self.insert_or_replace(chunk);
    }

    pub fn retain_chunks(&mut self, mut f: impl FnMut(&Vec2iChunk, &ClientChunk) -> bool) {
        let to_remove: Vec<_> = self
            .chunks
            .iter()
            .filter_map(|(coord, chunk)| if f(coord, chunk) { None } else { Some(*coord) })
            .collect();

        for coord in to_remove {
            if let Some(removed) = self.chunks.remove(&coord) {
                self.chunk_store.untrack_chunk(removed.chunk());
            }
        }
    }

    pub fn invalidate_chunk_mesh(&mut self, renderer: &mut Renderer, coordinate: Vec2iChunk) {
        let _ = renderer;

        let Some(client_chunk) = self.chunks.get_mut(&coordinate) else {
            return;
        };

        client_chunk.mark_dirty();
    }

    pub fn invalidate_chunk_meshes_around_block(
        &mut self,
        renderer: &mut Renderer,
        block_position: Vec3iGlobal,
    ) {
        let chunk_coordinate = IntoSpace::<spatial::vectors::Chunk>::into_space(block_position);
        let base = Vec2iChunk::from([chunk_coordinate[0], chunk_coordinate[2]]);

        self.invalidate_chunk_mesh(renderer, base);

        let chunk_size = CHUNK_SIZE as i32;
        let local_x = block_position[0].rem_euclid(chunk_size);
        let local_z = block_position[2].rem_euclid(chunk_size);

        if local_x == 0 {
            self.invalidate_chunk_mesh(renderer, Vec2iChunk::from([base.x() - 1, base.z()]));
        }
        if local_x == chunk_size - 1 {
            self.invalidate_chunk_mesh(renderer, Vec2iChunk::from([base.x() + 1, base.z()]));
        }
        if local_z == 0 {
            self.invalidate_chunk_mesh(renderer, Vec2iChunk::from([base.x(), base.z() - 1]));
        }
        if local_z == chunk_size - 1 {
            self.invalidate_chunk_mesh(renderer, Vec2iChunk::from([base.x(), base.z() + 1]));
        }
    }
}
