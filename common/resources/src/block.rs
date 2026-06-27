use std::{ops::Deref, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u8);

impl BlockId {
    pub const MAX: u8 = 5;

    pub fn iter() -> impl Iterator<Item = BlockId> {
        (0..Self::MAX).map(BlockId)
    }
}

impl Deref for BlockId {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockType {
    pub(crate) name: BlockName,
    pub(crate) texture: BlockTexture,
    pub(crate) opacity: BlockOpacity,
    pub(crate) solidity: BlockSolidity,
    pub(crate) light_emission: BlockLightEmission,
    pub(crate) hardness: BlockHardness,
    pub(crate) dimensions: BlockDimensions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockDimensions(pub [BlockScale; 3]);

impl BlockDimensions {
    pub const FULL: Self = Self([BlockScale::FULL; 3]);
    pub const TOP_HALF: Self = Self([BlockScale::FULL, BlockScale::TOP_HALF, BlockScale::FULL]);
    pub const BOTTOM_HALF: Self =
        Self([BlockScale::FULL, BlockScale::BOTTOM_HALF, BlockScale::FULL]);
    pub const BOTTOM_QUARTER: Self = Self([
        BlockScale::FULL,
        BlockScale::BOTTOM_QUARTER,
        BlockScale::FULL,
    ]);
    pub const TOP_QUARTER: Self =
        Self([BlockScale::FULL, BlockScale::TOP_QUARTER, BlockScale::FULL]);

    pub fn size(&self) -> [f32; 3] {
        [
            self.0[0].size_f32(),
            self.0[1].size_f32(),
            self.0[2].size_f32(),
        ]
    }

    pub fn offset(&self) -> [f32; 3] {
        [
            self.0[0].offset_f32(),
            self.0[1].offset_f32(),
            self.0[2].offset_f32(),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockScale {
    /// Size within the voxel cell, in units of 1/4
    pub size: u8,
    /// Offset from the bottom of the voxel cell, in units of 1/4
    pub offset: u8,
}

impl From<BlockScale> for [f32; 2] {
    fn from(val: BlockScale) -> Self {
        [val.size_f32(), val.offset_f32()]
    }
}

impl BlockScale {
    pub const FULL: Self = Self { offset: 0, size: 4 };
    pub const BOTTOM_HALF: Self = Self { offset: 0, size: 2 };
    pub const TOP_HALF: Self = Self { offset: 2, size: 2 };
    pub const BOTTOM_QUARTER: Self = Self { offset: 0, size: 1 };
    pub const TOP_QUARTER: Self = Self { offset: 3, size: 1 };

    pub fn size_f32(self) -> f32 {
        self.size as f32 / 4.0
    }

    pub fn offset_f32(self) -> f32 {
        self.offset as f32 / 4.0
    }

    pub fn reaches_top(self) -> bool {
        self.offset + self.size == 4
    }

    pub fn reaches_bottom(self) -> bool {
        self.offset == 0
    }

    pub fn is_full(self) -> bool {
        self.size == 4
    }

    pub fn is_half(self) -> bool {
        self.size == 2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockSize {
    Full,
    Half,
    Quarter,
}

impl BlockType {
    pub fn name(&self) -> BlockName {
        self.name
    }

    pub fn texture(&self) -> &BlockTexture {
        &self.texture
    }

    pub fn opacity(&self) -> BlockOpacity {
        self.opacity
    }

    pub fn solidity(&self) -> BlockSolidity {
        self.solidity
    }

    pub fn light_emission(&self) -> BlockLightEmission {
        self.light_emission
    }

    pub fn hardness(&self) -> BlockHardness {
        self.hardness
    }

    pub fn dimensions(&self) -> BlockDimensions {
        self.dimensions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockName(pub(crate) &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockTexture {
    Uniform(&'static Path),
    Directional([&'static Path; 6]),
}

impl BlockTexture {
    pub fn new(path: &'static Path) -> Self {
        Self::Uniform(path)
    }

    pub fn to_array(&self) -> [&'static Path; 6] {
        match self {
            BlockTexture::Uniform(path) => [*path; 6],
            BlockTexture::Directional(paths) => *paths,
        }
    }

    pub fn directional(
        top: &'static Path,
        bottom: &'static Path,
        north: &'static Path,
        south: &'static Path,
        west: &'static Path,
        east: &'static Path,
    ) -> Self {
        Self::Directional([east, west, top, bottom, north, south])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockOpacity(pub(crate) u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockSolidity(pub(crate) u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockLightEmission(pub(crate) u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockHardness(pub(crate) u8);

impl BlockSolidity {
    pub fn is_solid(&self) -> bool {
        self.0 > 0
    }
}

impl BlockId {
    pub const AIR: Self = Self(0);
    pub const STONE: Self = Self(1);
    pub const GRASS: Self = Self(2);
    pub const DIRT: Self = Self(3);
    pub const STONE_SLAB: Self = Self(4);
}
