//! Karts on screen: drift sparks at the back wheels (pale, then blue, then
//! orange as a drift charges), a flame out the back while boosting, the
//! engine's note, and a speed dial while you drive. All worked out from
//! what the kart's Model says (speed, drift, boosting), so every player
//! sees every kart the same way.

use brixo_core::{is_kart, kart_chassis, Attribute, DataModel, InstanceId};
use egui::Color32;
use glam::Vec3;


fn num(world: &DataModel, id: InstanceId, key: &str) -> f32 {
    match world.get(id).and_then(|i| i.attributes.get(key)) {
        Some(Attribute::Num(n)) => *n as f32,
        _ => 0.0,
    }
}

fn flag(world: &DataModel, id: InstanceId, key: &str) -> bool {
    matches!(world.get(id).and_then(|i| i.attributes.get(key)), Some(Attribute::Bool(true)))
}

fn v(p: brixo_core::Vec3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// A little random number, the same for the same inputs.
fn hash(a: f32, b: f32) -> f32 {
    ((a * 12.9898 + b * 78.233).sin() * 43758.547).fract().abs()
}

/// The colour of drift sparks for a charge level (0.5 starting, 1 mini, 2 big).
pub fn spark_color(level: f32) -> Color32 {
    if level >= 2.0 {
        Color32::from_rgb(255, 140, 30)
    } else if level >= 1.0 {
        Color32::from_rgb(70, 170, 255)
    } else {
        Color32::from_rgb(255, 245, 190)
    }
}

/// Sparks and flames for every kart on screen. `eye` is the camera's
/// position (things further away are drawn smaller); `time` animates them.
pub fn draw_effects(ctx: &egui::Context, world: &DataModel, eye: Vec3, project: &dyn Fn(Vec3) -> Option<egui::Pos2>, time: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("kart fx")));
    for kart in world.walk().into_iter().filter(|id| is_kart(world, *id)) {
        let drift = num(world, kart, "drift");
        let boosting = flag(world, kart, "boosting");
        if drift <= 0.0 && !boosting {
            continue;
        }
        let Some(chassis) = kart_chassis(world, kart).and_then(|c| world.part(c)) else { continue };
        let q = glam::Quat::from_euler(glam::EulerRot::YXZ, chassis.rotation.y.to_radians(), chassis.rotation.x.to_radians(), chassis.rotation.z.to_radians());
        let back = q * Vec3::NEG_Z;
        let seed = kart.raw() as f32;
        if drift > 0.0 {
            let color = spark_color(drift);
            let wheels: Vec<Vec3> = world
                .get(kart)
                .map(|k| k.children.clone())
                .unwrap_or_default()
                .into_iter()
                .filter(|c| world.get(*c).is_some_and(|i| i.name.starts_with("Wheel")))
                .filter_map(|c| world.part(c).map(|p| v(p.position) - Vec3::Y * p.size.x.max(p.size.y).max(p.size.z) * 0.45))
                .collect();
            for (w, at) in wheels.iter().enumerate() {
                for k in 0..7 {
                    let t = (time * 9.0 + k as f32 / 7.0 + hash(seed, w as f32)).fract();
                    let spread = Vec3::new(hash(k as f32, t.floor() + seed) - 0.5, 0.0, hash(t.floor() + 3.0, k as f32 + seed) - 0.5);
                    let p = *at + back * (t * 2.2) + spread * 1.6 + Vec3::Y * (t * (1.0 - t) * 3.0);
                    let Some(s) = project(p) else { continue };
                    let r = (18.0 / p.distance(eye).max(1.0)).clamp(0.8, 5.0) * (1.0 - t * 0.6) * if drift >= 1.0 { 1.3 } else { 0.9 };
                    painter.circle_filled(s, r, color.gamma_multiply(1.0 - t * 0.7));
                }
            }
        }
        if boosting {
            let tail = v(chassis.position) + back * (chassis.size.z / 2.0 + 0.6) + Vec3::Y * 0.2;
            for k in 0..10 {
                let t = (time * 12.0 + k as f32 / 10.0).fract();
                let p = tail + back * (t * 3.0) + Vec3::new(hash(k as f32, seed) - 0.5, hash(seed, k as f32) - 0.5, 0.0) * 0.6 * t;
                let Some(s) = project(p) else { continue };
                let r = (30.0 / p.distance(eye).max(1.0)).clamp(1.0, 9.0) * (1.0 - t * 0.7);
                let c = if t < 0.3 { Color32::from_rgb(255, 250, 200) } else if t < 0.6 { Color32::from_rgb(255, 170, 40) } else { Color32::from_rgb(230, 70, 20) };
                painter.circle_filled(s, r, c.gamma_multiply(1.0 - t * 0.6));
            }
        }
    }
}

