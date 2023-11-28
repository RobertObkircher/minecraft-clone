use crate::renderer::camera::Camera;
use crate::simulation::chunk::Block;
use glam::Vec2;

pub struct Gui {
    pub half_size: Vec2,
    pub distance: f32,
    pub elements: Vec<UiElement>,
}

pub enum Id {
    Forward,
    Left,
    Backward,
    Right,
}

pub struct UiElement {
    pub id: Id,
    pub center: Vec2,
    pub size: f32,
    pub block: Block,
}

impl Gui {
    pub fn for_camera(camera: &Camera) -> Gui {
        let distance = Camera::Z_NEAR + 10.0;
        let half_size = camera.half_size_at_distance(distance);

        dbg!(half_size);
        let size = 0.3 * half_size.min_element();
        let gap = 1.1;

        let center = Vec2::new(-half_size.x + 2.0 * size * gap, -half_size.y + size);

        let forward = UiElement {
            id: Id::Forward,
            center: center + Vec2::Y * size * gap,
            size,
            block: Block::Dirt,
        };
        let left = UiElement {
            id: Id::Left,
            center: center - Vec2::X * size * gap,
            size,
            block: Block::Dirt,
        };
        let backward = UiElement {
            id: Id::Backward,
            center,
            size,
            block: Block::Dirt,
        };
        let right = UiElement {
            id: Id::Right,
            center: center + Vec2::X * size * gap,
            size,
            block: Block::Dirt,
        };

        Self {
            distance,
            half_size,
            elements: vec![forward, left, backward, right],
        }
    }
}
