use std::{borrow::Cow, collections::HashMap};

use resources::block::{BlockId, BlockType};
use serde::{Deserialize, Serialize};
use spatial::{
    WORLD_HEIGHT,
    aabb::AxisAlignedBoundingBox,
    vectors::{CoordSpace, Vec2iChunk, Vec3f, Vec3i, Vec3iLocal},
};

use crate::{
    block_entity::{BlockEntityData, NO_ENTITY},
    packed::PackedIndices,
    store::{ChunkStore, EMPTY_CHUNK_SECTION},
};

pub(crate) const SEED: u64 = 0xD0F7A302BA1C4E3;
pub(crate) const SECTION_COUNT: usize = WORLD_HEIGHT / spatial::CHUNK_SIZE;
const CHUNK_COLUMN: [usize; 3] = [spatial::CHUNK_SIZE, WORLD_HEIGHT, spatial::CHUNK_SIZE];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chunk {
    pub(crate) coordinate: Vec2iChunk,
    pub(crate) slices: [u64; SECTION_COUNT],
    pub(crate) entity_index: [u16; SECTION_COUNT],
    pub(crate) block_entities: Vec<BlockEntityData>,
}

impl Chunk {
    pub const NUM_SLICES: usize = SECTION_COUNT;
}

pub struct ChunkScratch {
    coordinate: Vec2iChunk,
    slices: [ChunkSection; SECTION_COUNT],
    entity_index: [u16; SECTION_COUNT],
    block_entities: Vec<BlockEntityData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChunkSection {
    Homogeneous(BlockId),
    Heterogeneous(Box<[BlockId; Chunk::CHUNK_VOLUME]>),
    Palette {
        palette: Vec<BlockId>,
        indices: PackedIndices,
    },
}

pub trait ChunkLike {
    type Ctx;

    fn coordinate(&self) -> Vec2iChunk;
    fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self;

    fn section_ref<'a>(
        &'a self,
        ctx: &'a Self::Ctx,
        slice_index: usize,
    ) -> Option<Cow<'a, ChunkSection>>;

    fn write_section(&mut self, ctx: &mut Self::Ctx, slice_index: usize, section: ChunkSection);

    fn to_palette_on_edit() -> bool;

    fn new(coordinate: Vec2iChunk) -> Self
    where
        Self: Sized,
    {
        Self::new_with_material(coordinate, BlockId::AIR)
    }

    fn get<S: CoordSpace>(&self, ctx: &Self::Ctx, coordinate: Vec3i<S>) -> Option<BlockId>
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = slice_index_for(coordinate);
        self.section_ref(ctx, slice_index)
            .map(|section| section.get(coordinate))
    }

    fn set<S: CoordSpace>(&mut self, ctx: &mut Self::Ctx, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = slice_index_for(coordinate);
        if let Some(mut section) = self.section_ref(ctx, slice_index).map(Cow::into_owned) {
            section.set(coordinate, id, Self::to_palette_on_edit());
            self.write_section(ctx, slice_index, section);
        }
    }
}

impl ChunkLike for Chunk {
    type Ctx = ChunkStore;

    fn coordinate(&self) -> Vec2iChunk {
        Chunk::coordinate(self)
    }

    fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self {
        Chunk::new_with_material(coordinate, id)
    }

    fn section_ref<'a>(
        &'a self,
        store: &'a Self::Ctx,
        slice_index: usize,
    ) -> Option<Cow<'a, ChunkSection>> {
        let hash = self.slices[slice_index];
        store.load(hash)
    }

    fn write_section(&mut self, store: &mut Self::Ctx, slice_index: usize, section: ChunkSection) {
        let old_hash = self.slices[slice_index];
        let new_hash = store.insert(&section);
        store.replace_reference_if_tracked(old_hash, new_hash);
        self.slices[slice_index] = new_hash;
    }

    fn to_palette_on_edit() -> bool {
        true
    }
}

impl ChunkLike for ChunkScratch {
    type Ctx = ();

    fn coordinate(&self) -> Vec2iChunk {
        ChunkScratch::coordinate(self)
    }

    fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self {
        ChunkScratch::new_with_material(coordinate, id)
    }

    fn section_ref<'a>(
        &'a self,
        _ctx: &'a Self::Ctx,
        slice_index: usize,
    ) -> Option<Cow<'a, ChunkSection>> {
        Some(Cow::Borrowed(&self.slices[slice_index]))
    }

    fn write_section(&mut self, _ctx: &mut Self::Ctx, slice_index: usize, section: ChunkSection) {
        self.slices[slice_index] = section;
    }

    fn to_palette_on_edit() -> bool {
        false
    }
}

