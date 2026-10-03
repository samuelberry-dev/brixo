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
    /// A part's whole rotation (players only turn).
    rotation: Vec3,
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
            out.insert(id, Pose { position: p.body.position, yaw: p.body.rotation.y, rotation: p.body.rotation });
        } else if let Some(p) = world.part(id) {
            out.insert(id, Pose { position: p.position, yaw: p.rotation.y, rotation: p.rotation });
        }
    }
    out
}

/// Tools in hand ride with their holders as they're drawn. The game puts
/// a held tool in its holder's hand; but what's drawn is the holder
/// smoothed, or (you) predicted ahead of the game, so the tool, drawn where
/// the game last put it, trails behind. This keeps each held tool's parts
/// exactly where they sit on their holder in `source` (the game's latest),
/// moved to where the holder is drawn in `view`.
pub fn hold_tools(view: &mut DataModel, source: &DataModel) {
    use glam::{EulerRot, Quat, Vec3 as G};
    let turn = |r: Vec3| Quat::from_euler(EulerRot::YXZ, r.y.to_radians(), r.x.to_radians(), r.z.to_radians());
    let holders: Vec<(InstanceId, InstanceId)> =
        source.walk().into_iter().filter_map(|id| Some((id, source.player(id)?.equipped?))).collect();
    for (holder, tool) in holders {
        let (Some(from), Some(to)) = (source.player(holder), view.player(holder)) else { continue };
        let (fp, tp) = (from.body.position, to.body.position);
        let (fq, tq) = (Quat::from_rotation_y(from.body.rotation.y.to_radians()), Quat::from_rotation_y(to.body.rotation.y.to_radians()));
        if fp == tp && fq == tq {
            continue;
        }
        let inv = fq.inverse();
        for id in source.parts_under(tool) {
            let Some(q) = source.part(id) else { continue };
            let local = inv * (G::new(q.position.x, q.position.y, q.position.z) - G::new(fp.x, fp.y, fp.z));
            let at = G::new(tp.x, tp.y, tp.z) + tq * local;
            let (y, x, z) = (tq * inv * turn(q.rotation)).to_euler(EulerRot::YXZ);
            if let Some(v) = view.part_mut(id) {
                v.position = Vec3::new(at.x, at.y, at.z);
                v.rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
            }
        }
    }
}

/// Your own tool swing, shown the moment you click instead of a ping
/// later: draws your arm (and the tool in it) at `swing` seconds left of a
/// swing, whatever the server has. The hit itself is still the server's.
/// Call after `hold_tools`.
pub fn predict_swing(view: &mut DataModel, me: InstanceId, swing: f32) {
    use glam::{EulerRot, Quat, Vec3 as G};
    let Some(p) = view.player(me).copied() else { return };
    let Some(tool) = p.equipped else { return };
    let up = brixo_core::holds_up(view, tool);
    let delta = brixo_core::held_arm_angle(swing, up) - brixo_core::held_arm_angle(p.swing, up);
    if let Some(v) = view.player_mut(me) {
        v.swing = swing;
    }
    if delta.abs() < 1e-4 {
        return;
    }
    // Turn the tool about the shoulder by the difference in arm angle.
    let body = Quat::from_rotation_y(p.body.rotation.y.to_radians());
    let s = brixo_core::RIGHT_SHOULDER;
    let shoulder = G::new(p.body.position.x, p.body.position.y, p.body.position.z) + body * G::new(s.x, s.y, s.z);
    let turn = body * Quat::from_rotation_x(delta) * body.inverse();
    let euler = |r: Vec3| Quat::from_euler(EulerRot::YXZ, r.y.to_radians(), r.x.to_radians(), r.z.to_radians());
    for id in view.parts_under(tool) {
        let Some(q) = view.part_mut(id) else { continue };
        let at = shoulder + turn * (G::new(q.position.x, q.position.y, q.position.z) - shoulder);
        let (y, x, z) = (turn * euler(q.rotation)).to_euler(EulerRot::YXZ);
        q.position = Vec3::new(at.x, at.y, at.z);
        q.rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    }
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
            let jumped = distance(from.position, to.position) > TELEPORT;
            let position = if jumped { to.position } else { lerp(from.position, to.position, t) };
            if let Some(p) = view.player_mut(*id) {
                p.body.position = position;
                p.body.rotation.y = lerp_degrees(from.yaw, to.yaw, t);
            } else if let Some(p) = view.part_mut(*id) {
                p.position = position;
                // Turning smoothly too (a kart going round a bend).
                if !jumped && from.rotation != to.rotation {
                    p.rotation = Vec3::new(
                        lerp_degrees(from.rotation.x, to.rotation.x, t),
                        lerp_degrees(from.rotation.y, to.rotation.y, t),
                        lerp_degrees(from.rotation.z, to.rotation.z, t),
                    );
                }
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

    #[test]
    fn a_tool_in_hand_stays_in_hand_when_the_holder_is_drawn_elsewhere() {
        use brixo_core::Class;
        let mut dm = DataModel::new();
        let root = dm.root();
        let me = dm.create(Class::Player, "Me", root).unwrap();
        let tool = dm.create(Class::Tool, "Sword", me).unwrap();
        let blade = dm.create(Class::Part, "Blade", tool).unwrap();
        dm.player_mut(me).unwrap().equipped = Some(tool);
        dm.player_mut(me).unwrap().body.position = Vec3::new(0.0, 3.0, 0.0);
        // In the right hand, a stud ahead.
        dm.part_mut(blade).unwrap().position = Vec3::new(1.5, 3.0, 1.0);
        // Drawn (predicted) 10 studs on and turned to face +X.
        let mut view = dm.clone();
        view.player_mut(me).unwrap().body.position = Vec3::new(10.0, 3.0, 0.0);
        view.player_mut(me).unwrap().body.rotation.y = 90.0;
        hold_tools(&mut view, &dm);
        let at = view.part(blade).unwrap().position;
        // Turned 90 degrees: a stud ahead is now +X, the right hand is -Z.
        assert!((at.x - 11.0).abs() < 1e-3 && (at.y - 3.0).abs() < 1e-3 && (at.z + 1.5).abs() < 1e-3, "{at:?}");
        assert!((view.part(blade).unwrap().rotation.y - 90.0).abs() < 1e-3);
        // Not held: left alone.
        dm.player_mut(me).unwrap().equipped = None;
        let mut view2 = dm.clone();
        view2.player_mut(me).unwrap().body.position = Vec3::new(10.0, 3.0, 0.0);
        hold_tools(&mut view2, &dm);
        assert_eq!(view2.part(blade).unwrap().position, Vec3::new(1.5, 3.0, 1.0));
    }
}
