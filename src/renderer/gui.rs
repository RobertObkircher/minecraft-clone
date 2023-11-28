use crate::renderer::camera::Camera;
use crate::simulation::chunk::Block;
use glam::Vec2;

pub struct Gui {
    pub half_size: Vec2,
    pub distance: f32,
    pub elements: Vec<UiElement>,
}

pub enum Id {
    Movement,
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
        let size = 0.5 * half_size.min_element();

        let center = Vec2::new(-half_size.x + size, -half_size.y + size);

        let movement = UiElement {
            id: Id::Movement,
            center,
            size,
            block: Block::Dirt,
        };

        Self {
            distance,
            half_size,
            elements: vec![movement],
        }
    }
}
