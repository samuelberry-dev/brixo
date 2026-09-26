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
/// Closer than this (pulled in by a wall), your own character is hidden.
const CLOSE_UP: f32 = 2.5;
/// What the third-person camera looks at, above the character's centre.
const LOOK_HEIGHT: f32 = 1.5;

/// A projection for labels over parts: world point to screen, or None if
/// it's behind the camera or more than 150 studs away.
pub fn projector(camera: &Camera, screen: egui::Rect) -> impl Fn(Vec3) -> Option<egui::Pos2> + '_ {
    let vp = camera.view_proj(screen.width() / screen.height().max(1.0));
    move |p: Vec3| {
        if p.distance(camera.position) > 150.0 {
            return None;
        }
        let c = vp * p.extend(1.0);
        if c.w <= 0.1 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(egui::pos2(
            screen.left() + (n.x + 1.0) / 2.0 * screen.width(),
            screen.top() + (1.0 - n.y) / 2.0 * screen.height(),
        ))
    }
}

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
    /// How far back the camera actually was last frame, and when: walls
    /// pull it in instantly, and it eases back out from there.
    reach: Option<(f32, std::time::Instant)>,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self { distance: 16.0, reach: None }
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

    /// True when the camera is really in first person (zoomed all the way
    /// in, or the game says so), not just pulled close by a wall.
    pub fn first_person(&self, world: &DataModel, me: Option<InstanceId>) -> bool {
        match me.and_then(|m| world.player(m)).map(|p| p.camera_mode) {
            Some(CameraMode::FirstPerson) => true,
            Some(CameraMode::ThirdPerson) | None => false,
            Some(CameraMode::Default) => self.distance == 0.0,
        }
    }

    /// Puts the camera behind (or inside) your character `me` in `world`,
    /// a local game's world or a server's copy. Returns the player to hide
    /// (your own character, in first person).
    pub fn update(&mut self, camera: &mut Camera, world: &DataModel, me: Option<InstanceId>) -> Option<InstanceId> {
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
            let target = center + Vec3::Y * LOOK_HEIGHT;
            let back = -camera.forward();
            // Don't go through walls: stop just in front of the first solid
            // thing between the character and the camera.
            let wanted = first_hit(world, target, back, distance).map(|t| (t - 0.6).max(0.5)).unwrap_or(distance);
            // Snap in front of a wall at once (never see through it), but
            // glide back out once it's out of the way, instead of popping.
            let now = std::time::Instant::now();
            let reach = match self.reach {
                Some((last, at)) if wanted > last => {
                    let dt = (now - at).as_secs_f32().min(0.1);
                    (last + (wanted - last) * (1.0 - (-8.0 * dt).exp())).min(wanted)
                }
                _ => wanted,
            };
            self.reach = Some((reach, now));
            camera.position = target + back * reach;
            // Pushed right up against your own character: hide it, like
            // first person, rather than filling the screen with its head.
            (reach < CLOSE_UP).then_some(id)
        }
    }
}

/// How far along a ray the first solid, visible part is (within `max`).
/// Tools and see-through or non-colliding parts don't block the camera.
pub fn first_hit(world: &DataModel, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for id in world.walk() {
        let Some(p) = world.part(id) else { continue };
        if !p.can_collide || p.transparency > 0.5 || world.tool_of(id).is_some() {
            continue;
        }
        let q = glam::Quat::from_euler(
            glam::EulerRot::YXZ,
            p.rotation.y.to_radians(),
            p.rotation.x.to_radians(),
            p.rotation.z.to_radians(),
        );
        let inv = q.inverse();
        let o = inv * (origin - Vec3::new(p.position.x, p.position.y, p.position.z));
        let d = inv * dir;
        let h = Vec3::new(p.size.x, p.size.y, p.size.z) / 2.0;
        // Slab test in the part's own space.
        let (mut t0, mut t1) = (0.0f32, max);
        let mut hit = true;
        for axis in 0..3 {
            if d[axis].abs() < 1e-6 {
                if o[axis].abs() > h[axis] {
                    hit = false;
                    break;
                }
            } else {
                let a = (-h[axis] - o[axis]) / d[axis];
                let b = (h[axis] - o[axis]) / d[axis];
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
                if t0 > t1 {
                    hit = false;
                    break;
                }
            }
        }
        // Starting inside a part (t0 == 0) doesn't count: that's the floor
        // the character stands in, or similar.
        if hit && t0 > 0.0 && best.is_none_or(|b| t0 < b) {
            best = Some(t0);
        }
    }
    best
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
    fn camera_rays_stop_at_walls_but_not_see_through_parts() {
        let mut dm = brixo_core::DataModel::new();
        let root = dm.root();
        let wall = dm.create(brixo_core::Class::Part, "Wall", root).unwrap();
        {
            let p = dm.part_mut(wall).unwrap();
            p.position = brixo_core::Vec3::new(0.0, 0.0, -5.0);
            p.size = brixo_core::Vec3::new(10.0, 10.0, 1.0);
        }
        let hit = first_hit(&dm, Vec3::ZERO, -Vec3::Z, 20.0).unwrap();
        assert!((hit - 4.5).abs() < 1e-4, "hit the near face: {hit}");
        dm.part_mut(wall).unwrap().transparency = 1.0;
        assert_eq!(first_hit(&dm, Vec3::ZERO, -Vec3::Z, 20.0), None);
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
        let mut follow = FollowCamera::default();
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
