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

#[derive(Debug, Clone, Copy)]
pub struct ParticleSample {
	pub position: Vec3fGlobal,
	pub size: f32,
	pub color: [u8; 4],
}

fn hash_u64(mut x: u64) -> u64 {
	x ^= x >> 33;
	x = x.wrapping_mul(0xff51afd7ed558ccd);
	x ^= x >> 33;
	x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
	x ^ (x >> 33)
}

fn rand01(seed: u64) -> f32 {
	let bits = (hash_u64(seed) >> 40) as u32;
	bits as f32 / ((1u32 << 24) - 1) as f32
}

fn normalize(v: Vec3fGlobal) -> Vec3fGlobal {
	let len_sq = v.dot(v);
	if len_sq <= f32::EPSILON {
		Vec3fGlobal::new(0.0, 1.0, 0.0)
	} else {
		v * len_sq.sqrt().recip()
	}
}

fn axis_basis(normal: Vec3fGlobal) -> (Vec3fGlobal, Vec3fGlobal, Vec3fGlobal) {
	let n = normalize(normal);
	let helper = if n[1].abs() < 0.95 {
		Vec3fGlobal::new(0.0, 1.0, 0.0)
	} else {
		Vec3fGlobal::new(1.0, 0.0, 0.0)
	};
	let tangent = normalize(helper.cross(n));
	let bitangent = n.cross(tangent);
	(n, tangent, bitangent)
}

fn direction_for_particle(emitter: &ParticleEmitter, index: u64) -> Vec3fGlobal {
	let (n, tangent, bitangent) = axis_basis(emitter.normal);
	let r1 = rand01(emitter.seed ^ (index.wrapping_mul(0x9E3779B185EBCA87)));
	let r2 = rand01(emitter.seed ^ (index.wrapping_mul(0xD6E8FEB86659FD93)));

	let theta = r1 * std::f32::consts::TAU;
	let radial = emitter.spread.clamp(0.0, 1.5) * r2.sqrt();

	let local = Vec3fGlobal::new(radial * theta.cos(), radial * theta.sin(), 1.0 - radial);
	normalize(tangent * local[0] + bitangent * local[1] + n * local[2].max(0.1))
}

pub fn sample_emitter(
	emitter: &ParticleEmitter,
	elapsed_ms: u64,
	out: &mut Vec<ParticleSample>,
	max_samples: usize,
) {
	let emission_rate = emitter.emission_rate.max(1) as u64;
	let duration_ms = emitter.emission_duration_ms as u64;
	let life_ms = emitter.particle_lifetime_ms.max(1) as u64;

	let emitted_by_time = ((elapsed_ms.min(duration_ms) * emission_rate) / 1000) as usize;
	let emitted_cap = emitter.max_particles as usize;
	let emitted = emitted_by_time.min(emitted_cap);

	if emitted == 0 {
		return;
	}

	for index in 0..emitted {
		if out.len() >= max_samples {
			break;
		}

		let spawn_ms = (index as u64 * 1000) / emission_rate;
		if elapsed_ms < spawn_ms {
			continue;
		}

		let age_ms = elapsed_ms - spawn_ms;
		if age_ms >= life_ms {
			continue;
		}

		let age_t = age_ms as f32 / 1000.0;
		let life_t = age_ms as f32 / life_ms as f32;

		let direction = direction_for_particle(emitter, index as u64);
		let velocity = direction * emitter.initial_speed.max(0.0);
		let gravity = Vec3fGlobal::new(0.0, emitter.gravity, 0.0);

		let position = emitter.origin + velocity * age_t + gravity * (0.5 * age_t * age_t);

		let size = emitter.size_start + (emitter.size_end - emitter.size_start) * life_t;
		let alpha = ((1.0 - life_t).clamp(0.0, 1.0) * emitter.color[3] as f32) as u8;

		out.push(ParticleSample {
			position,
			size,
			color: [emitter.color[0], emitter.color[1], emitter.color[2], alpha],
		});
	}
}
