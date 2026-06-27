use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

/// 0.0 = midnight
/// 0.25 = sunrise (6am)
/// 0.5 = noon
/// 0.75 = sunset (6pm)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeOfDay {
    tick: u64,
    ticks_per_day: u64,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            tick: 12000,
            ticks_per_day: 24000,
        }
    }
}

impl TimeOfDay {
    pub fn new(tick: u64, ticks_per_day: u64) -> Self {
        Self {
            tick,
            ticks_per_day,
        }
    }

    /// Advance time (in fractional days)
    pub fn advance(&mut self, ticks: u64) {
        self.tick = (self.tick + ticks) % self.ticks_per_day;
    }

    /// 24-hour time (0-24)
    pub fn to_hours(&self) -> f32 {
        (self.tick as f32 / self.ticks_per_day as f32) * 24.0
    }

    pub fn to_fraction(&self) -> f32 {
        self.tick as f32 / self.ticks_per_day as f32
    }

    /// Gets the sun position as a normalized direction vector
    pub fn sun_direction(&self) -> [f32; 3] {
        // t=0.25 -> sunrise at +X, t=0.5 -> zenith, t=0.75 -> sunset at -X.
        let angle = (self.to_fraction() - 0.25) * TAU;
        [angle.cos(), angle.sin(), 0.0]
    }
}
