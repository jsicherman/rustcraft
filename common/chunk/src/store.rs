use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

use anyhow::Error;
use block::BlockId;
use spatial::{
    aabb::AxisAlignedBoundingBox,
    orientation::Direction,
    vectors::{Global, Vec2iChunk, Vec3fGlobal, Vec3iGlobal, Vec3iLocal, local_to_global},
};
use xxhash_rust::xxh64::xxh64;

use crate::{
    Block, Chunk, ChunkSection, SEED, packed::PackedIndices, persistence::ChunkPersistence,
};

pub(crate) static EMPTY_CHUNK_SECTION: LazyLock<u64> =
    LazyLock::new(|| ChunkSection::Homogeneous(BlockId::AIR).ser_hash().1);

pub(crate) static HOMOGENEOUS_SECTIONS: LazyLock<HashMap<u64, Vec<u8>>> = LazyLock::new(|| {
    let mut sections = HashMap::new();
    for id in BlockId::iter() {
        let (bytes, hash) = ChunkSection::Homogeneous(id).ser_hash();
        sections.insert(hash, bytes);
    }
    sections
});

pub fn materialize(chunk: &Chunk, store: &ChunkStore) -> Vec<u32> {
    let mut voxels = Vec::with_capacity(Chunk::CHUNK_VOLUME * chunk.section_hashes().len());

    for &hash in chunk.section_hashes() {
        let Some(section) = store.load(hash) else {
            voxels.resize(voxels.len() + Chunk::CHUNK_VOLUME, *BlockId::AIR as u32);
            continue;
        };

        match section.as_ref() {
            ChunkSection::Homogeneous(id) => {
                voxels.resize(voxels.len() + Chunk::CHUNK_VOLUME, **id as u32);
            }
            ChunkSection::Heterogeneous(blocks) => {
                voxels.extend(blocks.iter().map(|&id| *id as u32));
            }
            ChunkSection::Palette { .. } => {
                for block_index in 0..Chunk::CHUNK_VOLUME {
                    let x_local = block_index % Chunk::CHUNK_SIZE;
                    let z_local = (block_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;
                    let y_local = block_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
                    let local = Vec3iLocal::from((x_local as i32, y_local as i32, z_local as i32));
                    voxels.push(*section.get(local) as u32);
                }
            }
        }
    }

    voxels
}

#[derive(Debug, Default)]
pub struct ChunkStore {
    map: HashMap<u64, Vec<u8>>,
    ref_counts: HashMap<u64, usize>,
}

impl ChunkSection {
    pub(crate) fn ser(&self) -> Vec<u8> {
        match self {
            Self::Homogeneous(id) => {
                let mut v = vec![0u8];
                v.push(**id);
                v
            }
            Self::Palette { palette, indices } => {
                let mut v = vec![1u8];
                assert!(palette.len() <= u16::MAX as usize);
                v.extend_from_slice(&(palette.len() as u16).to_le_bytes());
                for id in palette {
                    v.push(**id);
                }
                v.push(indices.bits_per_index());
                v.extend_from_slice(&(indices.len() as u32).to_le_bytes());
                v.extend_from_slice(indices.as_bytes());
                v
            }
            Self::Heterogeneous(blocks) => {
                let mut v = vec![2u8];
                for &id in blocks.iter() {
                    v.push(*id);
                }
                v
            }
        }
    }

    pub(crate) fn deser(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() {
            anyhow::bail!("Empty byte slice");
        }

        match bytes[0] {
            0 => {
                if bytes.len() != 2 {
                    anyhow::bail!("Expected exactly 2 bytes");
                }
                Ok(Self::Homogeneous(BlockId(bytes[1])))
            }
            1 => {
                if bytes.len() < 3 {
                    anyhow::bail!("Expected at least 3 bytes");
                }

                let palette_len = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
                if palette_len == 0 {
                    anyhow::bail!("Empty palette");
                }

                let mut offset = 3;
                let mut palette = Vec::with_capacity(palette_len);

                for _ in 0..palette_len {
                    if offset + 1 > bytes.len() {
                        anyhow::bail!("Invalid length");
                    }
                    palette.push(BlockId(bytes[offset]));
                    offset += 1;
                }

                if offset + 5 > bytes.len() {
                    anyhow::bail!("Invalid length");
                }

                let bits_per_index = bytes[offset];
                offset += 1;

                let indices_len = u32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]) as usize;

                offset += 4;
                let packed_len = PackedIndices::packed_len(indices_len, bits_per_index);
                if offset + packed_len != bytes.len() {
                    anyhow::bail!("Invalid length");
                }

                let indices = PackedIndices::from_parts(
                    bits_per_index,
                    indices_len,
                    bytes[offset..].to_vec(),
                )?;

                let max_palette_index = palette_len.saturating_sub(1) as u16;
                for idx in 0..indices_len {
                    if indices.get(idx) > max_palette_index {
                        anyhow::bail!("Index out of bounds");
                    }
                }

                if indices_len != Chunk::CHUNK_VOLUME {
                    anyhow::bail!("Invalid index length");
                }

                Ok(Self::Palette { palette, indices })
            }
            2 => {
                if bytes.len() != 1 + Chunk::CHUNK_VOLUME {
                    anyhow::bail!("Invalid heterogeneous length");
                }

                let mut blocks = Box::new([BlockId::AIR; Chunk::CHUNK_VOLUME]);
                for (block, &raw) in blocks.iter_mut().zip(bytes[1..].iter()) {
                    *block = BlockId(raw);
                }

                Ok(Self::Heterogeneous(blocks))
            }
            _ => anyhow::bail!("Unknown type"),
        }
    }

    pub(crate) fn ser_hash(&self) -> (Vec<u8>, u64) {
        let bytes = self.ser();
        let hash = xxh64(bytes.as_slice(), SEED);
        (bytes, hash)
    }
}

