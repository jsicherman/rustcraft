use serde::{Deserialize, Serialize};
use spatial::vectors::Vec3fGlobal;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ParticleEmitter {
    pub origin: Vec3fGlobal,
    pub normal: Vec3fGlobal,
    pub seed: u64,
    pub emission_rate: u16,
    pub emission_duration_ms: u16,
    pub particle_lifetime_ms: u16,
    pub max_particles: u16,
    pub initial_speed: f32,
    pub spread: f32,
    pub gravity: f32,
    pub size_start: f32,
    pub size_end: f32,
    pub color: [u8; 4],
}
