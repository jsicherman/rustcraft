use std::path::Path;

use entity::EntityType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BlockId {
    Air = 0,
    Stone = 1,
    Grass = 2,
    Dirt = 3,
}

impl BlockId {
    pub const MAX: usize = 4;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TextureId {
    Missing = BlockId::MAX as u8,
    Head = BlockId::MAX as u8 + 1,
    LightPart = BlockId::MAX as u8 + 2,
    DarkPart = BlockId::MAX as u8 + 3,
}

impl TextureId {
    pub const MAX: usize = 4;
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

// FIXME: clean up
pub struct BlockRegistry {
    blocks: Vec<BlockType>,
    textures: Vec<BlockTexture>,
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
        ];

        assert_eq!(blocks.len(), BlockId::MAX);

        let textures = vec![
            BlockTexture::Uniform(Path::new("textures/missing.png")),
            BlockTexture::directional(
                Path::new("textures/dirt.png"),
                Path::new("textures/steve_shirt.png"),
                Path::new("textures/steve_skin.png"),
                Path::new("textures/stone.png"),
                Path::new("textures/stone.png"),
                Path::new("textures/stone.png"),
            ),
            BlockTexture::Uniform(Path::new("textures/steve_shirt.png")),
            BlockTexture::Uniform(Path::new("textures/steve_pants.png")),
        ];

        assert_eq!(textures.len(), TextureId::MAX);

        Self { blocks, textures }
    }

    pub fn get_textures(&self, entity: EntityType) -> &[TextureId] {
        match entity {
            EntityType::Human => &[
                TextureId::Head,
                TextureId::DarkPart,
                TextureId::LightPart,
                TextureId::LightPart,
                TextureId::LightPart,
                TextureId::LightPart,
            ],
        }
    }

    pub fn get_texture(&self, id: TextureId) -> &BlockTexture {
        self.textures.get(id as usize).unwrap()
    }

    pub fn get_block_type(&self, id: BlockId) -> &BlockType {
        self.blocks.get(id as usize).unwrap()
    }

    pub fn blocks(&self) -> impl Iterator<Item = &BlockType> {
        self.blocks.iter()
    }

    pub fn textures(&self) -> impl Iterator<Item = &BlockTexture> {
        self.textures.iter()
    }
}
