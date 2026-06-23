use spatial::{
    orientation::Orientation,
    vectors::{Vec3fGlobal, Vec4f},
};

pub struct Camera {
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
    eye_height: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: 45.0,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
            eye_height: 1.62,
        }
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    pub fn set_eye_height(&mut self, eye_height: f32) {
        self.eye_height = eye_height;
    }

    pub fn view_projection(&self, position: Vec3fGlobal, orientation: Orientation) -> [Vec4f; 4] {
        let eye_position = position + [0.0, self.eye_height, 0.0].into();
        orientation.view_projection(eye_position, self.aspect)
    }
}
