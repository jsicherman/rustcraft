use std::collections::HashMap;

use chunk::{Block, Chunk, ChunkProvider};
use spatial::{
    aabb::AxisAlignedBoundingBox,
    vectors::{Global, Vec2iChunk, local_to_global},
};

use crate::chunk::ClientChunk;

#[derive(Default)]
pub struct ChunkCache {
    pub chunks: HashMap<Vec2iChunk, ClientChunk>,
}

impl ChunkProvider for ChunkCache {
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
                    chunk.iter_nonempty().map(move |block| {
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
        self.chunks
            .insert(chunk.coordinate(), ClientChunk::new(chunk));
    }
}
