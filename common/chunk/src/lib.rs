use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
};

use block::BlockId;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error, ser::SerializeStruct};
use spatial::{
    WORLD_HEIGHT,
    aabb::AxisAlignedBoundingBox,
    orientation::Direction,
    vectors::{CoordSpace, Global, Vec2iChunk, Vec3i, Vec3iGlobal, Vec3iLocal, local_to_global},
};
use xxhash_rust::xxh64::xxh64 as compute_xxh64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chunk {
    coordinate: Vec2iChunk,
    slices: [u64; spatial::WORLD_HEIGHT / Self::CHUNK_SIZE],
}

impl Serialize for Chunk {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Chunk", 2)?;

        state.serialize_field("coordinate", &self.coordinate)?;

        let mut rle = Vec::new();

        let mut iter = self.slices.iter().copied();
        if let Some(mut current) = iter.next() {
            let mut count = 1;

            for v in iter {
                if v == current && count < u16::MAX {
                    count += 1;
                } else {
                    rle.push((current, count));
                    current = v;
                    count = 1;
                }
            }

            rle.push((current, count));
        }

        state.serialize_field("slices", &rle)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Chunk {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct ChunkHelper {
            coordinate: Vec2iChunk,
            slices: Vec<(u64, u16)>,
        }

        let helper = ChunkHelper::deserialize(deserializer)?;

        let mut slices = [0u64; spatial::WORLD_HEIGHT / Chunk::CHUNK_SIZE];
        let mut idx = 0;

        for (value, count) in helper.slices {
            let end = idx + count as usize;
            if end > slices.len() {
                return Err(serde::de::Error::custom("RLE overflow"));
            }

            for v in slices.iter_mut().take(end).skip(idx) {
                *v = value;
            }

            idx = end;
        }

        if idx != slices.len() {
            return Err(serde::de::Error::custom(
                "RLE did not fill full slice array",
            ));
        }

        Ok(Chunk {
            coordinate: helper.coordinate,
            slices,
        })
    }
}

#[derive(Debug, Default)]
pub struct ChunkStore {
    map: HashMap<u64, Vec<u8>>,
    cache: HashMap<u64, ChunkSection>,
    ref_counts: HashMap<u64, usize>,
}

const SEED: u64 = 0xD0F7A302BA1C4E3;

static EMPTY_CHUNK_SECTION: LazyLock<u64> =
    LazyLock::new(|| ChunkSection::Homogeneous(BlockId::Air).ser_hash().1);

static HOMOGENEOUS_SECTIONS: LazyLock<HashMap<u64, Vec<u8>>> = LazyLock::new(|| {
    let mut sections = HashMap::new();
    for id in [BlockId::Air, BlockId::Stone, BlockId::Grass, BlockId::Dirt] {
        let (bytes, hash) = ChunkSection::Homogeneous(id).ser_hash();
        sections.insert(hash, bytes);
    }
    sections
});

impl ChunkStore {
    fn get_bytes(&self, hash: u64) -> Option<&[u8]> {
        self.map
            .get(&hash)
            .map(|v| v.as_slice())
            .or_else(|| HOMOGENEOUS_SECTIONS.get(&hash).map(|v| v.as_slice()))
    }

    pub fn load(&mut self, hash: u64) -> Option<&ChunkSection> {
        if self.cache.contains_key(&hash) {
            return self.cache.get(&hash);
        }

        let bytes = self.get_bytes(hash)?;
        let section = ChunkSection::deser(bytes).ok()?;
        self.cache.insert(hash, section);
        self.cache.get(&hash)
    }

    pub fn load_no_cache(&self, hash: u64) -> Option<Cow<'_, ChunkSection>> {
        if let Some(section) = self.cache.get(&hash) {
            return Some(Cow::Borrowed(section));
        }

