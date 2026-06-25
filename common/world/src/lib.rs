use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

/// 0.0 = midnight
/// 0.25 = sunrise (6am)
/// 0.5 = noon
/// 0.75 = sunset (6pm)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeOfDay(pub f32);

impl TimeOfDay {
    pub fn new(time: f32) -> Self {
        Self(time % 1.0)
    }

    /// Advance time (in fractional days)
    pub fn advance(&mut self, delta: f32) {
        self.0 = (self.0 + delta) % 1.0;
    }

    /// 24-hour time (0-24)
    pub fn to_hours(&self) -> f32 {
        self.0 * 24.0
    }

    /// Gets the sun position as a normalized direction vector
    pub fn sun_direction(&self) -> [f32; 3] {
        // t=0.25 -> sunrise at +X, t=0.5 -> zenith, t=0.75 -> sunset at -X.
        let angle = (self.0 - 0.25) * TAU;
        [angle.cos(), angle.sin(), 0.0]
    }
}
