use std::{
    ops::{
        Bound::{Excluded, Included, Unbounded},
        RangeBounds,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::vectors::{Vec3f, Vec3fGlobal, Vec3iGlobal, Vec4f};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    PlusX,
    MinusX,
    PlusY,
    MinusY,
    PlusZ,
    MinusZ,
}

impl From<Direction> for Vec3fGlobal {
    fn from(val: Direction) -> Self {
        Vec3iGlobal::from(val).into()
    }
}

impl From<Direction> for Vec3iGlobal {
    fn from(val: Direction) -> Self {
        match val {
            Direction::PlusX => Vec3iGlobal::from((1, 0, 0)),
            Direction::MinusX => Vec3iGlobal::from((-1, 0, 0)),
            Direction::PlusY => Vec3iGlobal::from((0, 1, 0)),
            Direction::MinusY => Vec3iGlobal::from((0, -1, 0)),
            Direction::PlusZ => Vec3iGlobal::from((0, 0, 1)),
            Direction::MinusZ => Vec3iGlobal::from((0, 0, -1)),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Orientation {
    yaw: f32,
    pitch: f32,
}

impl Orientation {
    pub fn new(yaw_deg: f32, pitch_deg: f32) -> Self {
        Self {
            yaw: yaw_deg.to_radians(),
            pitch: pitch_deg.to_radians(),
        }
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    pub fn yaw_pitch(&mut self, delta_yaw: f32, delta_pitch: f32) -> &mut Self {
        self.yaw += delta_yaw;
        self.pitch += delta_pitch;
        self
    }

    pub fn clamp<R1: RangeBounds<f32>, R2: RangeBounds<f32>>(
        &mut self,
        yaw_range: R1,
        pitch_range: R2,
    ) -> &mut Self {
        match yaw_range.start_bound() {
            Unbounded => {}
            Included(start_bound) | Excluded(start_bound) => {
                self.yaw = self.yaw.max(*start_bound);
            }
        }

        match yaw_range.end_bound() {
            Unbounded => {}
            Included(end_bound) | Excluded(end_bound) => {
                self.yaw = self.yaw.min(*end_bound);
            }
        }

        match pitch_range.start_bound() {
            Unbounded => {}
            Included(start_bound) | Excluded(start_bound) => {
                self.pitch = self.pitch.max(*start_bound);
            }
        }

        match pitch_range.end_bound() {
            Unbounded => {}
            Included(end_bound) | Excluded(end_bound) => {
                self.pitch = self.pitch.min(*end_bound);
            }
        }

        self
    }

    pub fn movement_offset(
        &self,
        velocity: f32,
        dt: Duration,
        forward: f32,
        strafe: f32,
    ) -> Vec3fGlobal {
        let (sin_yaw, cos_yaw) = self.yaw().sin_cos();

        let fwd = Vec3fGlobal::from((sin_yaw, 0.0, cos_yaw));
        let right = Vec3fGlobal::from((cos_yaw, 0.0, -sin_yaw));

        let mut direction = fwd * forward + right * strafe;

        if direction.length_sq() > 1.0 {
            direction = direction.normalize();
        }

        direction * velocity * dt.as_secs_f32()
    }

    pub fn look_direction(&self) -> Vec3fGlobal {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pitch.sin_cos();

        Vec3fGlobal::from((yaw_sin * pitch_cos, pitch_sin, yaw_cos * pitch_cos))
    }

    pub fn view_projection(&self, eye: Vec3fGlobal, aspect: f32) -> [Vec4f; 4] {
        let look_dir = self.look_direction();
        let center = eye + look_dir;

        let view = look_at(eye, center, Vec3f::UP);
        let proj = perspective(aspect, 60.0, 0.1, 1000.0);
        Vec4f::mat_mul(proj, view)
    }
}

fn look_at(eye: Vec3fGlobal, center: Vec3fGlobal, up: Vec3fGlobal) -> [Vec4f; 4] {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);

    [
        [s.x(), u.x(), -f.x(), 0.0].into(),
        [s.y(), u.y(), -f.y(), 0.0].into(),
        [s.z(), u.z(), -f.z(), 0.0].into(),
        [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0].into(),
    ]
}

fn perspective(aspect: f32, fov_y_deg: f32, near: f32, far: f32) -> [Vec4f; 4] {
    let f = 1.0 / (fov_y_deg.to_radians() / 2.0).tan();
    let range = near - far;

    [
        [f / aspect, 0.0, 0.0, 0.0].into(),
        [0.0, f, 0.0, 0.0].into(),
        [0.0, 0.0, (far + near) / range, -1.0].into(),
        [0.0, 0.0, (2.0 * far * near) / range, 0.0].into(),
    ]
}
