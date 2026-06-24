use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BlockId {
    Air = 0,
    Stone = 1,
    Grass = 2,
    Dirt = 3,
    Missing = 4,
}

impl BlockId {
    pub const MAX: usize = 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockType {
    name: BlockName,
    texture: BlockTexture,
    opacity: BlockOpacity,
    solidity: BlockSolidity,
    light_emission: BlockLightEmission,
    hardness: BlockHardness,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockName(&'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockTexture {
    Uniform(&'static Path),
    Directional([&'static Path; 6]),
}

impl BlockTexture {
    pub fn new(path: &'static Path) -> Self {
        Self::Uniform(path)
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
pub struct BlockOpacity(u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockSolidity(u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockLightEmission(u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockHardness(u8);

impl BlockSolidity {
    pub fn is_solid(&self) -> bool {
        self.0 > 0
    }
}

pub struct BlockRegistry {
    blocks: Vec<BlockType>,
}

impl BlockRegistry {
    pub fn load() -> Self {
        let blocks = vec![
            BlockType {
                name: BlockName("Air"),
                texture: BlockTexture::Uniform(Path::new("textures/missing.png")),
                opacity: BlockOpacity(0),
                solidity: BlockSolidity(0),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(0),
            },
            BlockType {
                name: BlockName("Stone"),
                texture: BlockTexture::Uniform(Path::new("textures/stone.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
            },
            BlockType {
                name: BlockName("Grass"),
                texture: BlockTexture::directional(
                    Path::new("textures/grass_top.png"),
                    Path::new("textures/dirt.png"),
                    Path::new("textures/grass_side.png"),
                    Path::new("textures/grass_side.png"),
                    Path::new("textures/grass_side.png"),
                    Path::new("textures/grass_side.png"),
                ),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
            },
            BlockType {
                name: BlockName("Dirt"),
                texture: BlockTexture::Uniform(Path::new("textures/dirt.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
            },
            BlockType {
                name: BlockName("Missing"),
                texture: BlockTexture::Uniform(Path::new("textures/steve_skin.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
            },
        ];

        assert_eq!(blocks.len(), BlockId::MAX);
        Self { blocks }
    }

    pub fn get_block_type(&self, id: BlockId) -> &BlockType {
        self.blocks.get(id as usize).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = &BlockType> {
        self.blocks.iter()
    }
}
