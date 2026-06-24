use block::BlockId;
use chunk::Chunk;
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

    pub fn iter(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.chunk.iter()
    }

    pub fn chunk(&self) -> &Chunk {
        &self.chunk
    }
}
