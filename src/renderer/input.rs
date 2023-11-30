use crate::renderer::camera::Camera;
use crate::renderer::gui::{ElementId, Gui};
use crate::simulation::position::ChunkPosition;
use crate::simulation::PlayerCommand;
use crate::timer::Timer;
use crate::worker::{MessageTag, Worker, WorkerId};
use glam::{DVec2, Vec3};
use log::info;
use std::mem;
use std::time::Duration;
use winit::event::{DeviceId, ElementState, KeyEvent, MouseButton, Touch, TouchPhase};
use winit::keyboard::Key;
use winit::window::Window;

#[derive(Default)]
pub struct Input {
    controller: PlayerController,
    fingers: Vec<Finger>,
    seconds_without_touch: f32,
}

#[derive(Default)]
pub struct PlayerController {
    forward: f32,
    left: f32,
    back: f32,
    right: f32,
    exploding: Option<(f32, Option<(DeviceId, u64)>)>,
    creating: Option<f32>,
}

struct Finger {
    id: (DeviceId, u64),
    normalized_previous_position: DVec2,
    total_distance: f64,
    start: Timer,
    action: FingerAction,
}

#[derive(Eq, PartialEq)]
enum FingerAction {
    ShortWorldTab,
    LongWorldTab,
    PlayerMovement,
    CameraMovement,
}

impl Finger {
    const LONG_TAP: Duration = Duration::from_millis(400);
    const CUTOFF: f64 = 0.05;
}

impl Input {
    pub fn start_of_frame(
        &mut self,
        worker: &impl Worker,
        simulation: WorkerId,
        player_chunk: ChunkPosition,
        camera: &mut Camera,
        gui: &Gui,
        delta_time: f32,
    ) {
        self.seconds_without_touch += delta_time;

        let mut movement = Vec3::ZERO;

        {
            // movement
            if let Some(index) = self
                .fingers
                .iter()
                .position(|f| f.action == FingerAction::PlayerMovement)
            {
                let finger = self.fingers[index].normalized_previous_position;
                let to_finger = gui.movement_element_to_finger(finger);
                let direction = camera.rotate_movement(to_finger).clamp_length_max(1.0);

                movement += direction;
            }

            // destruction
            if let Some(index) = self.fingers.iter().position(|f| {
                f.action == FingerAction::ShortWorldTab && f.start.elapsed() >= Finger::LONG_TAP
            }) {
                self.fingers[index].action = FingerAction::LongWorldTab;

                let second = self.fingers[index..]
                    .iter()
                    .position(|f| f.action == FingerAction::ShortWorldTab);

                let diameter = if let Some(second) = second {
                    self.fingers[second].action = FingerAction::LongWorldTab;
                    self.controller.exploding = Some((0.0, Some(self.fingers[index].id)));
                    -20
                } else {
                    -1
                };

                send_player_command(
                    worker,
                    simulation,
                    player_chunk,
                    diameter,
                    camera,
                    Some(self.fingers[index].normalized_previous_position),
                );
            }
        }

        {
            let vectors = camera.computed_vectors();

            let forward = self.controller.forward - self.controller.back;
            let right = self.controller.right - self.controller.left;

            let delta = vectors.direction * forward + vectors.right * right;
            movement += delta;

            let movement_speed = delta_time * 100.0;
            camera.position += movement * movement_speed;

            if let Some((accumulator, finger)) = &mut self.controller.exploding {
                *accumulator += delta_time;
                let time = 0.05;
                if *accumulator > time {
                    *accumulator -= time;

                    let location = finger.map(|it| {
                        self.fingers
                            .iter()
                            .find(|f| f.id == it)
                            .unwrap()
                            .normalized_previous_position
                    });
                    send_player_command(worker, simulation, player_chunk, -20, camera, location);
                }
            }
            if let Some(accumulator) = &mut self.controller.creating {
                *accumulator += delta_time;
                let time = 0.3;
                if *accumulator > time {
                    *accumulator -= time;
                    send_player_command(worker, simulation, player_chunk, 20, camera, None);
                }
            }
        }
    }

    pub fn keyboard(
        &mut self,
        event: KeyEvent,
        worker: &impl Worker,
        simulation: WorkerId,
        player_chunk: ChunkPosition,
        camera: &Camera,
        print_statistics: &mut bool,
    ) {
        if let Key::Character(str) = event.logical_key {
            let pressed = event.state.is_pressed();
            let amount = if pressed { 1.0 } else { 0.0 };
            match str.as_str() {
                "w" => self.controller.forward = amount,
                "a" => self.controller.left = amount,
                "s" => self.controller.back = amount,
                "d" => self.controller.right = amount,
                "p" => {
                    if pressed {
                        *print_statistics ^= true;
                        #[cfg(target_arch = "wasm32")]
                        if !*print_statistics {
                            crate::wasm::hide_statistics();
                        }
                    }
                }
                "q" => {
                    let accumulator = self.controller.exploding.map(|it| it.0);
                    if pressed && accumulator.is_none() {
                        send_player_command(worker, simulation, player_chunk, -20, camera, None);
                    }
                    self.controller.exploding =
                        pressed.then_some((accumulator.unwrap_or(-0.1), None));
                }
                "e" => {
                    let accumulator = self.controller.creating;
                    if pressed && accumulator.is_none() {
                        send_player_command(worker, simulation, player_chunk, 20, camera, None);
                    }
                    self.controller.creating = pressed.then_some(accumulator.unwrap_or(0.0));
                }
                _ => {}
            }
        }
    }