#[inline]
fn slice_index_for<S: CoordSpace>(coordinate: Vec3i<S>) -> usize
where
    Vec3iLocal: From<Vec3i<S>>,
{
    Vec3iLocal::from(coordinate).y() as usize / Chunk::CHUNK_SIZE
}

#[inline]
fn section_block_id(section: &ChunkSection, block_index: usize) -> BlockId {
    match section {
        ChunkSection::Homogeneous(id) => *id,
        ChunkSection::Heterogeneous(blocks) => blocks[block_index],
        ChunkSection::Palette { palette, indices } => palette[indices.get(block_index) as usize],
    }
}

#[inline]
fn block_local_position(block_index: usize, slice_index: usize) -> Vec3iLocal {
    let x_local = block_index % Chunk::CHUNK_SIZE;
    let z_local = (block_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;

    let y_local = block_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
    let world_y = slice_index * Chunk::CHUNK_SIZE + y_local;

    Vec3iLocal::from((x_local as i32, world_y as i32, z_local as i32))
}

#[inline]
fn advance_cursor(slice_index: &mut usize, block_index: &mut usize) {
    *block_index += 1;
    if *block_index >= Chunk::CHUNK_VOLUME {
        *block_index = 0;
        *slice_index += 1;
    }
}

#[inline]
fn next_dense_block<F>(
    slice_index: &mut usize,
    block_index: &mut usize,
    slice_count: usize,
    mut block_at: F,
) -> Option<BlockId>
where
    F: FnMut(usize, usize) -> BlockId,
{
    if *slice_index >= slice_count {
        return None;
    }

    let block_id = block_at(*slice_index, *block_index);
    advance_cursor(slice_index, block_index);
    Some(block_id)
}

#[inline]
fn next_nonempty_block<S: CoordSpace, FSkip, FBlock>(
    slice_index: &mut usize,
    block_index: &mut usize,
    slice_count: usize,
    mut skip_slice: FSkip,
    mut block_at: FBlock,
) -> Option<Block<S>>
where
    Vec3i<S>: From<Vec3iLocal>,
    FSkip: FnMut(usize) -> bool,
    FBlock: FnMut(usize, usize) -> Option<BlockId>,
{
    while *slice_index < slice_count {
        if skip_slice(*slice_index) {
            *slice_index += 1;
            *block_index = 0;
            continue;
        }

        let current_index = *block_index;
        let current_slice = *slice_index;
        let Some(block_id) = block_at(current_slice, current_index) else {
            // Missing section data is treated as empty space in non-empty iteration.
            *slice_index += 1;
            *block_index = 0;
            continue;
        };

        advance_cursor(slice_index, block_index);

        let local = block_local_position(current_index, current_slice);
        return Some(Block {
            id: block_id,
            position: Vec3i::<S>::from(local),
        });
    }

    None
}

impl Chunk {
    pub const CHUNK_SIZE: usize = spatial::CHUNK_SIZE;
    pub const CHUNK_VOLUME: usize = spatial::CHUNK_VOLUME;
    pub const CHUNK_COLUMN: [usize; 3] = CHUNK_COLUMN;

    /// Create a new chunk at the given coordinate, filled with air blocks
    pub fn new(coordinate: Vec2iChunk) -> Self {
        Self::new_with_material(coordinate, BlockId::AIR)
    }

    /// Create a new chunk at the given coordinate, filled with the given block
    pub fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self {
        let hash = ChunkSection::Homogeneous(id).ser_hash().1;

        Self {
            coordinate,
            slices: [hash; SECTION_COUNT],
            entity_index: [NO_ENTITY; SECTION_COUNT],
            block_entities: Default::default(),
        }
    }

    pub fn tick(&mut self) {
        for entity in &mut self.block_entities {
            entity.tick();
        }
    }

    pub fn insert_block_entity(&mut self, block_idx: usize, data: BlockEntityData) {
        assert!(
            self.entity_index[block_idx] == NO_ENTITY,
            "block already has an entity"
        );

        let slot = self.block_entities.len() as u16;
        self.block_entities.push(data);
        self.entity_index[block_idx] = slot;
    }

    pub fn get_block_entity(&self, block_idx: usize) -> Option<&BlockEntityData> {
        let slot = self.entity_index[block_idx];
        (slot != NO_ENTITY).then(|| &self.block_entities[slot as usize])
    }

    pub fn get_block_entity_mut(&mut self, block_idx: usize) -> Option<&mut BlockEntityData> {
        let slot = self.entity_index[block_idx];
        (slot != NO_ENTITY).then(|| &mut self.block_entities[slot as usize])
    }

    pub fn remove_block_entity(&mut self, block_idx: usize) {
        let slot = self.entity_index[block_idx];
        assert_ne!(slot, NO_ENTITY);

        let slot = slot as usize;
        let last = self.block_entities.len() - 1;

        if slot != last {
            let displaced_block = self
                .entity_index
                .iter()
                .position(|&s| s == last as u16)
                .unwrap();

            self.entity_index[displaced_block] = slot as u16;
        }

        self.block_entities.swap_remove(slot);
        self.entity_index[block_idx] = NO_ENTITY;
    }

    /// Get the coordinate of this chunk
    pub fn coordinate(&self) -> Vec2iChunk {
        self.coordinate
    }

    pub fn section_hashes(&self) -> &[u64] {
        &self.slices
    }

    pub const fn num_sections(&self) -> usize {
        self.slices.len()
    }

    /// Get the block at the given coordinate
    pub fn get<S: CoordSpace>(&self, store: &ChunkStore, coordinate: Vec3i<S>) -> Option<BlockId>
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        <Self as ChunkLike>::get(self, store, coordinate)
    }

    /// Set the block at the given coordinate
    pub fn set<S: CoordSpace>(&mut self, store: &mut ChunkStore, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        <Self as ChunkLike>::set(self, store, coordinate, id)
    }

    pub fn iter<'a>(&'a self, chunk_store: &'a ChunkStore) -> ChunkIterator<'a> {
        ChunkIterator {
            chunk: self,
            chunk_store,
            slice_index: 0,
            block_index: 0,
        }
    }

    pub fn iter_nonempty<'a, S: CoordSpace>(
        &'a self,
        chunk_store: &'a ChunkStore,
    ) -> NonEmptyChunkIterator<'a, S> {
        NonEmptyChunkIterator {
            phantom: std::marker::PhantomData,
            chunk: self,
            chunk_store,
            slice_index: 0,
            block_index: 0,
        }
    }

    pub fn fill(&mut self, store: &mut ChunkStore, slice: usize, id: BlockId) {
        let old_hash = self.slices[slice];
        let new_hash = store.insert(&ChunkSection::Homogeneous(id));
        store.replace_reference_if_tracked(old_hash, new_hash);
        self.slices[slice] = new_hash;
    }

    pub fn slice<'a>(&self, store: &'a ChunkStore, slice: usize) -> Option<Cow<'a, ChunkSection>> {
        let hash = self.slices[slice];
        store.load(hash)
    }

    pub fn slice_mut(
        &mut self,
        store: &mut ChunkStore,
        slice: usize,
        f: impl FnOnce(&mut ChunkSection),
    ) {
        let old_hash = self.slices[slice];
        if let Some(mut section) = store.load_no_cache(old_hash).map(Cow::into_owned) {
            f(&mut section);
            let new_hash = store.insert(&section);
            store.replace_reference_if_tracked(old_hash, new_hash);
            self.slices[slice] = new_hash;
        }
    }
}