impl ChunkStore {
    fn get_bytes(&self, hash: u64) -> Option<&[u8]> {
        self.map
            .get(&hash)
            .map(|v| v.as_slice())
            .or_else(|| HOMOGENEOUS_SECTIONS.get(&hash).map(|v| v.as_slice()))
    }

    pub fn load(&self, hash: u64) -> Option<Cow<'_, ChunkSection>> {
        if let Some(bytes) = HOMOGENEOUS_SECTIONS.get(&hash) {
            let section = ChunkSection::deser(bytes).ok()?;
            return Some(Cow::Owned(section));
        }

        let bytes = self.get_bytes(hash)?;
        let section = ChunkSection::deser(bytes).ok()?;
        Some(Cow::Owned(section))
    }

    pub fn load_no_cache(&self, hash: u64) -> Option<Cow<'_, ChunkSection>> {
        self.load(hash)
    }

    pub fn get(&self, hash: u64) -> Option<&[u8]> {
        self.get_bytes(hash)
    }

    fn insert_bytes(&mut self, bytes: Vec<u8>) -> u64 {
        let hash = xxh64(bytes.as_slice(), SEED);
        self.map.entry(hash).or_insert(bytes);
        hash
    }

    pub fn insert(&mut self, section: &ChunkSection) -> u64 {
        self.insert_bytes(section.ser())
    }

    fn add_ref(&mut self, hash: u64) {
        *self.ref_counts.entry(hash).or_insert(0) += 1;
    }

    fn release_ref(&mut self, hash: u64) {
        let Some(count) = self.ref_counts.get_mut(&hash) else {
            return;
        };

        *count -= 1;
        if *count == 0 {
            self.ref_counts.remove(&hash);
            self.map.remove(&hash);
        }
    }

    pub fn track_chunk(&mut self, chunk: &Chunk) {
        for &hash in chunk.section_hashes() {
            self.add_ref(hash);
        }
    }

    pub fn untrack_chunk(&mut self, chunk: &Chunk) {
        for &hash in chunk.section_hashes() {
            self.release_ref(hash);
        }
    }

    pub fn replace_reference_if_tracked(&mut self, old_hash: u64, new_hash: u64) {
        if old_hash == new_hash {
            return;
        }

        if self.ref_counts.get(&old_hash).copied().unwrap_or_default() == 0 {
            return;
        }

        self.add_ref(new_hash);
        self.release_ref(old_hash);
    }
}

