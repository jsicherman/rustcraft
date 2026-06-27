mod block;
mod builder;
mod camera;
mod lighting;
mod math;
mod mesher;
pub mod model;
mod overlay;
mod render;
mod shader;
mod texture;

#[derive(Debug, Clone, Copy)]
pub struct OverlayParticle {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [u8; 4],
}

pub use block::cube;
pub use builder::{MeshBuildResult, VoxelMesher};
pub use mesher::MeshGpu as Mesh;
pub use overlay::DebugOverlayData;
pub use render::{Renderer, init};