impl ChunkScratch {
    pub fn to_chunk(self, store: &mut ChunkStore) -> Chunk {
        let mut slices = [0; SECTION_COUNT];
        for (i, section) in self.slices.iter().enumerate() {
            let hash = store.insert(&section.compacted());
            slices[i] = hash;
        }

        Chunk {
            slices,
            coordinate: self.coordinate,
            entity_index: self.entity_index,
            block_entities: self.block_entities,
        }
    }
}

impl ChunkScratch {
    pub const CHUNK_SIZE: usize = Chunk::CHUNK_SIZE;
    pub const CHUNK_VOLUME: usize = Chunk::CHUNK_VOLUME;
    pub const CHUNK_COLUMN: [usize; 3] = Chunk::CHUNK_COLUMN;

    /// Create a new chunk at the given coordinate, filled with air blocks
    pub fn new(coordinate: Vec2iChunk) -> Self {
        Self::new_with_material(coordinate, BlockId::AIR)
    }

    /// Create a new chunk at the given coordinate, filled with the given block
    pub fn new_with_material(coordinate: Vec2iChunk, id: BlockId) -> Self {
        Self {
            coordinate,
            slices: std::array::from_fn(|_| ChunkSection::Homogeneous(id)),
            entity_index: [NO_ENTITY; SECTION_COUNT],
            block_entities: Default::default(),
        }
    }

