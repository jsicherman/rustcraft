use std::collections::HashMap;

use resources::block::BlockId;
use serde::{Deserialize, Serialize};
use spatial::{WORLD_HEIGHT, vectors::Vec2iChunk};

use crate::{
    Chunk, ChunkSection, SECTION_COUNT,
    block_entity::{BlockEntityData, NO_ENTITY},
    packed::PackedIndices,
    store::ChunkStore,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WireChunk {
    coordinate: Vec2iChunk,
    sections: Vec<(WireSection, u16)>,

    block_entities: Vec<(u16, BlockEntityData)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum WireSection {
    Homogeneous(BlockId),
    Palette {
        palette: Vec<BlockId>,
        indices: PackedIndices,
    },
    Raw(Vec<BlockId>),
}

impl WireSection {
    fn palette_size(palette_len: usize, indices_bytes: usize) -> usize {
        // tag(1) + palette_len(2) + palette entries + bits_per_index(1) + indices_len(4) + indices
        1 + 2 + palette_len + 1 + 4 + indices_bytes
    }

    fn raw_size() -> usize {
        // tag(1) + one byte per block
        1 + Chunk::CHUNK_VOLUME
    }

    #[allow(unused)]
    fn homogeneous_size() -> usize {
        // tag(1) + block id(1)
        2
    }
}

impl From<&ChunkSection> for WireSection {
    fn from(section: &ChunkSection) -> Self {
        match section {
            ChunkSection::Homogeneous(id) => WireSection::Homogeneous(*id),

            ChunkSection::Palette { palette, indices } => {
                let palette_size =
                    WireSection::palette_size(palette.len(), indices.as_bytes().len());

                if palette_size <= WireSection::raw_size() {
                    WireSection::Palette {
                        palette: palette.clone(),
                        indices: indices.clone(),
                    }
                } else {
                    WireSection::Raw(
                        (0..Chunk::CHUNK_VOLUME)
                            .map(|i| palette[indices.get(i) as usize])
                            .collect(),
                    )
                }
            }

            ChunkSection::Heterogeneous(blocks) => {
                // Try to build a palette first
                let mut palette = Vec::new();
                let mut palette_index_by_id = HashMap::new();
                let mut index_list = Vec::with_capacity(Chunk::CHUNK_VOLUME);

                for &id in blocks.iter() {
                    let idx = match palette_index_by_id.get(&id) {
                        Some(&i) => i,
                        None => {
                            let i = palette.len() as u16;
                            palette.push(id);
                            palette_index_by_id.insert(id, i);
                            i
                        }
                    };
                    index_list.push(idx);
                }

                if palette.len() == 1 {
                    return WireSection::Homogeneous(palette[0]);
                }

                let packed = PackedIndices::from_indices(&index_list, palette.len());
                let palette_size =
                    WireSection::palette_size(palette.len(), packed.as_bytes().len());

                if palette_size <= WireSection::raw_size() {
                    WireSection::Palette {
                        palette,
                        indices: packed,
                    }
                } else {
                    WireSection::Raw(blocks.to_vec())
                }
            }
        }
    }
}

impl From<WireSection> for ChunkSection {
    fn from(wire: WireSection) -> Self {
        match wire {
            WireSection::Homogeneous(id) => ChunkSection::Homogeneous(id),
            WireSection::Palette { palette, indices } => ChunkSection::Palette { palette, indices },
            WireSection::Raw(blocks) => {
                let arr: Box<[BlockId; Chunk::CHUNK_VOLUME]> =
                    blocks.into_boxed_slice().try_into().unwrap();
                ChunkSection::Heterogeneous(arr)
            }
        }
    }
}

impl WireChunk {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();

        out.extend_from_slice(&self.coordinate.x().to_le_bytes());
        out.extend_from_slice(&self.coordinate.z().to_le_bytes());
        out.extend_from_slice(&(self.sections.len() as u16).to_le_bytes());

        for (section, count) in &self.sections {
            out.extend_from_slice(&count.to_le_bytes());
            match section {
                WireSection::Homogeneous(id) => {
                    out.push(0);
                    out.push(**id);
                }
                WireSection::Palette { palette, indices } => {
                    out.push(1);
                    out.extend_from_slice(&(palette.len() as u16).to_le_bytes());
                    for id in palette {
                        out.push(**id);
                    }
                    out.push(indices.bits_per_index());
                    out.extend_from_slice(&(indices.len() as u32).to_le_bytes());
                    out.extend_from_slice(indices.as_bytes());
                }
                WireSection::Raw(blocks) => {
                    out.push(2);
                    for &id in blocks {
                        out.push(*id);
                    }
                }
            }
        }

        out.extend_from_slice(&(self.block_entities.len() as u16).to_le_bytes());
        for (block_idx, data) in self.block_entities {
            out.extend_from_slice(&block_idx.to_le_bytes());

            bincode::serde::encode_into_slice(data, &mut out, bincode::config::standard()).unwrap();
        }

        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut cur = 0;

        macro_rules! read {
            ($n:expr) => {{
                if cur + $n > bytes.len() {
                    return Err("unexpected end of input");
                }
                let slice = &bytes[cur..cur + $n];
                cur += $n;
                slice
            }};
        }
        macro_rules! read_u8 {
            () => {
                read!(1)[0]
            };
        }
        macro_rules! read_u16 {
            () => {
                u16::from_le_bytes(read!(2).try_into().unwrap())
            };
        }
        macro_rules! read_u32 {
            () => {
                u32::from_le_bytes(read!(4).try_into().unwrap())
            };
        }
        macro_rules! read_i32 {
            () => {
                i32::from_le_bytes(read!(4).try_into().unwrap())
            };
        }

        let x = read_i32!();
        let z = read_i32!();
        let coordinate = Vec2iChunk::from([x, z]);

        let section_count = read_u16!() as usize;
        let mut sections = Vec::with_capacity(section_count);

        for _ in 0..section_count {
            let count = read_u16!();
            let tag = read_u8!();

            let section = match tag {
                0 => WireSection::Homogeneous(BlockId(read_u8!())),
                1 => {
                    let palette_len = read_u16!() as usize;
                    if palette_len == 0 {
                        return Err("empty palette");
                    }
                    let mut palette = Vec::with_capacity(palette_len);
                    for _ in 0..palette_len {
                        palette.push(BlockId(read_u8!()));
                    }
                    let bits_per_index = read_u8!();
                    let indices_len = read_u32!() as usize;
                    if indices_len != Chunk::CHUNK_VOLUME {
                        return Err("wrong indices length");
                    }
                    let packed_len = PackedIndices::packed_len(indices_len, bits_per_index);
                    let data = read!(packed_len).to_vec();
                    let indices = PackedIndices::from_parts(bits_per_index, indices_len, data)
                        .map_err(|_| "invalid packed indices")?;
                    let max = (palette_len - 1) as u16;
                    for i in 0..indices_len {
                        if indices.get(i) > max {
                            return Err("palette index out of bounds");
                        }
                    }
                    WireSection::Palette { palette, indices }
                }
                2 => {
                    let mut blocks = Vec::with_capacity(Chunk::CHUNK_VOLUME);
                    for _ in 0..Chunk::CHUNK_VOLUME {
                        blocks.push(BlockId(read_u8!()));
                    }
                    WireSection::Raw(blocks)
                }
                _ => return Err("unknown section tag"),
            };

            sections.push((section, count));
        }

        let entity_count = read_u16!() as usize;
        let mut block_entities = Vec::with_capacity(entity_count);
        for _ in 0..entity_count {
            let block_idx = read_u16!();

            let Ok((data, bytes_read)) = bincode::serde::decode_from_slice::<BlockEntityData, _>(
                &bytes[cur..],
                bincode::config::standard(),
            ) else {
                return Err("failed to decode block entity data");
            };

            cur += bytes_read;
            block_entities.push((block_idx, data));
        }

        if cur != bytes.len() {
            return Err("trailing bytes");
        }

        Ok(WireChunk {
            coordinate,
            sections,
            block_entities,
        })
    }
}

impl WireChunk {
    pub fn coordinate(&self) -> Vec2iChunk {
        self.coordinate
    }

    pub fn from_chunk(chunk: &Chunk, store: &ChunkStore) -> Option<Self> {
        let mut rle = Vec::new();

        for &hash in chunk.section_hashes() {
            let section = store.load_no_cache(hash)?;
            let wire = WireSection::from(section.as_ref());

            match rle.last_mut() {
                Some((last, count)) if *last == wire && *count < u16::MAX => {
                    *count += 1;
                }
                _ => rle.push((wire, 1)),
            }
        }

        let block_entities = chunk
            .entity_index
            .iter()
            .enumerate()
            .filter(|(_, slot)| **slot != NO_ENTITY)
            .map(|(block_idx, &slot)| {
                (
                    block_idx as u16,
                    chunk.block_entities[slot as usize].clone(),
                )
            })
            .collect();

        Some(WireChunk {
            coordinate: chunk.coordinate(),
            sections: rle,
            block_entities,
        })
    }

    pub fn into_chunk(self, store: &mut ChunkStore) -> Chunk {
        const EXPECTED_SLICES: usize = WORLD_HEIGHT / Chunk::CHUNK_SIZE;

        let total: usize = self.sections.iter().map(|(_, n)| *n as usize).sum();

        assert_eq!(total, EXPECTED_SLICES, "RLE section count mismatch");

        let mut slices = [0; EXPECTED_SLICES];
        let mut idx = 0;

        for (wire_section, count) in self.sections {
            let section: ChunkSection = wire_section.into();
            let hash = store.insert(&section);
            for slot in slices.iter_mut().skip(idx).take(count as usize) {
                *slot = hash;
            }
            idx += count as usize;
        }

        let mut entity_index = [NO_ENTITY; SECTION_COUNT];
        let mut block_entities = Vec::with_capacity(self.block_entities.len());

        for (block_idx, data) in self.block_entities {
            entity_index[block_idx as usize] = block_entities.len() as u16;
            block_entities.push(data);
        }

        Chunk {
            coordinate: self.coordinate,
            slices,
            entity_index,
            block_entities,
        }
    }
}
