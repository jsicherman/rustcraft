use spatial::{
    orientation::Orientation,
    vectors::{Vec3fGlobal, Vec4f},
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
    last_cursor_position: Option<(f64, f64)>,
    cursor_delta: (f64, f64),
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: 45.0,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
            eye_height: 1.62,
            last_cursor_position: None,
            cursor_delta: (0.0, 0.0),
        }
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_cursor_moved(&mut self, x: f64, y: f64) {
        if let Some((last_x, last_y)) = self.last_cursor_position {
            self.cursor_delta = (x - last_x, y - last_y);
        }
        self.last_cursor_position = Some((x, y));
    }

    pub fn get_cursor_delta(&mut self) -> (f64, f64) {
        let delta = self.cursor_delta;
        self.cursor_delta = (0.0, 0.0);
        delta
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

    pub fn view_projection(&self, position: Vec3fGlobal, orientation: Orientation) -> [Vec4f; 4] {
        let eye_position = position + [0.0, self.eye_height, 0.0].into();
        orientation.view_projection(eye_position, self.aspect, self.fov_y, self.near, self.far)
    }
}
