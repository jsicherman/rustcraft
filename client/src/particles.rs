use std::{collections::VecDeque, time::Duration};

use protocol::particles::ParticleEmitter;
use render::OverlayParticle;
use simulation::particles::{ParticleSample, sample_emitter};
use spatial::vectors::Vec3fGlobal;

const MAX_EMITTERS: usize = 128;
const MAX_PARTICLES_PER_EMITTER: usize = 96;
const MAX_OVERLAY_PARTICLES: usize = 1024;
const PARTICLE_CULL_DISTANCE_SQ: f32 = 64.0 * 64.0;

struct ActiveEmitter {
    emitter: ParticleEmitter,
    spawn_time_ms: u64,
    death_time_ms: u64,
}

#[derive(Default)]
pub struct ParticleSystem {
    time_ms: u64,
    emitters: VecDeque<ActiveEmitter>,
    sampled_particles: Vec<ParticleSample>,
    overlay_particles: Vec<OverlayParticle>,
}

impl ParticleSystem {
    pub fn tick(&mut self, dt: Duration) {
        self.time_ms = self.time_ms.saturating_add(dt.as_millis() as u64);

        self.emitters
            .retain(|emitter| self.time_ms <= emitter.death_time_ms);
    }

    pub fn spawn(&mut self, emitter: ParticleEmitter) {
        if self.emitters.len() >= MAX_EMITTERS {
            self.emitters.pop_front();
        }

        let death_time_ms = self
            .time_ms
            .saturating_add(emitter.emission_duration_ms as u64)
            .saturating_add(emitter.particle_lifetime_ms as u64)
            .saturating_add(16);

        self.emitters.push_back(ActiveEmitter {
            emitter,
            spawn_time_ms: self.time_ms,
            death_time_ms,
        });
    }

    pub fn collect_overlay_particles(
        &mut self,
        camera_position: Vec3fGlobal,
    ) -> &[OverlayParticle] {
        self.sampled_particles.clear();
        self.overlay_particles.clear();

        for active in &self.emitters {
            let elapsed_ms = self.time_ms.saturating_sub(active.spawn_time_ms);
            let remaining_budget =
                MAX_OVERLAY_PARTICLES.saturating_sub(self.sampled_particles.len());
            let sample_budget = remaining_budget.min(MAX_PARTICLES_PER_EMITTER);
            if sample_budget == 0 {
                break;
            }

            let max_samples = self.sampled_particles.len() + sample_budget;

            sample_emitter(
                &active.emitter,
                elapsed_ms,
                &mut self.sampled_particles,
                max_samples,
            );

            if self.sampled_particles.len() >= MAX_OVERLAY_PARTICLES {
                break;
            }
        }

        for sample in &self.sampled_particles {
            if self.overlay_particles.len() >= MAX_OVERLAY_PARTICLES {
                break;
            }

            if (sample.position - camera_position).length_sq() > PARTICLE_CULL_DISTANCE_SQ {
                continue;
            }

            self.overlay_particles.push(OverlayParticle {
                position: sample.position.into(),
                radius: sample.size,
                color: sample.color,
            });
        }

        &self.overlay_particles
    }
}
