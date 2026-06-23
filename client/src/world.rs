use std::collections::HashMap;

use block::BlockId;
use chunk::{Block, Chunk, ChunkProvider};
use protocol::NetworkId;
use render::Mesh;
use spatial::{
    aabb::AxisAlignedBoundingBox,
    vectors::{Global, Vec2iChunk, local_to_global},
};

#[derive(Default)]
pub struct EntityCache {
    pub entities: HashMap<NetworkId, ClientEntity>,
}

pub struct ClientEntity {
    mesh: Option<Mesh>,
    queued: bool,
    dirty: bool,
}

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

pub struct ClientChunk {
    chunk: Chunk,
    mesh: Option<Mesh>,
    queued: bool,
}

pub trait ClientRenderable {
    fn meshes(&self) -> Option<&Mesh>;
    fn has_meshes(&self) -> bool {
        self.meshes().is_some()
    }
    fn is_queued(&self) -> bool;
    fn is_dirty(&self) -> bool {
        false
    }
    fn mark_dirty(&mut self) {}
    fn queue_mesh(&mut self);
    fn unqueue_mesh(&mut self);
    fn provide_mesh(&mut self, mesh: Mesh);
}

impl ClientChunk {
    pub fn new(chunk: Chunk) -> Self {
        Self {
            chunk,
            mesh: None,
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

impl ClientRenderable for ClientChunk {
    fn meshes(&self) -> Option<&Mesh> {
        self.mesh.as_ref()
    }

    fn is_queued(&self) -> bool {
        self.mesh.is_none() && self.queued
    }

    fn queue_mesh(&mut self) {
        self.queued = true;
    }

    fn unqueue_mesh(&mut self) {
        self.queued = false;
    }

    fn provide_mesh(&mut self, mesh: Mesh) {
        self.mesh = Some(mesh);
        self.queued = false;
    }
}

impl ClientEntity {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ClientEntity {
    fn default() -> Self {
        Self {
            mesh: None,
            queued: false,
            dirty: true,
        }
    }
}

impl ClientRenderable for ClientEntity {
    fn meshes(&self) -> Option<&Mesh> {
        self.mesh.as_ref()
    }

    fn is_queued(&self) -> bool {
        self.queued
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn queue_mesh(&mut self) {
        self.queued = true;
    }

    fn unqueue_mesh(&mut self) {
        self.queued = false;
    }

    fn provide_mesh(&mut self, mesh: Mesh) {
        self.mesh = Some(mesh);
        self.queued = false;
        self.dirty = false;
    }
}
