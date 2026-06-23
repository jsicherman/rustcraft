use std::collections::BTreeMap;

use block::BlockId;
use serde::{Deserialize, Serialize};
use spatial::{
    WORLD_HEIGHT,
    aabb::AxisAlignedBoundingBox,
    orientation::Direction,
    vectors::{CoordSpace, Global, Vec2iChunk, Vec3i, Vec3iGlobal, Vec3iLocal, local_to_global},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chunk {
    coordinate: Vec2iChunk,
    slices: [ChunkSection; spatial::WORLD_HEIGHT / Self::CHUNK_SIZE],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkSection {
    Homogeneous(BlockId),
    #[serde(with = "heterogeneous_serde")]
    Heterogeneous(Box<[BlockId; Chunk::CHUNK_VOLUME]>),
}

mod heterogeneous_serde {
    use block::BlockId;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    use crate::Chunk;

    #[allow(clippy::borrowed_box)]
    pub fn serialize<S: Serializer>(
        v: &Box<[BlockId; Chunk::CHUNK_VOLUME]>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Box<[BlockId; Chunk::CHUNK_VOLUME]>, D::Error> {
        let vec = Vec::<BlockId>::deserialize(d)?;
        let arr: [BlockId; Chunk::CHUNK_VOLUME] = vec
            .try_into()
            .map_err(|_| D::Error::custom("wrong length"))?;
        Ok(Box::new(arr))
    }
}

impl Chunk {
    pub const CHUNK_SIZE: usize = spatial::CHUNK_SIZE;
    pub const CHUNK_VOLUME: usize = spatial::CHUNK_VOLUME;
    pub const CHUNK_COLUMN: [usize; 3] = [Self::CHUNK_SIZE, WORLD_HEIGHT, Self::CHUNK_SIZE];

    /// Create a new chunk at the given coordinate, filled with air blocks
    pub fn new(coordinate: Vec2iChunk) -> Self {
        Self::new_with_material(coordinate, BlockId::Air)
    }

    /// Create a new chunk at the given coordinate, filled with the given block
    pub fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self {
        Self {
            coordinate,
            slices: std::array::from_fn(|_| ChunkSection::Homogeneous(id)),
        }
    }

    /// Get the coordinate of this chunk
    pub fn coordinate(&self) -> Vec2iChunk {
        self.coordinate
    }

    /// Get the block at the given coordinate
    pub fn get<S: CoordSpace>(&self, coordinate: Vec3i<S>) -> BlockId
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = Vec3iLocal::from(coordinate).y() as usize;
        self.slices[slice_index].get(coordinate)
    }

    /// Set the block at the given coordinate
    pub fn set<S: CoordSpace>(&mut self, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = Vec3iLocal::from(coordinate).y() as usize;
        self.slices[slice_index].set(coordinate, id);
    }

    pub fn iter(&self) -> ChunkIterator<'_> {
        ChunkIterator {
            chunk: self,
            slice_index: 0,
            block_index: 0,
        }
    }

    pub fn iter_nonempty<S: CoordSpace>(&self) -> NonEmptyChunkIterator<'_, S> {
        NonEmptyChunkIterator {
            phantom: std::marker::PhantomData,
            chunk: self,
            slice_index: 0,
            block_index: 0,
        }
    }

    pub fn fill(&mut self, slice: usize, id: BlockId) {
        self.slices[slice] = ChunkSection::Homogeneous(id);
    }

    pub fn slice(&self, slice: usize) -> &ChunkSection {
        &self.slices[slice]
    }

    pub fn slice_mut(&mut self, slice: usize) -> &mut ChunkSection {
        &mut self.slices[slice]
    }
}

pub struct NonEmptyChunkIterator<'a, S: CoordSpace> {
    phantom: std::marker::PhantomData<S>,
    chunk: &'a Chunk,
    slice_index: usize,
    block_index: usize,
}

impl<'a, S: CoordSpace> Iterator for NonEmptyChunkIterator<'a, S>
where
    Vec3i<S>: From<Vec3iLocal>,
{
    type Item = Block<S>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.slice_index < self.chunk.slices.len() {
            if matches!(
                self.chunk.slices[self.slice_index],
                ChunkSection::Homogeneous(BlockId::Air)
            ) {
                self.slice_index += 1;
                self.block_index = 0;
                continue;
            }

            let block_id = match &self.chunk.slices[self.slice_index] {
                ChunkSection::Homogeneous(id) => *id,
                ChunkSection::Heterogeneous(blocks) => blocks[self.block_index],
            };

            let current_index = self.block_index;
            let current_slice = self.slice_index;

            self.block_index += 1;
            if self.block_index >= Chunk::CHUNK_VOLUME {
                self.block_index = 0;
                self.slice_index += 1;
            }

            let x_local = current_index % Chunk::CHUNK_SIZE;
            let z_local = (current_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;

            let y_local = current_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
            let world_y = current_slice * Chunk::CHUNK_SIZE + y_local;

            let local = Vec3iLocal::from((x_local as i32, world_y as i32, z_local as i32));

            return Some(Block {
                id: block_id,
                position: Vec3i::<S>::from(local),
            });
        }

        None
    }
}