    /// Get the coordinate of this chunk
    pub fn coordinate(&self) -> Vec2iChunk {
        self.coordinate
    }

    pub fn section_hashes(&self) -> &[ChunkSection] {
        &self.slices
    }

    /// Get the block at the given coordinate
    pub fn get<S: CoordSpace>(&self, coordinate: Vec3i<S>) -> BlockId
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let ctx = ();
        <Self as ChunkLike>::get(self, &ctx, coordinate).unwrap()
    }

    /// Set the block at the given coordinate
    pub fn set<S: CoordSpace>(&mut self, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let mut ctx = ();
        <Self as ChunkLike>::set(self, &mut ctx, coordinate, id)
    }

    pub fn iter<'a>(&'a self) -> ChunkScratchIterator<'a> {
        ChunkScratchIterator {
            chunk: self,
            slice_index: 0,
            block_index: 0,
        }
    }

    pub fn iter_nonempty<'a, S: CoordSpace>(&'a self) -> NonEmptyChunkScratchIterator<'a, S> {
        NonEmptyChunkScratchIterator {
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
    chunk_store: &'a ChunkStore,
    slice_index: usize,
    block_index: usize,
}

impl<'a, S: CoordSpace> Iterator for NonEmptyChunkIterator<'a, S>
where
    Vec3i<S>: From<Vec3iLocal>,
{
    type Item = Block<S>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk;
        let chunk_store = &mut self.chunk_store;

        next_nonempty_block(
            &mut self.slice_index,
            &mut self.block_index,
            chunk.slices.len(),
            |slice_index| chunk.slices[slice_index] == *EMPTY_CHUNK_SECTION,
            |slice_index, block_index| {
                let hash = chunk.slices[slice_index];
                chunk_store
                    .load(hash)
                    .map(|section| section_block_id(section.as_ref(), block_index))
            },
        )
    }
}

pub struct ChunkIterator<'a> {
    chunk: &'a Chunk,
    chunk_store: &'a ChunkStore,
    slice_index: usize,
    block_index: usize,
}

impl Iterator for ChunkIterator<'_> {
    type Item = BlockId;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk;
        let chunk_store = &mut self.chunk_store;

        next_dense_block(
            &mut self.slice_index,
            &mut self.block_index,
            chunk.slices.len(),
            |slice_index, block_index| {
                let hash = chunk.slices[slice_index];
                chunk_store
                    .load(hash)
                    .map(|section| section_block_id(section.as_ref(), block_index))
                    .unwrap_or(BlockId::AIR)
            },
        )
    }
}

pub struct NonEmptyChunkScratchIterator<'a, S: CoordSpace> {
    phantom: std::marker::PhantomData<S>,
    chunk: &'a ChunkScratch,
    slice_index: usize,
    block_index: usize,
}

impl<'a, S: CoordSpace> Iterator for NonEmptyChunkScratchIterator<'a, S>
where
    Vec3i<S>: From<Vec3iLocal>,
{
    type Item = Block<S>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk;

        next_nonempty_block(
            &mut self.slice_index,
            &mut self.block_index,
            chunk.slices.len(),
            |slice_index| chunk.slices[slice_index] == ChunkSection::Homogeneous(BlockId::AIR),
            |slice_index, block_index| {
                Some(section_block_id(&chunk.slices[slice_index], block_index))
            },
        )
    }
}

pub struct ChunkScratchIterator<'a> {
    chunk: &'a ChunkScratch,
    slice_index: usize,
    block_index: usize,
}

impl Iterator for ChunkScratchIterator<'_> {
    type Item = BlockId;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk;

        next_dense_block(
            &mut self.slice_index,
            &mut self.block_index,
            chunk.slices.len(),
            |slice_index, block_index| section_block_id(&chunk.slices[slice_index], block_index),
        )
    }
}

impl ChunkSection {
    fn index<S: CoordSpace>(coordinate: Vec3i<S>) -> usize
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let local = Vec3iLocal::from(coordinate);
        let x = local.x() as usize;
        let y = (local.y() as usize) % Chunk::CHUNK_SIZE;
        let z = local.z() as usize;

