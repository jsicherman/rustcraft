use std::path::Path;

use crate::{
    block::{
        BlockDimensions, BlockHardness, BlockId, BlockLightEmission, BlockName, BlockOpacity,
        BlockSolidity, BlockTexture, BlockType,
    },
    texture::TextureId,
};

pub mod block;
pub mod entity;
pub mod texture;

pub struct ResourcePack {
    blocks: Vec<BlockType>,
    textures: Vec<BlockTexture>,
}

impl ResourcePack {
    pub fn load() -> Self {
        let blocks = vec![
            BlockType {
                name: BlockName("Air"),
                texture: BlockTexture::Uniform(Path::new("textures/missing.png")),
                opacity: BlockOpacity(0),
                solidity: BlockSolidity(0),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(0),
                dimensions: BlockDimensions::FULL,
            },
            BlockType {
                name: BlockName("Stone"),
                texture: BlockTexture::Uniform(Path::new("textures/stone.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
                dimensions: BlockDimensions::FULL,
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
                dimensions: BlockDimensions::FULL,
            },
            BlockType {
                name: BlockName("Dirt"),
                texture: BlockTexture::Uniform(Path::new("textures/dirt.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
                dimensions: BlockDimensions::FULL,
            },
            BlockType {
                name: BlockName("Stone Slab"),
                texture: BlockTexture::Uniform(Path::new("textures/stone.png")),
                opacity: BlockOpacity(255),
                solidity: BlockSolidity(255),
                light_emission: BlockLightEmission(0),
                hardness: BlockHardness(255),
                dimensions: BlockDimensions::TOP_HALF,
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

    pub fn get_texture(&self, id: TextureId) -> &BlockTexture {
        self.textures.get(*id as usize).unwrap()
    }

    pub fn get_block_type(&self, id: BlockId) -> &BlockType {
        self.blocks.get(*id as usize).unwrap()
    }

    pub fn texture_resources(&self) -> Vec<[&'static Path; 6]> {
        self.textures
            .iter()
            .map(|texture| texture.to_array())
            .collect()
    }

    pub fn block_resources(&self) -> Vec<([&'static Path; 6], [[f32; 2]; 3])> {
        self.blocks
            .iter()
            .map(|block| {
                (
                    block.texture.to_array(),
                    block.dimensions.0.map(|scale| scale.into()),
                )
            })
            .collect()
    }
}
