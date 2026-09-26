//! Smooth motion at any frame rate.
//!
//! Physics (and the server) move things 60 times a second, but screens draw
//! 144 or more. Drawing the newest positions each frame makes motion step
//! unevenly: some frames show a new step, some repeat the last one. Instead
//! we draw one update behind, blending between the last two updates, so
//! movement is smooth whatever the refresh rate (about 16 ms of delay).

use std::collections::HashMap;
use std::time::Instant;

use brixo_core::{DataModel, InstanceId, Vec3};

/// Where something is: its position, and (for players) which way it faces.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    position: Vec3,
    yaw: f32,
}

/// Anything that jumps further than this between updates was teleported
/// (respawned, moved by a script): snap it, don't slide it across the map.
const TELEPORT: f32 = 12.0;

pub struct Smoother {
    /// Recent updates: when each happened (on a steady clock, in seconds
    /// since `base`) and where everything was. Newest last.
    snaps: std::collections::VecDeque<(f64, HashMap<InstanceId, Pose>)>,
    /// How far apart updates arrive, learned as they come (about 1/60 s).
    period: f64,
    last_arrival: Option<f64>,
    base: Instant,
}

impl Default for Smoother {
    fn default() -> Self {
        Smoother { snaps: Default::default(), period: 1.0 / 60.0, last_arrival: None, base: Instant::now() }
    }
}

fn poses(world: &DataModel) -> HashMap<InstanceId, Pose> {
    let mut out = HashMap::new();
    for id in world.walk() {
        if let Some(p) = world.player(id) {
            out.insert(id, Pose { position: p.body.position, yaw: p.body.rotation.y });
        } else if let Some(p) = world.part(id) {
            out.insert(id, Pose { position: p.position, yaw: p.rotation.y });
        }
    }
    out
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
}

/// Blends angles the short way round (359 to 1 goes through 0).
fn lerp_degrees(a: f32, b: f32, t: f32) -> f32 {
    let d = (b - a + 540.0).rem_euclid(360.0) - 180.0;
    a + d * t
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

impl Smoother {
    /// Starts over (a new game: nothing to blend from).
    pub fn reset(&mut self) {
        *self = Smoother::default();
    }

    /// The world as it should be drawn right now: a copy of `world` with
    /// moving players and parts blended between their last two updates.
    pub fn view(&mut self, world: &DataModel) -> DataModel {
        self.view_at(world, Instant::now())
    }

    pub fn view_at(&mut self, world: &DataModel, now: Instant) -> DataModel {
        let now = now.saturating_duration_since(self.base).as_secs_f64();
        let latest = poses(world);
        if self.snaps.back().is_none_or(|(_, last)| *last != latest) {
            // A new update. Frames notice updates a little late, by up to a
            // frame, so don't trust the arrival time itself: updates come
            // evenly, so place this one a period after the last, as long
            // as that stays close to when it really arrived.
            if let Some(prev) = self.last_arrival {
                self.period = (self.period * 0.9 + (now - prev) * 0.1).clamp(1.0 / 240.0, 0.1);
            }
            self.last_arrival = Some(now);
            let stamp = match self.snaps.back() {
                Some((t, _)) => (t + self.period).clamp(now - 2.0 * self.period, now + 0.5 * self.period),
                None => now,
            };
            self.snaps.push_back((stamp, latest));
            if self.snaps.len() > 4 {
                self.snaps.pop_front();
            }
        }

        // Draw one period behind, between the two updates around then.
        let at = now - self.period;
        let mut view = world.clone();
        let pair = self.snaps.iter().zip(self.snaps.iter().skip(1)).find(|((_, _), (tb, _))| at <= *tb);
        let Some(((ta, a), (tb, b))) = pair else { return view }; // too new: draw the latest as-is
        let t = (((at - ta) / (tb - ta).max(1e-6)) as f32).clamp(0.0, 1.0);
        for (id, to) in b {
            let from = a.get(id).unwrap_or(to);
            let position = if distance(from.position, to.position) > TELEPORT {
                to.position
            } else {
                lerp(from.position, to.position, t)
            };
            if let Some(p) = view.player_mut(*id) {
                p.body.position = position;
                p.body.rotation.y = lerp_degrees(from.yaw, to.yaw, t);
            } else if let Some(p) = view.part_mut(*id) {
                p.position = position;
            }
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brixo_core::Class;
    use std::time::Duration;

    fn world_with_part(x: f32) -> (DataModel, InstanceId) {
        let mut dm = DataModel::new();
        let root = dm.root();
        let id = dm.create(Class::Part, "Mover", root).unwrap();
        dm.part_mut(id).unwrap().position = Vec3::new(x, 0.0, 0.0);
        (dm, id)
    }

    #[test]
    fn steady_60hz_updates_draw_smoothly_at_144hz() {
        // The part moves 1 stud per 60 Hz update; we draw at 144 Hz.
        let start = Instant::now();
        let mut s = Smoother::default();
        let mut drawn = Vec::new();
        for frame in 0..144 {
            let now = start + Duration::from_secs_f64(frame as f64 / 144.0);
            let updates = (frame as f64 / 144.0 * 60.0).floor();
            let (dm, id) = world_with_part(updates as f32);
            drawn.push(s.view_at(&dm, now).part(id).unwrap().position.x);
        }
        // Once warmed up, every frame moves forward by about the same
        // amount (60/144 of a stud): no frames stuck, no double jumps.
        let steps: Vec<f32> = drawn.windows(2).skip(10).map(|w| w[1] - w[0]).collect();
        let ideal = 60.0 / 144.0;
        for s in &steps {
            assert!((s - ideal).abs() < 0.12, "uneven step {s} (ideal {ideal}): {steps:?}");
        }
        // Without smoothing, over half of these frames would move 0 and
        // the rest a whole stud.
        assert!(steps.iter().all(|s| *s > 0.2), "a frame stalled: {steps:?}");
    }

    #[test]
    fn teleports_snap_instead_of_sliding() {
        // Moving along slowly, then respawned far away: every frame shows
        // it either near the start or at the new spot, never in between.
        let start = Instant::now();
        let mut s = Smoother::default();
        let mut shown = Vec::new();
        for frame in 0..40 {
            let now = start + Duration::from_secs_f64(frame as f64 / 144.0);
            let update = (frame as f64 / 144.0 * 60.0).floor() as i32;
            let x = if update < 8 { update as f32 * 0.5 } else { 500.0 };
            let (dm, id) = world_with_part(x);
            shown.push(s.view_at(&dm, now).part(id).unwrap().position.x);
        }
        assert!(shown.iter().all(|x| *x <= 4.0 || *x == 500.0), "slid across the map: {shown:?}");
        assert_eq!(*shown.last().unwrap(), 500.0, "and it does get there");
    }

    #[test]
    fn angles_blend_the_short_way_round() {
        assert!((lerp_degrees(350.0, 10.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((lerp_degrees(10.0, 350.0, 0.5) - 0.0).abs() < 1e-3);
    }
}
