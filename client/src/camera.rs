use std::{f32::consts::TAU, time::Duration};

use ecs::eye_position;
use resources::entity::ModelDefinition;
use spatial::{
    orientation::{Orientation, perspective},
    vectors::{Global, Vec3fGlobal, Vec4f},
};

// Rads per pixel
pub const MOUSE_SENSITIVITY: f64 = 0.005;
pub const INVERT_YAW: f64 = -1.0;
pub const INVERT_PITCH: f64 = -1.0;

pub struct Camera {
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
    eye_height: f32,
    view_bob: ViewBob,
    last_cursor_position: Option<(f64, f64)>,
    cursor_delta: (f64, f64),
}

pub struct ViewBob {
    phase: f32,
    amplitude: f32,
    frequency: f32,
}

impl Default for ViewBob {
    fn default() -> Self {
        Self {
            phase: 0.0,
            amplitude: 0.03,
            frequency: 1.8,
        }
    }
}

impl ViewBob {
    pub fn update(&mut self, horizontal_speed: f32, dt: Duration) {
        let dt = dt.as_secs_f32();

        self.phase += dt * self.frequency * horizontal_speed;

        let target_amplitude = 1.0 - (-horizontal_speed * 0.1).exp();

        let blend = 1.0 - (-10.0 * dt).exp();
        self.amplitude += (target_amplitude - self.amplitude) * blend;
    }

    pub fn y_offset(&self) -> f32 {
        (self.phase * TAU).sin() * self.amplitude
    }

    pub fn roll_offset(&self) -> f32 {
        (self.phase * TAU).cos() * self.amplitude * 0.2
    }
}

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: Vec3fGlobal,
    d: f32,
}

impl Plane {
    fn from_raw(raw: Vec4f<Global>) -> Option<Self> {
        let len = raw.length();
        if len == 0.0 {
            return None;
        }

        Some(Self {
            normal: ([raw[0] / len, raw[1] / len, raw[2] / len]).into(),
            d: raw[3] / len,
        })
    }

    fn distance_to_point(&self, p: Vec3fGlobal) -> f32 {
        self.d + self.normal.dot(p)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    pub fn from_view_projection(view_proj: [Vec4f<Global>; 4]) -> Self {
        let rows = [
            [
                view_proj[0][0],
                view_proj[1][0],
                view_proj[2][0],
                view_proj[3][0],
            ],
            [
                view_proj[0][1],
                view_proj[1][1],
                view_proj[2][1],
                view_proj[3][1],
            ],
            [
                view_proj[0][2],
                view_proj[1][2],
                view_proj[2][2],
                view_proj[3][2],
            ],
            [
                view_proj[0][3],
                view_proj[1][3],
                view_proj[2][3],
                view_proj[3][3],
            ],
        ]
        .map(std::convert::Into::into);

        let make_plane = |a: Vec4f<Global>, b: Vec4f<Global>| {
            Plane::from_raw(a + b).unwrap_or(Plane {
                normal: [0.0, 0.0, 1.0].into(),
                d: 0.0,
            })
        };

        Self {
            planes: [
                // left
                make_plane(rows[3], rows[0]),
                // right
                make_plane(
                    rows[3],
                    [-rows[0][0], -rows[0][1], -rows[0][2], -rows[0][3]].into(),
                ),
                // bottom
                make_plane(rows[3], rows[1]),
                // top
                make_plane(
                    rows[3],
                    [-rows[1][0], -rows[1][1], -rows[1][2], -rows[1][3]].into(),
                ),
                // near
                make_plane(rows[3], rows[2]),
                // far
                make_plane(
                    rows[3],
                    [-rows[2][0], -rows[2][1], -rows[2][2], -rows[2][3]].into(),
                ),
            ],
        }
    }

    pub fn intersects_sphere(&self, center: Vec3fGlobal, radius: f32) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.distance_to_point(center) >= -radius)
    }

    pub fn intersects_aabb(&self, min: Vec3fGlobal, max: Vec3fGlobal) -> bool {
        self.planes.iter().all(|plane| {
            let px = if plane.normal[0] >= 0.0 {
                max[0]
            } else {
                min[0]
            };
            let py = if plane.normal[1] >= 0.0 {
                max[1]
            } else {
                min[1]
            };
            let pz = if plane.normal[2] >= 0.0 {
                max[2]
            } else {
                min[2]
            };

            plane.distance_to_point([px, py, pz].into()) >= 0.0
        })
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: 45.0,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
            eye_height: ModelDefinition::Humanoid.eye_height(),
            view_bob: ViewBob::default(),
            last_cursor_position: None,
            cursor_delta: (0.0, 0.0),
        }
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, horizontal_speed: f32, dt: Duration) {
        self.view_bob.update(horizontal_speed, dt);
    }

    pub fn without_view_bobbing(mut self) -> Self {
        self.view_bob = ViewBob {
            phase: 0.0,
            amplitude: 0.0,
            frequency: 0.0,
        };
        self
    }

    pub fn handle_cursor_moved(&mut self, dx: f64, dy: f64) {
        self.cursor_delta.0 += dx;
        self.cursor_delta.1 += dy;
    }

    pub fn get_cursor_delta(&mut self) -> (f64, f64) {
        let delta = self.cursor_delta;
        self.cursor_delta = (0.0, 0.0);
        delta
    }

    pub fn reset_cursor_delta(&mut self) {
        self.cursor_delta = (0.0, 0.0);
    }

    pub fn reset_cursor_position(&mut self, x: f64, y: f64) {
        self.last_cursor_position = Some((x, y));
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    pub fn set_eye_height(&mut self, eye_height: f32) {
        self.eye_height = eye_height;
    }

    pub fn get_eye_position(&self, position: Vec3fGlobal) -> Vec3fGlobal {
        eye_position(position, self.eye_height)
    }

    pub fn view_projection(&self, position: Vec3fGlobal, orientation: Orientation) -> [Vec4f; 4] {
        let eye_position =
            self.get_eye_position(position) + [0.0, self.view_bob.y_offset(), 0.0].into();

        let view = orientation.view_matrix(eye_position);
        let proj = perspective(self.aspect, self.fov_y, self.near, self.far);

        let (rs, rc) = self.view_bob.roll_offset().sin_cos();
        let roll: [Vec4f; 4] = [
            [rc, rs, 0.0, 0.0].into(),
            [-rs, rc, 0.0, 0.0].into(),
            [0.0, 0.0, 1.0, 0.0].into(),
            [0.0, 0.0, 0.0, 1.0].into(),
        ];

        Vec4f::mat_mul(Vec4f::mat_mul(proj, roll), view)
    }

    pub fn skybox_view_projection(&self, orientation: Orientation) -> [Vec4f; 4] {
        orientation.view_projection(
            Vec3fGlobal::ZERO,
            self.aspect,
            self.fov_y,
            self.near,
            self.far,
        )
    }

    pub fn frustum(&self, position: Vec3fGlobal, orientation: Orientation) -> Frustum {
        Frustum::from_view_projection(self.view_projection(position, orientation))
    }
}
