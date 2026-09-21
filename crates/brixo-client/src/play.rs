//! Controls and camera while playing.

use brixo_core::{CameraMode, DataModel, InstanceId};
use brixo_render::Camera;
use brixo_runtime::PlayerInput;
use glam::Vec3;

/// Closest the third-person camera gets before switching to first person.
pub const MIN_FOLLOW_DISTANCE: f32 = 6.0;
const FARTHEST: f32 = 60.0;
/// Where the camera sits in first person, above the character's centre.
const EYE_HEIGHT: f32 = 1.8;
/// What the third-person camera looks at, above the character's centre.
const LOOK_HEIGHT: f32 = 1.5;

/// Which movement keys are down.
#[derive(Debug, Clone, Copy, Default)]
pub struct Held {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

/// WASD relative to where the camera faces, flattened onto the ground.
pub fn movement_input(camera: &Camera, held: Held) -> PlayerInput {
    let forward = Vec3::new(camera.yaw.cos(), 0.0, camera.yaw.sin());
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let mut dir = Vec3::ZERO;
    if held.forward {
        dir += forward;
    }
    if held.back {
        dir -= forward;
    }
    if held.right {
        dir += right;
    }
    if held.left {
        dir -= right;
    }
    let dir = dir.normalize_or_zero();
    PlayerInput { move_x: dir.x, move_z: dir.z, jump: held.jump }
}

/// The camera that follows your character. The game picks the mode; in the
/// default mode the wheel zooms between third person and, all the way in,
/// first person.
#[derive(Debug, Clone, Copy)]
pub struct FollowCamera {
    /// 0 means first person.
    pub distance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self { distance: 16.0 }
    }
}

impl FollowCamera {
    /// `scroll` is positive when zooming in.
    pub fn zoom(&mut self, scroll: f32) {
        if scroll == 0.0 {
            return;
        }
        self.distance = if self.distance == 0.0 {
            if scroll < 0.0 { MIN_FOLLOW_DISTANCE } else { 0.0 }
        } else {
            let next = self.distance - scroll * 0.05;
            if next < MIN_FOLLOW_DISTANCE - 1.0 { 0.0 } else { next.clamp(MIN_FOLLOW_DISTANCE, FARTHEST) }
        };
    }

    /// Puts the camera behind (or inside) your character `me` in `world`,
    /// a local game's world or a server's copy. Returns the player to hide
    /// (your own character, in first person).
    pub fn update(&self, camera: &mut Camera, world: &DataModel, me: Option<InstanceId>) -> Option<InstanceId> {
        let id = me?;
        let player = world.player(id)?;
        let center = Vec3::new(player.body.position.x, player.body.position.y, player.body.position.z);
        let mode = player.camera_mode;
        let first_person = match mode {
            CameraMode::FirstPerson => true,
            CameraMode::ThirdPerson => false,
            CameraMode::Default => self.distance == 0.0,
        };
        if first_person {
            camera.position = center + Vec3::Y * EYE_HEIGHT;
            Some(id)
        } else {
            let distance = self.distance.max(MIN_FOLLOW_DISTANCE);
            camera.position = center + Vec3::Y * LOOK_HEIGHT - camera.forward() * distance;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera_facing(yaw: f32) -> Camera {
        let mut c = Camera::new();
        c.yaw = yaw;
        c
    }

    #[test]
    fn forward_follows_the_camera() {
        let c = camera_facing(-std::f32::consts::FRAC_PI_2); // looking toward -Z
        let i = movement_input(&c, Held { forward: true, ..Default::default() });
        assert!(i.move_x.abs() < 1e-5 && (i.move_z + 1.0).abs() < 1e-5);
        let i = movement_input(&c, Held { forward: true, right: true, ..Default::default() });
        assert!(((i.move_x * i.move_x + i.move_z * i.move_z) - 1.0).abs() < 1e-5, "diagonals aren't faster");
    }

    #[test]
    fn zooming_all_the_way_in_is_first_person_and_back_out_again() {
        let mut f = FollowCamera::default();
        for _ in 0..20 {
            f.zoom(50.0);
        }
        assert_eq!(f.distance, 0.0);
        f.zoom(50.0);
        assert_eq!(f.distance, 0.0, "stays in first person");
        f.zoom(-50.0);
        assert_eq!(f.distance, MIN_FOLLOW_DISTANCE);
        for _ in 0..100 {
            f.zoom(-50.0);
        }
        assert_eq!(f.distance, FARTHEST);
    }

    #[test]
    fn the_camera_follows_the_player_and_respects_the_game_mode() {
        let mut dm = brixo_core::DataModel::new();
        let root = dm.root();
        let floor = dm.create(brixo_core::Class::Part, "Floor", root).unwrap();
        dm.part_mut(floor).unwrap().size = brixo_core::Vec3::new(50.0, 1.0, 50.0);
        let mut game = brixo_runtime::Game::start(dm);
        for _ in 0..30 {
            game.step(1.0 / 60.0);
        }
        let mut cam = Camera::new();
        let follow = FollowCamera::default();
        let me = game.player_id();
        assert_eq!(follow.update(&mut cam, &game.world(), me), None);
        let player = game.player_position().unwrap();
        assert!((cam.position.distance(player + Vec3::Y * LOOK_HEIGHT) - 16.0).abs() < 1e-3);

        let id = game.player_id().unwrap();
        game.world().player_mut(id).unwrap().camera_mode = CameraMode::FirstPerson;
        assert_eq!(follow.update(&mut cam, &game.world(), me), Some(id), "own avatar hidden");
        assert!((cam.position - (player + Vec3::Y * EYE_HEIGHT)).length() < 1e-3);
    }
}