        x + z * Chunk::CHUNK_SIZE + y * Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE
    }

    fn palette_serialized_size(palette_len: usize, packed_len: usize) -> usize {
        1 + 2 + palette_len + 1 + 4 + packed_len
    }

    pub fn compacted(&self) -> Self {
        match self {
            Self::Homogeneous(_) => self.clone(),
            Self::Heterogeneous(blocks) => {
                let mut palette = Vec::new();
                let mut indices = Vec::with_capacity(Chunk::CHUNK_VOLUME);
                let mut palette_index_by_id = HashMap::new();

                for &id in blocks.iter() {
                    let palette_index = match palette_index_by_id.get(&id) {
                        Some(&i) => i,
                        None => {
                            assert!(palette.len() < u16::MAX as usize, "palette overflow");

                            let i = palette.len() as u16;
                            palette.push(id);
                            palette_index_by_id.insert(id, i);
                            i
                        }
                    };

                    indices.push(palette_index);
                }

                if palette.len() == 1 {
                    return Self::Homogeneous(palette[0]);
                }

                let packed = PackedIndices::from_indices(&indices, palette.len());

                if Self::palette_serialized_size(palette.len(), packed.as_bytes().len())
                    < 1 + Chunk::CHUNK_VOLUME
                {
                    Self::Palette {
                        palette,
                        indices: packed,
                    }
                } else {
                    self.clone()
                }
            }
            Self::Palette { palette, .. } => {
                if palette.len() == 1 {
                    Self::Homogeneous(palette[0])
                } else {
                    self.clone()
                }
            }
        }
    }

    /// Promote this section to a palette if it is currently homogeneous, filling it
    /// with the given block.
    #[inline]
    pub fn promote(&mut self, fill: BlockId, to_palette: bool) {
        if matches!(self, Self::Homogeneous(_)) {
            if to_palette {
                *self = Self::Palette {
                    palette: vec![fill],
                    indices: PackedIndices::filled(Chunk::CHUNK_VOLUME, 1, 0),
                };
            } else {
                *self = Self::Heterogeneous(Box::new([fill; Chunk::CHUNK_VOLUME]));
            }
        }
    }

    #[inline]
    pub fn get<S: CoordSpace>(&self, coordinate: Vec3i<S>) -> BlockId
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        match self {
            Self::Homogeneous(id) => *id,
            Self::Heterogeneous(blocks) => blocks[Self::index(coordinate)],
            Self::Palette { palette, indices } => {
                palette[indices.get(Self::index(coordinate)) as usize]
            }
        }
    }

    #[inline]
    pub fn set_many<S: CoordSpace, I>(&mut self, entries: I, to_palette: bool)
    where
        I: IntoIterator<Item = (Vec3i<S>, BlockId)>,
        Vec3iLocal: From<Vec3i<S>>,
    {
        let edits: Vec<_> = entries
            .into_iter()
            .map(|(coordinate, id)| (Self::index(coordinate), id))
            .collect();

        if edits.is_empty() {
            return;
        }

        if let Self::Homogeneous(fill) = self {
            let fill = *fill;
            if edits.iter().all(|(_, id)| *id == fill) {
                return;
            }
            self.promote(fill, to_palette);
        }

        match self {
            Self::Homogeneous(_) => unreachable!(),
            Self::Heterogeneous(blocks) => {
                for (voxel_index, id) in edits {
                    blocks[voxel_index] = id;
                }
            }
            Self::Palette { palette, indices } => {
                let mut palette_index_by_id: HashMap<_, _> = palette
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(i, id)| (id, i as u16))
                    .collect();

                for (voxel_index, id) in edits {
                    let palette_index = match palette_index_by_id.get(&id) {
                        Some(&i) => i,
                        None => {
                            assert!(palette.len() < u16::MAX as usize, "palette overflow");

                            let i = palette.len() as u16;
                            palette.push(id);
                            palette_index_by_id.insert(id, i);
                            if PackedIndices::bits_for_palette_len(palette.len())
                                != indices.bits_per_index()
                            {
                                *indices = indices.repacked(palette.len());
                            }
                            i
                        }
                    };

                    indices.set(voxel_index, palette_index);
                }
            }
        }
    }

    #[inline]
    pub fn set<S: CoordSpace>(&mut self, coordinate: Vec3i<S>, id: BlockId, to_palette: bool)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        self.set_many(std::iter::once((coordinate, id)), to_palette);
    }
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

    pub fn aabb(&self, block_type: &BlockType) -> AxisAlignedBoundingBox<S> {
        let offset = block_type.dimensions().offset().into();
        let min = Vec3f::from(self.position) + offset;
        let max = min + block_type.dimensions().size().into();

        AxisAlignedBoundingBox::new(min, max)
    }
}
