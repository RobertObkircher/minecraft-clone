use crate::renderer::camera::Camera;
use crate::simulation::chunk::Block;
use glam::{DVec2, Vec2};

pub struct Gui {
    pub half_size: Vec2,
    pub distance: f32,
    pub elements: Vec<UiElement>,
}

#[derive(Eq, PartialEq)]
pub enum ElementId {
    Movement,
    Center,
}

pub struct UiElement {
    pub id: ElementId,
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
            id: ElementId::Movement,
            center,
            size,
            block: Block::Button,
        };

        let mut elements = vec![movement];

        let size = 0.01 * half_size.min_element();
        for i in -2..=2 {
            elements.push(UiElement {
                id: ElementId::Center,
                center: Vec2::X * 2.0 * size * i as f32,
                size,
                block: Block::Button,
            });
            elements.push(UiElement {
                id: ElementId::Center,
                center: Vec2::Y * 2.0 * size * i as f32,
                size,
                block: Block::Button,
            });
        }

        Self {
            distance,
            half_size,
            elements,
        }
    }

    pub fn closest_element(&self, finger: DVec2) -> Option<(&UiElement, Vec2)> {
        let mut distance = f32::INFINITY;
        let mut to_finger = Vec2::ZERO;
        let mut closest = None;
        for e in self.elements.iter() {
            let to_f = finger.as_vec2() - e.center / self.half_size;
            let d = to_f.length_squared();
            if d < distance {
                distance = d;
                to_finger = to_f;
                closest = Some(e);
            }
        }

        let size = 2.0 * self.half_size;
        closest
            .filter(|e| to_finger.x.abs() < e.size / size.x && to_finger.y.abs() < e.size / size.y)
            .map(|e| (e, to_finger))
    }
    pub fn movement_element_to_finger(&self, finger: DVec2) -> Vec2 {
        let element = self
            .elements
            .iter()
            .filter(|it| it.id == ElementId::Movement)
            .next()
            .unwrap();

        let size = element.size / (2.0 * self.half_size);

        let to_finger = finger.as_vec2() - element.center / self.half_size;
        to_finger / size
    }
}