    pub fn mouse(
        &self,
        worker: &impl Worker,
        simulation: WorkerId,
        player_chunk: ChunkPosition,
        camera: &Camera,
        state: ElementState,
        button: MouseButton,
    ) {
        if state == ElementState::Pressed && button == MouseButton::Left {
            send_player_command(worker, simulation, player_chunk, -1, camera, None);
        }
        if state == ElementState::Pressed && button == MouseButton::Right {
            send_player_command(worker, simulation, player_chunk, 1, camera, None);
        }
    }

    pub fn mouse_motion(&self, camera: &mut Camera, delta_time: f32, delta: (f64, f64)) {
        let speed = delta_time * 0.1;
        camera.turn_right(delta.0 as f32 * speed);
        camera.turn_up(-delta.1 as f32 * speed);
    }

    pub fn touch(
        &mut self,
        window: &Window,
        Touch {
            device_id,
            phase,
            location,
            force: _,
            id,
        }: Touch,
        worker: &impl Worker,
        simulation: WorkerId,
        player_chunk: ChunkPosition,
        camera: &mut Camera,
        gui: &Gui,
        delta_time: f32,
    ) {
        self.seconds_without_touch = 0.0;

        let id = (device_id, id);

        let size = window.inner_size();

        // TODO is this correct?
        let location = DVec2::new(
            2.0 * location.x / size.width as f64 - 1.0,
            1.0 - 2.0 * location.y / size.height as f64,
        );
        // I was able to trigger these in the browser
        // debug_assert!(location.x.abs() <= 1.1);
        // debug_assert!(location.y.abs() <= 1.1);

        match phase {
            TouchPhase::Started => {
                let action = if let Some((element, _to_finger)) = gui.closest_element(location) {
                    match element.id {
                        ElementId::Movement => FingerAction::PlayerMovement,
                        ElementId::Center => FingerAction::ShortWorldTab,
                    }
                } else {
                    FingerAction::ShortWorldTab
                };

                self.fingers.push(Finger {
                    id,
                    normalized_previous_position: location,
                    total_distance: 0.0,
                    start: Timer::now(),
                    action,
                });
            }
            TouchPhase::Moved => {
                let position = self.fingers.iter().position(|f| f.id == id).unwrap();
                let finger = self.fingers.get_mut(position).unwrap();

                let old = mem::replace(&mut finger.normalized_previous_position, location);
                let dx = location.x - old.x;
                let dy = location.y - old.y;
                finger.total_distance += (dx * dx + dy * dy).sqrt();

                if finger.total_distance >= Finger::CUTOFF
                    && finger.action == FingerAction::ShortWorldTab
                {
                    finger.action = FingerAction::CameraMovement;
                }

                if finger.action == FingerAction::CameraMovement {
                    let speed = delta_time * 60.0;
                    camera.turn_right(-dx as f32 * speed);
                    camera.turn_up(-dy as f32 * speed);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let position = self.fingers.iter().position(|f| f.id == id).unwrap();
                let finger = self.fingers.remove(position);

                if self
                    .controller
                    .exploding
                    .map(|it| it.1 == Some(finger.id))
                    .unwrap_or(false)
                {
                    self.controller.exploding = None;
                }

                if finger.action == FingerAction::ShortWorldTab {
                    // TODO what if this is a long press?
                    let elapsed = finger.start.elapsed();
                    if elapsed < Finger::LONG_TAP {
                        send_player_command(
                            worker,
                            simulation,
                            player_chunk,
                            1,
                            camera,
                            Some(location),
                        );
                        // no need to mark finger as tapped because it removed from the list
                    }
                }
            }
        }
    }

    pub fn seconds_without_touch(&self) -> f32 {
        self.seconds_without_touch
    }
}

fn screen_to_world(camera: &Camera, finger: DVec2) -> (Vec3, Vec3) {
    let inverse = camera.projection_view_matrix().inverse();

    let world_position = inverse.project_point3(Vec3::new(finger.x as f32, finger.y as f32, 0.0));

    // from eye to near clipping plane
    let direction = world_position - camera.position;

    // in the center of the screen they should be equal, on the edges the new vector is longer
    debug_assert!(direction.length() >= Camera::Z_NEAR);

    (world_position, direction.normalize())
}

fn send_player_command(
    worker: &impl Worker,
    simulation: WorkerId,
    player_chunk: ChunkPosition,
    diameter: i32,
    camera: &Camera,
    touch_location: Option<DVec2>,
) {
    let (position, direction) = touch_location
        .map(|it| screen_to_world(camera, it))
        .unwrap_or_else(|| (camera.position, camera.computed_vectors().direction));

    let command = PlayerCommand {
        player_chunk: player_chunk.index().to_array(),
        position: position.to_array(),
        direction: direction.to_array(),
        diameter,
    };
    info!("send_player_command: {command:?}");

    let command_bytes = bytemuck::bytes_of(&command);
    let mut message_bytes = [0u8; mem::size_of::<PlayerCommand>() + 1];
    message_bytes[0..mem::size_of::<PlayerCommand>()].copy_from_slice(command_bytes);
    *message_bytes.last_mut().unwrap() = MessageTag::PlayerCommand as u8;

    let message = Box::<[u8]>::from(message_bytes);
    worker.send_message(simulation, message);
}