#[derive(Default)]
pub struct ChunkMap {
    pub(crate) chunks: HashMap<Vec2iChunk, Chunk>,
    pub(crate) chunk_store: ChunkStore,
    pub(crate) persistence: Option<ChunkPersistence>,
    pub(crate) chunk_states: HashMap<Vec2iChunk, ChunkState>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChunkState(u8);

impl ChunkState {
    pub(crate) const DIRTY: u8 = 1 << 0;
    pub(crate) const LOAD_PENDING: u8 = 1 << 1;
    pub(crate) const LOAD_MISSING: u8 = 1 << 2;

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub(crate) fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }
}

pub trait ChunkProvider {
    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a>;

    fn store(&self) -> &ChunkStore;
    fn store_mut(&mut self) -> &mut ChunkStore;

    fn chunk_and_store_mut(&mut self, coord: Vec2iChunk) -> (Option<&mut Chunk>, &mut ChunkStore);

    fn chunk(&self, coordinate: Vec2iChunk) -> Option<&Chunk>;
    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk>;
    fn contains_chunk(&self, coordinate: Vec2iChunk) -> bool {
        self.chunk(coordinate).is_some()
    }
    fn insert_chunk(&mut self, chunk: Chunk);

    fn block(&self, coordinate: Vec3fGlobal) -> Option<Block<Global>> {
        let chunk_coord = Vec2iChunk::from(coordinate);
        let local_coord = Vec3iLocal::from(coordinate);

        let chunk = self.chunk(chunk_coord)?;
        let block_id = chunk.get(self.store(), local_coord)?;
        Some(Block::new(
            block_id,
            local_to_global(local_coord, chunk_coord),
        ))
    }

    fn set_block(&mut self, coordinate: Vec3fGlobal, block_id: BlockId) -> Option<()> {
        let chunk_coord = Vec2iChunk::from(coordinate);
        let local_coord = Vec3iLocal::from(coordinate);

        let (chunk, store) = self.chunk_and_store_mut(chunk_coord);
        chunk?.set(store, local_coord, block_id);
        Some(())
    }
}

impl ChunkProvider for ChunkMap {
    fn store(&self) -> &ChunkStore {
        &self.chunk_store
    }
    fn store_mut(&mut self) -> &mut ChunkStore {
        &mut self.chunk_store
    }

