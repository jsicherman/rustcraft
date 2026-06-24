use std::collections::HashMap;

use chunk::{Block, Chunk, ChunkProvider, ChunkStore};
use spatial::{
    aabb::AxisAlignedBoundingBox,
    vectors::{Global, Vec2iChunk, local_to_global},
};

use crate::chunk::ClientChunk;

#[derive(Default)]
pub struct ChunkCache {
    pub chunks: HashMap<Vec2iChunk, ClientChunk>,
    chunk_store: ChunkStore,
}

impl ChunkProvider for ChunkCache {
    fn store(&self) -> &ChunkStore {
        &self.chunk_store
    }
    fn store_mut(&mut self) -> &mut ChunkStore {
        &mut self.chunk_store
    }

    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a> {
        Box::new(
            aabb.chunks()
                .filter_map(move |coordinate| {
                    self.chunk(coordinate).map(|chunk| (coordinate, chunk))
                })
                .flat_map(move |(coordinate, chunk)| {
                    chunk.iter_nonempty(self.store()).map(move |block| {
                        let global_position = local_to_global(block.position(), coordinate);
                        Block::new(block.id(), global_position)
                    })
                }),
        )
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

    fn insert_chunk(&mut self, chunk: Chunk) {
        self.insert_or_replace(chunk);
    }
}

impl ChunkCache {
    pub fn insert_or_replace(&mut self, chunk: Chunk) {
        let coordinate = chunk.coordinate();
        if let Some(previous) = self.chunks.insert(coordinate, ClientChunk::new(chunk)) {
            self.chunk_store.untrack_chunk(previous.chunk());
        }

        if let Some(inserted) = self.chunks.get(&coordinate) {
            self.chunk_store.track_chunk(inserted.chunk());
        }
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
}
