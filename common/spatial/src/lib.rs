pub mod aabb;
pub mod orientation;
pub mod vectors;

/// The world height
pub const WORLD_HEIGHT: usize = 256;
/// Sea level
pub const SEA_LEVEL: usize = 64;

/// Edge length of a chunk
pub const CHUNK_SIZE: usize = 16;
/// Number of blocks in a chunk
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
