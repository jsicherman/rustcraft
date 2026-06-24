use std::array::IntoIter;

use block::TextureId;
use render::model::{MeshHandle, ModelAsset, ModelHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDefinition {
    Humanoid,
}

impl ModelDefinition {
    pub fn iter() -> IntoIter<Self, 1> {
        [Self::Humanoid].into_iter()
    }

    pub fn handle(self) -> ModelHandle {
        match self {
            Self::Humanoid => ModelHandle::from(0),
        }
    }

    pub fn height(self) -> f32 {
        match self {
            Self::Humanoid => 1.8,
        }
    }

    pub fn half_width(self) -> f32 {
        match self {
            Self::Humanoid => 0.4,
        }
    }

    pub fn build(self, cube_mesh: MeshHandle, textures: &[TextureId]) -> ModelAsset {
        match self {
            Self::Humanoid => ModelAsset::humanoid(textures).with_geometry(cube_mesh),
        }
    }
}
