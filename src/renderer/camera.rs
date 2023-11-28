use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::{Mat4, Vec2, Vec3};

#[derive(Debug)]
pub struct Camera {
    pub position: Vec3,
    fov_y_radians: f32,
    aspect_ratio: f32,
    /// counterclockwise rotation around the y (up) axis
    /// range [0..TAU)
    ccw_y_rot_radians: f32,
    /// positive is upwards, zero parallel to xz plane, negative downwards
    /// range [-`Camera::MAX_UP_DOWN`, `Camera::MAX_UP_DOWN`]
    up_down_radians: f32,
}

pub struct ComputedVectors {
    pub direction: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

impl Camera {
    pub const MAX_UP_DOWN: f32 = FRAC_PI_2 - 0.0001;
    pub const DEFAULT_FOV_Y: f32 = FRAC_PI_4;
    pub const Z_NEAR: f32 = 0.1f32;

    pub fn new(position: Vec3, fov_y_radians: f32) -> Self {
        Self {
            position,
            fov_y_radians,
            aspect_ratio: 1.0,
            ccw_y_rot_radians: 0.0,
            up_down_radians: 0.0,
        }
    }

    pub fn set_aspect_ratio(&mut self, width: u32, height: u32) {
        self.aspect_ratio = width as f32 / height as f32;
    }

    pub fn computed_vectors(&self) -> ComputedVectors {
        let direction = Vec3 {
            x: self.ccw_y_rot_radians.cos() * self.up_down_radians.cos(),
            y: self.up_down_radians.sin(),
            z: -self.ccw_y_rot_radians.sin() * self.up_down_radians.cos(),
        };
        debug_assert!(direction.is_normalized());
        let world_up = Vec3::Y;
        let right = direction.cross(world_up).normalize();
        let up = right.cross(direction).normalize();
        ComputedVectors {
            direction,
            right,
            up,
        }
    }

    pub fn turn_right(&mut self, radians: f32) {
        self.ccw_y_rot_radians -= radians;
        while self.ccw_y_rot_radians >= TAU {
            self.ccw_y_rot_radians -= TAU;
        }
        while self.ccw_y_rot_radians < 0.0 {
            self.ccw_y_rot_radians += TAU;
        }
    }

    pub fn turn_up(&mut self, radians: f32) {
        self.up_down_radians = (self.up_down_radians + radians)
            .max(-Self::MAX_UP_DOWN)
            .min(Self::MAX_UP_DOWN)
    }

    pub fn projection_view_matrix(&self) -> Mat4 {
        let projection = Mat4::perspective_rh(
            self.fov_y_radians,
            self.aspect_ratio,
            Camera::Z_NEAR,
            1000.0,
        );

        let vs = self.computed_vectors();
        let view = Mat4::look_to_rh(self.position, vs.direction, vs.up);

        projection * view
    }

    pub fn half_size_at_distance(&self, distance: f32) -> Vec2 {
        debug_assert!(distance > 0.0);

        // O..opposite, A..adjacent, H hypotenuse
        // tan(aplha) = O / A

        let height = (self.fov_y_radians / 2.0).tan() * distance;
        let width = self.aspect_ratio * height;
        Vec2::new(width, height)
    }
}
