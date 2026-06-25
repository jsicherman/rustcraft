mod builder;
mod debug;
mod mesher;
pub mod model;
mod overlay;
mod render;
mod texture;

#[derive(Debug, Clone, Copy)]
pub struct OverlayParticle {
	pub position: [f32; 3],
	pub radius: f32,
	pub color: [u8; 4],
}

pub use builder::{MeshBuildResult, VoxelMesher};
pub use debug::DebugOverlayData;
pub use render::{MeshGpu as Mesh, Renderer, init};
