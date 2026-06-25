use chunk::Chunk;
use render::model::RenderInstance;

pub struct ClientChunk {
    pub chunk: Chunk,
    pub instance: Option<RenderInstance>,
    pub dirty: bool,
    pub queued: bool,
}

impl ClientChunk {
    pub fn new(chunk: Chunk) -> Self {
        Self {
            chunk,
            instance: None,
            dirty: true,
            queued: false,
        }
    }

    pub fn chunk(&self) -> &Chunk {
        &self.chunk
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.queued = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}