pub struct ChunkIterator<'a> {
    chunk: &'a Chunk,
    slice_index: usize,
    block_index: usize,
}

impl Iterator for ChunkIterator<'_> {
    type Item = BlockId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice_index >= self.chunk.slices.len() {
            return None;
        }

        let slice = &self.chunk.slices[self.slice_index];
        let block_id = match slice {
            ChunkSection::Homogeneous(id) => *id,
            ChunkSection::Heterogeneous(blocks) => blocks[self.block_index],
        };

        self.block_index += 1;
        if self.block_index >= Chunk::CHUNK_VOLUME {
            self.block_index = 0;
            self.slice_index += 1;
        }

        Some(block_id)
    }
}

impl ChunkSection {
    fn index<S: CoordSpace>(coordinate: Vec3i<S>) -> usize
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let local = Vec3iLocal::from(coordinate);
        let x = local.x() as usize;
        let y = local.y() as usize;
        let z = local.z() as usize;

        x + z * Chunk::CHUNK_SIZE + y * Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE
    }

    /// Promote this section to heterogeneous if it is currently homogeneous, filling it
    /// with the given block.
    pub fn promote(&mut self, fill: BlockId) {
        if matches!(self, Self::Homogeneous(_)) {
            *self = Self::Heterogeneous(Box::new([fill; Chunk::CHUNK_VOLUME]));
        }
    }

    pub fn get<S: CoordSpace>(&self, coordinate: Vec3i<S>) -> BlockId
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        match self {
            Self::Homogeneous(id) => *id,
            Self::Heterogeneous(blocks) => blocks[Self::index(coordinate)],
        }
    }

    pub fn set<S: CoordSpace>(&mut self, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        match self {
            Self::Homogeneous(current_id) => {
                if *current_id != id {
                    let mut blocks = Box::new([*current_id; Chunk::CHUNK_VOLUME]);
                    blocks[Self::index(coordinate)] = id;
                    *self = Self::Heterogeneous(blocks);
                }
            }
            Self::Heterogeneous(blocks) => {
                blocks[Self::index(coordinate)] = id;
            }
        }
    }
}

pub trait ChunkProvider {
    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a>;

    fn chunk(&self, coordinate: Vec2iChunk) -> Option<&Chunk>;
    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk>;
    fn contains_chunk(&self, coordinate: Vec2iChunk) -> bool {
        self.chunk(coordinate).is_some()
    }
    fn insert_chunk(&mut self, chunk: Chunk);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block<S: CoordSpace> {
    id: BlockId,
    position: Vec3i<S>,
}

impl<S: CoordSpace> Block<S> {
    pub fn new(id: BlockId, position: Vec3i<S>) -> Self {
        Self { id, position }
    }

    pub fn id(&self) -> BlockId {
        self.id
    }

    pub fn position(&self) -> Vec3i<S> {
        self.position
    }

    pub fn position_mut(&mut self) -> &mut Vec3i<S> {
        &mut self.position
    }

    pub fn aabb(&self) -> AxisAlignedBoundingBox<S> {
        let min = spatial::vectors::Vec3f::from(self.position);
        let max = min + spatial::vectors::Vec3f::from([1.0, 1.0, 1.0]);

        AxisAlignedBoundingBox::new(min, max)
    }
}

#[derive(Debug, Default)]
pub struct ChunkMap {
    chunks: BTreeMap<Vec2iChunk, Chunk>,
}

impl ChunkProvider for ChunkMap {
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
        self.chunks.get(&coordinate)
    }

    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk> {
        self.chunks.get_mut(&coordinate)
    }

    fn insert_chunk(&mut self, chunk: Chunk) {
        self.chunks.insert(chunk.coordinate(), chunk);
    }
}

impl ChunkMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn neighboring_chunk(
        &self,
        coordinate: Vec2iChunk,
        direction: Direction,
    ) -> Option<&Chunk> {
        let v3i = Vec3iGlobal::from(direction);
        self.chunk(coordinate + [v3i.x(), v3i.z()].into())
    }

    /// Remove chunks that are far from any of the given positions
    /// Keeps chunks within max_distance of any position
    pub fn unload_distant_chunks(
        &mut self,
        player_positions: &[Vec2iChunk],
        max_distance: i32,
    ) -> usize {
        let n_before = self.chunks.len();
        let d2 = max_distance * max_distance;

        self.chunks.retain(|coord, _| {
            player_positions
                .iter()
                .any(|pos| (*coord - *pos).length_sq() <= d2)
        });

        n_before - self.chunks.len()
    }

    /// Get the number of chunks currently loaded
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
