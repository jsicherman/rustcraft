use std::array::IntoIter;

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

    pub fn build(self, cube_mesh: MeshHandle) -> ModelAsset {
        match self {
            Self::Humanoid => ModelAsset::humanoid().with_geometry(cube_mesh),
        }
    }
}