        let bytes = self.get_bytes(hash)?;
        let section = ChunkSection::deser(bytes).ok()?;
        Some(Cow::Owned(section))
    }

    pub fn load_mut(&mut self, hash: u64) -> Option<&mut ChunkSection> {
        if self.cache.contains_key(&hash) {
            return self.cache.get_mut(&hash);
        }

        let bytes = self.get_bytes(hash)?;
        let section = ChunkSection::deser(bytes).ok()?;
        self.cache.insert(hash, section);
        self.cache.get_mut(&hash)
    }

    pub fn get(&self, hash: u64) -> Option<&[u8]> {
        self.get_bytes(hash)
    }

    fn insert_bytes(&mut self, bytes: Vec<u8>) -> u64 {
        let hash = compute_xxh64(bytes.as_slice(), SEED);
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
            self.cache.remove(&hash);
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkSection {
    Homogeneous(BlockId),
    #[serde(with = "heterogeneous_serde")]
    Heterogeneous(Box<[BlockId; Chunk::CHUNK_VOLUME]>),
    Palette {
        palette: Vec<BlockId>,
        indices: Vec<u16>,
    },
}

impl ChunkSection {
    pub fn ser(&self) -> Vec<u8> {
        match self {
            Self::Homogeneous(id) => {
                let mut v = vec![0u8];
                v.extend_from_slice(&(*id as u16).to_le_bytes());
                v
            }
            Self::Palette { palette, indices } => {
                let mut v = vec![1u8];
                assert!(palette.len() <= u16::MAX as usize);
                assert!(indices.len() <= u32::MAX as usize);
                v.extend_from_slice(&(palette.len() as u16).to_le_bytes());
                for id in palette {
                    v.extend_from_slice(&(*id as u16).to_le_bytes());
                }
                v.extend_from_slice(&(indices.len() as u32).to_le_bytes());
                for index in indices {
                    v.extend_from_slice(&index.to_le_bytes());
                }
                v
            }
            Self::Heterogeneous(_) => unimplemented!(),
        }
    }

    pub fn deser(bytes: &[u8]) -> Result<Self, serde::de::value::Error> {
        if bytes.is_empty() {
            return Err(Error::custom("Empty slice"));
        }

        match bytes[0] {
            0 => {
                if bytes.len() != 3 {
                    return Err(Error::custom("Expected exactly 3 bytes"));
                }
                let id = u16::from_le_bytes([bytes[1], bytes[2]]);
                Ok(Self::Homogeneous(
                    BlockId::try_from(id).map_err(|_| Error::custom("Invalid BlockId"))?,
                ))
            }
            1 => {
                if bytes.len() < 3 {
                    return Err(Error::custom("Expected at least 3 bytes"));
                }

                let palette_len = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
                if palette_len == 0 {
                    return Err(Error::custom("Empty palette"));
                }

                let mut offset = 3;
                let mut palette = Vec::with_capacity(palette_len);

                for _ in 0..palette_len {
                    if offset + 2 > bytes.len() {
                        return Err(Error::custom("Invalid length"));
                    }
                    let id = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                    palette
                        .push(BlockId::try_from(id).map_err(|_| Error::custom("Invalid BlockId"))?);
                    offset += 2;
                }

                if offset + 4 > bytes.len() {
                    return Err(Error::custom("Invalid length"));
                }

                let indices_len = u32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]) as usize;

                offset += 4;
                let mut indices = Vec::with_capacity(indices_len);

                for _ in 0..indices_len {
                    if offset + 2 > bytes.len() {
                        return Err(Error::custom("Invalid length"));
                    }

                    let index = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                    if index as usize >= palette_len {
                        return Err(Error::custom("Index out of bounds"));
                    }
                    indices.push(index);
                    offset += 2;
                }

                if offset != bytes.len() {
                    return Err(Error::custom("Trailing bytes"));
                }

                if indices_len != Chunk::CHUNK_VOLUME {
                    return Err(Error::custom("Invalid index length"));
                }

                Ok(Self::Palette { palette, indices })
            }
            _ => Err(Error::custom("Unknown type")),
        }
    }

    fn ser_hash(&self) -> (Vec<u8>, u64) {
        let bytes = self.ser();
        let hash = compute_xxh64(bytes.as_slice(), SEED);
        (bytes, hash)
    }
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
        let hash = ChunkSection::Homogeneous(id).ser_hash().1;

        Self {
            coordinate,
            slices: [hash; WORLD_HEIGHT / Self::CHUNK_SIZE],
        }
    }

    /// Get the coordinate of this chunk
    pub fn coordinate(&self) -> Vec2iChunk {
        self.coordinate
    }

    pub fn section_hashes(&self) -> &[u64] {
        &self.slices
    }

    /// Get the block at the given coordinate
    pub fn get<S: CoordSpace>(
        &self,
        store: &mut ChunkStore,
        coordinate: Vec3i<S>,
    ) -> Option<BlockId>
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = Vec3iLocal::from(coordinate).y() as usize;
        let hash = self.slices[slice_index];
        store.load(hash).map(|section| section.get(coordinate))
    }

    /// Set the block at the given coordinate
    pub fn set<S: CoordSpace>(&mut self, store: &mut ChunkStore, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        let slice_index = Vec3iLocal::from(coordinate).y() as usize;
        let old_hash = self.slices[slice_index];
        if let Some(mut section) = store.load_no_cache(old_hash).map(Cow::into_owned) {
            section.set(coordinate, id);
            let new_hash = store.insert(&section);
            store.replace_reference_if_tracked(old_hash, new_hash);
            self.slices[slice_index] = new_hash;
        }
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

    pub fn fill_direct(&mut self, store: &mut ChunkStore, slice: usize, id: BlockId) {
        let new_hash = store.insert(&ChunkSection::Homogeneous(id));
        self.slices[slice] = new_hash;
    }

    pub fn slice<'a>(&self, store: &'a mut ChunkStore, slice: usize) -> Option<&'a ChunkSection> {
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
        while self.slice_index < self.chunk.slices.len() {
            if self.chunk.slices[self.slice_index] == *EMPTY_CHUNK_SECTION {
                self.slice_index += 1;
                self.block_index = 0;
                continue;
            }

            let hash = self.chunk.slices[self.slice_index];
            let Some(section) = self.chunk_store.load_no_cache(hash) else {
                // Missing section data is treated as empty space in non-empty iteration.
                self.slice_index += 1;
                self.block_index = 0;
                continue;
            };

            let block_id = match section.as_ref() {
                ChunkSection::Homogeneous(id) => *id,
                ChunkSection::Heterogeneous(blocks) => blocks[self.block_index],
                ChunkSection::Palette { palette, indices } => {
                    palette[indices[self.block_index] as usize]
                }
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
    chunk_store: &'a ChunkStore,
    slice_index: usize,
    block_index: usize,
}

impl Iterator for ChunkIterator<'_> {
    type Item = BlockId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice_index >= self.chunk.slices.len() {
            return None;
        }

        let hash = self.chunk.slices[self.slice_index];
        let block_id = match self.chunk_store.load_no_cache(hash) {
            Some(section) => match section.as_ref() {
                ChunkSection::Homogeneous(id) => *id,
                ChunkSection::Heterogeneous(blocks) => blocks[self.block_index],
                ChunkSection::Palette { palette, indices } => {
                    palette[indices[self.block_index] as usize]
                }
            },
            None => BlockId::Air,
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

    /// Promote this section to a palette if it is currently homogeneous, filling it
    /// with the given block.
    #[inline]
    pub fn promote(&mut self, fill: BlockId) {
        if matches!(self, Self::Homogeneous(_)) {
            *self = Self::Palette {
                palette: vec![fill],
                indices: vec![0; Chunk::CHUNK_VOLUME],
            };
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
                palette[indices[Self::index(coordinate)] as usize]
            }
        }
    }

    #[inline]
    pub fn set_many<S: CoordSpace, I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (Vec3i<S>, BlockId)>,
        Vec3iLocal: From<Vec3i<S>>,
    {
        let edits: Vec<(usize, BlockId)> = entries
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
            self.promote(fill);
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
                            i
                        }
                    };

                    indices[voxel_index] = palette_index;
                }
            }
        }
    }

    #[inline]
    pub fn set<S: CoordSpace>(&mut self, coordinate: Vec3i<S>, id: BlockId)
    where
        Vec3iLocal: From<Vec3i<S>>,
    {
        self.set_many(std::iter::once((coordinate, id)));
    }
}

pub trait ChunkProvider {
    fn intersecting<'a>(
        &'a self,
        aabb: &'a AxisAlignedBoundingBox<Global>,
    ) -> Box<dyn Iterator<Item = Block<Global>> + 'a>;

    fn store(&self) -> &ChunkStore;
    fn store_mut(&mut self) -> &mut ChunkStore;

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
    chunk_store: ChunkStore,
}

impl ChunkProvider for ChunkMap {
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
        self.chunks.get(&coordinate)
    }

    fn chunk_mut(&mut self, coordinate: Vec2iChunk) -> Option<&mut Chunk> {
        self.chunks.get_mut(&coordinate)
    }

    fn insert_chunk(&mut self, chunk: Chunk) {
        let coordinate = chunk.coordinate();

        if let Some(previous) = self.chunks.insert(coordinate, chunk) {
            self.chunk_store.untrack_chunk(&previous);
        }

        let inserted = self
            .chunks
            .get(&coordinate)
            .expect("chunk must exist after insertion");
        self.chunk_store.track_chunk(inserted);
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
                self.chunk_store.untrack_chunk(&chunk);
            }
        }

        unloaded
    }

    /// Get the number of chunks currently loaded
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
