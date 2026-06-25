use std::{ops::Deref, path::Path};

use entity::EntityType;
use serde::{Deserialize, Serialize};

pub const REACH_DISTANCE: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u8);

impl BlockId {
    pub const MAX: u8 = 4;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureId(pub u8);

impl Deref for TextureId {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TextureId {
    pub const MAX: u8 = 3;
    pub const OFFSET: u8 = BlockId::MAX;

    pub fn iter() -> impl Iterator<Item = TextureId> {
        (0..Self::MAX).map(|id| TextureId(id + Self::OFFSET))
    }
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

pub struct TexturePack {
    blocks: Vec<BlockType>,
    textures: Vec<BlockTexture>,
}

impl BlockId {
    pub const AIR: Self = Self(0);
    pub const STONE: Self = Self(1);
    pub const GRASS: Self = Self(2);
    pub const DIRT: Self = Self(3);
}

impl TexturePack {
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

        assert_eq!(blocks.len(), BlockId::MAX as usize);

        let textures = vec![
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

        assert_eq!(textures.len(), TextureId::MAX as usize);

        Self { blocks, textures }
    }

    pub fn get_textures(&self, entity: EntityType) -> &[TextureId] {
        match entity {
            EntityType::Human => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(2),
                TextureId(2),
            ],
            EntityType::Horse => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(1),
            ],
            EntityType::Snake => &[TextureId(0), TextureId(1), TextureId(2), TextureId(1)],
            EntityType::Bird => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(0),
                TextureId(0),
                TextureId(1),
                TextureId(2),
            ],
            EntityType::Giant => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(1),
                TextureId(3),
                TextureId(2),
            ],
            EntityType::Slime => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(3),
                TextureId(2),
                TextureId(1),
            ],
            EntityType::Spider => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(3),
                TextureId(2),
                TextureId(1),
            ],
        }
    }

    pub fn get_texture(&self, id: TextureId) -> &BlockTexture {
        self.textures.get(*id as usize).unwrap()
    }

    pub fn get_block_type(&self, id: BlockId) -> &BlockType {
        self.blocks.get(*id as usize).unwrap()
    }

    pub fn blocks(&self) -> impl Iterator<Item = &BlockType> {
        self.blocks.iter()
    }

    pub fn textures(&self) -> impl Iterator<Item = &BlockTexture> {
        self.textures.iter()
    }
}
