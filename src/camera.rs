use std::f32::consts::{PI, TAU};

use glam::Vec3;

#[derive(Debug)]
pub struct Camera {
    pub position: Vec3,
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
    pub const MAX_UP_DOWN: f32 = PI - 0.0001;

    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            ccw_y_rot_radians: 0.0,
            up_down_radians: 0.0,
        }
    }
    pub fn computed_vectors(&self) -> ComputedVectors {
        let direction = Vec3 {
            x: self.ccw_y_rot_radians.cos() * self.up_down_radians.cos(),
            y: self.up_down_radians.sin(),
            z: -self.ccw_y_rot_radians.sin() * self.up_down_radians.cos(),
        }.normalize();
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
        self.up_down_radians = (self.up_down_radians + radians).max(-Self::MAX_UP_DOWN).min(Self::MAX_UP_DOWN)
    }
}
