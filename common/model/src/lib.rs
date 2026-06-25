use std::array::IntoIter;

use block::TextureId;
use render::model::{MeshHandle, ModelAsset, ModelHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDefinition {
    Humanoid,
    Quadruped,
    Snake,
    Bird,
    Giant,
    Slime,
    Spider,
}

impl ModelDefinition {
    pub fn iter() -> IntoIter<Self, 7> {
        [
            Self::Humanoid,
            Self::Snake,
            Self::Bird,
            Self::Quadruped,
            Self::Giant,
            Self::Slime,
            Self::Spider,
        ]
        .into_iter()
    }

    pub fn handle(self) -> ModelHandle {
        match self {
            Self::Humanoid => ModelHandle::from(0),
            Self::Bird => ModelHandle::from(1),
            Self::Quadruped => ModelHandle::from(2),
            Self::Snake => ModelHandle::from(3),
            Self::Giant => ModelHandle::from(4),
            Self::Slime => ModelHandle::from(5),
            Self::Spider => ModelHandle::from(6),
        }
    }

    pub fn eye_height(self) -> f32 {
        match self {
            Self::Humanoid => 1.68,
            Self::Bird => 0.8,
            Self::Quadruped => 0.9,
            Self::Snake => 0.2,
            Self::Giant => 5.5,
            Self::Slime => 0.8,
            Self::Spider => 0.3,
        }
    }

    pub fn height(self) -> f32 {
        match self {
            Self::Humanoid => 1.8,
            Self::Bird => 1.1,
            Self::Quadruped => 1.15,
            Self::Snake => 0.3,
            Self::Giant => 5.9,
            Self::Slime => 1.0,
            Self::Spider => 0.4,
        }
    }

    pub fn half_width(self) -> f32 {
        match self {
            Self::Humanoid => 0.4,
            Self::Bird => 0.4,
            Self::Quadruped => 0.55,
            Self::Snake => 0.3,
            Self::Giant => 1.6,
            Self::Slime => 0.42,
            Self::Spider => 0.6,
        }
    }

    pub fn build(self, cube_mesh: MeshHandle, textures: &[TextureId]) -> ModelAsset {
        match self {
            Self::Humanoid => ModelAsset::humanoid(textures).with_geometry(cube_mesh),
            Self::Bird => ModelAsset::bird(textures).with_geometry(cube_mesh),
            Self::Quadruped => ModelAsset::quadruped(textures).with_geometry(cube_mesh),
            Self::Snake => ModelAsset::snake(textures).with_geometry(cube_mesh),
            Self::Giant => ModelAsset::giant(textures).with_geometry(cube_mesh),
            Self::Slime => ModelAsset::slime(textures).with_geometry(cube_mesh),
            Self::Spider => ModelAsset::spider(textures).with_geometry(cube_mesh),
        }
    }
}
