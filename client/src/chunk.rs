use block::BlockId;
use chunk::{Chunk, ChunkStore};
use render::model::RenderInstance;

pub struct ClientChunk {
    pub chunk: Chunk,
    pub instance: Option<RenderInstance>,
    pub queued: bool,
}

impl ClientChunk {
    pub fn new(chunk: Chunk) -> Self {
        Self {
            chunk,
            instance: None,
            queued: false,
        }
    }

    pub fn iter<'a>(&'a self, store: &'a ChunkStore) -> impl Iterator<Item = BlockId> + 'a {
        self.chunk.iter(store)
    }

    pub fn chunk(&self) -> &Chunk {
        &self.chunk
    }
}