    fn chunk_and_store_mut(&mut self, coord: Vec2iChunk) -> (Option<&mut Chunk>, &mut ChunkStore) {
        let chunk = self.chunks.get_mut(&coord);
        (chunk, &mut self.chunk_store)
    }

    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a> {
        let mut blocks = Vec::new();

        for coordinate in aabb.chunks() {
            let Some(chunk) = self.chunks.get(&coordinate) else {
                continue;
            };

            for (slice_index, &hash) in chunk.section_hashes().iter().enumerate() {
                let Some(section) = self.chunk_store.load(hash) else {
                    continue;
                };

                match section.as_ref() {
                    ChunkSection::Homogeneous(BlockId::AIR) => continue,
                    ChunkSection::Homogeneous(id) => {
                        for block_index in 0..Chunk::CHUNK_VOLUME {
                            let x_local = block_index % Chunk::CHUNK_SIZE;
                            let z_local = (block_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;
                            let y_local = block_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
                            let world_y = slice_index * Chunk::CHUNK_SIZE + y_local;

                            let local =
                                Vec3iLocal::from((x_local as i32, world_y as i32, z_local as i32));
                            let global_position = local_to_global(local, coordinate);
                            blocks.push(Block::new(*id, global_position));
                        }
                    }
                    ChunkSection::Heterogeneous(section_blocks) => {
                        for (block_index, &id) in section_blocks.iter().enumerate() {
                            let x_local = block_index % Chunk::CHUNK_SIZE;
                            let z_local = (block_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;
                            let y_local = block_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
                            let world_y = slice_index * Chunk::CHUNK_SIZE + y_local;

                            let local =
                                Vec3iLocal::from((x_local as i32, world_y as i32, z_local as i32));
                            let global_position = local_to_global(local, coordinate);
                            blocks.push(Block::new(id, global_position));
                        }
                    }
                    ChunkSection::Palette { palette, indices } => {
                        for block_index in 0..Chunk::CHUNK_VOLUME {
                            let id = palette[indices.get(block_index) as usize];
                            let x_local = block_index % Chunk::CHUNK_SIZE;
                            let z_local = (block_index / Chunk::CHUNK_SIZE) % Chunk::CHUNK_SIZE;
                            let y_local = block_index / (Chunk::CHUNK_SIZE * Chunk::CHUNK_SIZE);
                            let world_y = slice_index * Chunk::CHUNK_SIZE + y_local;

                            let local =
                                Vec3iLocal::from((x_local as i32, world_y as i32, z_local as i32));
                            let global_position = local_to_global(local, coordinate);
                            blocks.push(Block::new(id, global_position));
                        }
                    }
                }
            }
        }

        Box::new(blocks.into_iter())
    }

    fn chunk(&self, coordinate: Vec2iChunk) -> Option<&Chunk> {
        self.chunks.get(&coordinate)
    }

    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk> {
        if self.chunks.contains_key(&coordinate) {
            self.set_chunk_state(coordinate, ChunkState::DIRTY, true);
        }
        self.chunks.get_mut(&coordinate)
    }

    fn insert_chunk(&mut self, chunk: Chunk) {
        self.insert_chunk_internal(chunk, true);
    }
}

impl ChunkMap {
    pub(crate) fn set_chunk_state(&mut self, coordinate: Vec2iChunk, flag: u8, enabled: bool) {
        let remove_state = {
            let state = self.chunk_states.entry(coordinate).or_default();
            state.set(flag, enabled);
            state.is_empty()
        };

        if remove_state {
            self.chunk_states.remove(&coordinate);
        }
    }

    pub(crate) fn has_chunk_state(&self, coordinate: Vec2iChunk, flag: u8) -> bool {
        self.chunk_states
            .get(&coordinate)
            .copied()
            .is_some_and(|state| state.has(flag))
    }

    pub(crate) fn take_chunk_state(&mut self, coordinate: Vec2iChunk, flag: u8) -> bool {
        let mut had = false;
        let mut remove_state = false;

        if let Some(state) = self.chunk_states.get_mut(&coordinate) {
            had = state.has(flag);
            state.set(flag, false);
            remove_state = state.is_empty();
        }

        if remove_state {
            self.chunk_states.remove(&coordinate);
        }

        had
    }

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
        let d2 = max_distance * max_distance;
        let to_unload: Vec<_> = self
            .chunks
            .keys()
            .copied()
            .filter(|coord| {
                !player_positions
                    .iter()
                    .any(|pos| (*coord - *pos).length_sq() <= d2)
            })
            .collect();

        let unloaded = to_unload.len();
        for coord in to_unload {
            if let Some(chunk) = self.chunks.remove(&coord) {
                if let Some(persistence) = &self.persistence
                    && let Err(err) = persistence.enqueue_save(&chunk, &self.chunk_store)
                {
                    tracing::warn!("Failed to persist chunk {coord:?} on unload: {err:#}");
                }

                self.chunk_store.untrack_chunk(&chunk);
                self.chunk_states.remove(&coord);
            }
        }

        unloaded
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