/// The kart `me` is driving, if any.
pub fn driving(world: &DataModel, me: Option<InstanceId>) -> Option<InstanceId> {
    world.player(me?)?.kart.filter(|k| is_kart(world, *k))
}

/// The engine's note for the kart you drive (None when not driving).
pub fn engine_pitch(world: &DataModel, me: Option<InstanceId>) -> Option<f32> {
    let kart = driving(world, me)?;
    let speed = num(world, kart, "speed");
    let boost = if flag(world, kart, "boosting") { 0.12 } else { 0.0 };
    Some(0.8 + speed / 70.0 * 0.65 + boost)
}

/// While you drive: the speed in the bottom-right corner (left of the
/// health bar), with the drift charge under it, in the classic style.
pub fn draw_hud(ctx: &egui::Context, area: egui::Rect, world: &DataModel, me: Option<InstanceId>) {
    use crate::classic;
    let Some(kart) = driving(world, me) else { return };
    let speed = num(world, kart, "speed");
    let drift = num(world, kart, "drift");
    let boosting = flag(world, kart, "boosting");
    let size = egui::vec2(132.0, 58.0);
    let rect = classic::snap(egui::Rect::from_min_size(area.right_bottom() - size - egui::vec2(48.0, 10.0), size));
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("kart hud")));
    classic::panel(&p, rect);
    let color = if boosting { Color32::from_rgb(255, 170, 30) } else { Color32::WHITE };
    classic::text(&p, rect.right_top() + egui::vec2(-58.0, 23.0), egui::Align2::RIGHT_CENTER, &format!("{}", speed.round() as i32), classic::bold(30.0), color);
    classic::text(&p, rect.right_top() + egui::vec2(-52.0, 27.0), egui::Align2::LEFT_CENTER, if boosting { "BOOST" } else { "SPEED" }, classic::bold(12.0), color);
    // Drift charge: a sunken bar filling in the spark colour.
    let bar = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(9.0, -17.0), egui::vec2(size.x - 18.0, 9.0));
    classic::bevel(&p, bar, Color32::from_gray(45), true);
    if drift > 0.0 {
        let fill = (drift / 2.0).clamp(0.15, 1.0);
        let inside = bar.shrink(2.0);
        p.rect_filled(classic::snap(egui::Rect::from_min_size(inside.min, egui::vec2(inside.width() * fill, inside.height()))), 0.0, spark_color(drift));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_note_rises_with_speed_and_sparks_change_colour() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let k = dm.create(brixo_core::Class::Model, "Kart", root).unwrap();
        dm.get_mut(k).unwrap().attributes.insert("kart".into(), Attribute::Bool(true));
        let me = dm.create(brixo_core::Class::Player, "Me", root).unwrap();
        assert_eq!(engine_pitch(&dm, Some(me)), None, "not driving");
        dm.player_mut(me).unwrap().kart = Some(k);
        let idle = engine_pitch(&dm, Some(me)).unwrap();
        dm.get_mut(k).unwrap().attributes.insert("speed".into(), Attribute::Num(70.0));
        assert!(engine_pitch(&dm, Some(me)).unwrap() > idle + 0.5);
        assert_ne!(spark_color(0.5), spark_color(1.0));
        assert_ne!(spark_color(1.0), spark_color(2.0));
    }
}
