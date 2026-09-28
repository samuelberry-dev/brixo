//! Drawing a game's on-screen GUI and the tool hotbar, and turning clicks
//! on them into events. The studio and the player both use this.

use brixo_core::{Class, DataModel, InstanceId};
use brixo_runtime::{backpack, gui_shown};

/// What the player did with the GUI this frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GuiEvents {
    /// A TextButton was clicked.
    pub clicked: Option<InstanceId>,
    /// A hotbar slot was clicked (0-based).
    pub hotbar: Option<usize>,
    /// The pointer is over a button or the hotbar, so a click there isn't
    /// a tool swing.
    pub pointer_on_gui: bool,
}

/// Which GUI elements `me` sees, in drawing order (later on top): shared
/// ones, plus ones inside `me`. With no player (the studio editor), only
/// shared ones.
pub fn visible_gui(world: &DataModel, me: Option<InstanceId>) -> Vec<InstanceId> {
    world
        .walk()
        .into_iter()
        .filter(|id| world.gui(*id).is_some() && gui_shown(world, *id))
        .filter(|id| match world.player_of(*id) {
            None => true,
            owner => owner == me && me.is_some(),
        })
        .collect()
}

fn color(c: brixo_core::Color) -> egui::Color32 {
    egui::Color32::from_rgb(c.r, c.g, c.b)
}

/// Where a point in the world lands on screen, if it's in front of the
/// camera and not too far away.
pub type Project<'a> = &'a dyn Fn(glam::Vec3) -> Option<egui::Pos2>;

/// Draws the GUI `me` can see over `area`, and reports clicks. Labels
/// attached to parts float above them, placed by `project`.
pub fn draw_gui(
    ctx: &egui::Context,
    area: egui::Rect,
    world: &DataModel,
    me: Option<InstanceId>,
    interactive: bool,
    project: Project,
) -> GuiEvents {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("brixo gui")));
    let (pointer, clicked) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked()));
    let mut events = GuiEvents::default();
    for id in visible_gui(world, me) {
        let (Some(g), Some(inst)) = (world.gui(id), world.get(id)) else { continue };
        let size = egui::vec2(g.width * area.width(), g.height * area.height());
        let rect = match g.attached_to {
            // Above the part it's attached to, nudged by x/y.
            Some(part) => {
                let Some(p) = world.part(part) else { continue };
                let top = glam::Vec3::new(p.position.x, p.position.y + p.size.y / 2.0 + 1.5, p.position.z);
                let Some(at) = project(top) else { continue };
                let at = at + egui::vec2(g.x * area.width(), g.y * area.height());
                egui::Rect::from_center_size(at, size)
            }
            None => egui::Rect::from_min_size(area.min + egui::vec2(g.x * area.width(), g.y * area.height()), size),
        };
        let is_button = inst.class == Class::TextButton;
        let hovered = interactive && is_button && pointer.is_some_and(|p| rect.contains(p));
        let pressed = hovered && ctx.input(|i| i.pointer.primary_down());
        // The Brixo look, like the website: labels and buttons glossy in
        // their own colour (buttons brighten under the mouse and push in
        // when pressed), Frames a softer gloss with faint studs.
        if g.background {
            let bg = color(g.background_color);
            if inst.class == Class::Frame {
                crate::classic::gloss_panel(&painter, rect, bg);
            } else {
                let hot = if hovered && !pressed { 1.0 } else { 0.0 };
                crate::classic::gloss(&painter, rect, bg, hot, pressed);
                if is_button {
                    crate::classic::edge(&painter, rect.expand(1.0), egui::Color32::from_black_alpha(110));
                }
            }
        }
        if !g.text.is_empty() {
            let at = rect.center() + if pressed { egui::vec2(1.0, 1.0) } else { egui::Vec2::ZERO };
            // Shrunk to fit its box if it's too wide (bold text is wide).
            let mut size = g.text_size;
            let wide = painter.layout_no_wrap(g.text.clone(), crate::classic::bold(size), egui::Color32::WHITE).size().x;
            if wide > rect.width() - 6.0 && rect.width() > 12.0 {
                size = (size * (rect.width() - 6.0) / wide).max(size * 0.6);
            }
            let font = crate::classic::bold(size);
            let ink = color(g.text_color);
            if g.background {
                // On its own background: the website's drop shadow.
                crate::classic::shadow_text(&painter, at, egui::Align2::CENTER_CENTER, &g.text, font, ink);
            } else {
                crate::classic::text(&painter, at, egui::Align2::CENTER_CENTER, &g.text, font, ink);
            }
        }
        if hovered {
            events.pointer_on_gui = true;
            if clicked {
                events.clicked = Some(id);
            }
        }
    }
    events
}

/// Where a beacon lands on a screen of `size` pixels, given its clip-space
/// position: `(point, on_screen)`. Off screen (or behind the camera) it's
/// pinned to the nearest edge, `margin` pixels in, in the direction it lies.
pub fn beacon_spot(clip: glam::Vec4, size: egui::Vec2, margin: f32) -> (egui::Pos2, bool) {
    let half = size / 2.0;
    if clip.w > 0.05 {
        let n = glam::Vec2::new(clip.x, clip.y) / clip.w;
        if n.x.abs() <= 1.0 && n.y.abs() <= 1.0 {
            return (egui::pos2(half.x * (1.0 + n.x), half.y * (1.0 - n.y)), true);
        }
    }
    // Which way it lies from the middle of the screen. (Before dividing by
    // w, clip x and y still point the way the thing really is, even when
    // it's behind the camera, where dividing would flip them.)
    let mut d = glam::Vec2::new(clip.x, -clip.y);
    if d.length_squared() < 1e-8 {
        d = glam::Vec2::new(0.0, 1.0); // dead behind: point down
    }
    let (hx, hy) = ((half.x - margin).max(1.0), (half.y - margin).max(1.0));
    let t = (d.x.abs() / hx).max(d.y.abs() / hy);
    let e = d / t;
    (egui::pos2(half.x + e.x, half.y + e.y), false)
}

/// Draws a marker over every part with `beacon = true`, visible through
/// walls and at any distance, pinned to the screen edge when it's off to
/// one side. Shows its `beacon_text` and how far away it is. Parts being
/// `carried_by` `me` are skipped (you know you've got it).
pub fn draw_beacons(ctx: &egui::Context, area: egui::Rect, world: &DataModel, me: Option<InstanceId>, camera: &brixo_render::Camera) {
    use brixo_core::Attribute;
    let my_name = me.and_then(|m| world.get(m)).map(|i| i.name.clone());
    let vp = camera.view_proj(area.width() / area.height().max(1.0));
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("brixo beacons")));
    for id in world.walk() {
        let (Some(inst), Some(p)) = (world.get(id), world.part(id)) else { continue };
        if !matches!(inst.attributes.get("beacon"), Some(Attribute::Bool(true))) {
            continue;
        }
        if let (Some(Attribute::Str(by)), Some(mine)) = (inst.attributes.get("carried_by"), &my_name) {
            if by == mine {
                continue;
            }
        }
        let top = glam::Vec3::new(p.position.x, p.position.y + p.size.y / 2.0 + 2.0, p.position.z);
        let (spot, on_screen) = beacon_spot(vp * top.extend(1.0), area.size(), 36.0);
        let at = area.min + spot.to_vec2();
        let col = egui::Color32::from_rgb(p.color.r, p.color.g, p.color.b);
        // A diamond, outlined so it reads on any background.
        let r = if on_screen { 9.0 } else { 11.0 };
        let diamond = vec![at + egui::vec2(0.0, -r), at + egui::vec2(r, 0.0), at + egui::vec2(0.0, r), at + egui::vec2(-r, 0.0)];
        painter.add(egui::Shape::convex_polygon(diamond, col, egui::Stroke::new(2.0, egui::Color32::BLACK)));
        if !on_screen {
            // An arrow on the outside, pointing the way to turn.
            let out = (at - area.center()).normalized();
            let side = egui::vec2(-out.y, out.x);
            let tip = at + out * (r + 9.0);
            let base = at + out * (r + 2.0);
            painter.add(egui::Shape::convex_polygon(vec![tip, base + side * 6.0, base - side * 6.0], egui::Color32::WHITE, egui::Stroke::NONE));
        }
        let label = match inst.attributes.get("beacon_text") {
            Some(Attribute::Str(t)) if !t.is_empty() => t.clone(),
            _ => inst.name.clone(),
        };
        let dist = camera.position.distance(top);
        let text = format!("{label}  {}m", dist.round() as i64);
        // Keep the words on screen near the edges.
        let below = at.y < area.min.y + 60.0;
        let text_at = egui::pos2(
            at.x.clamp(area.min.x + 90.0, area.max.x - 90.0),
            if below { at.y + r + 12.0 } else { at.y - r - 12.0 },
        );
        crate::classic::text(&painter, text_at, egui::Align2::CENTER_CENTER, &text, crate::classic::bold(14.0), egui::Color32::WHITE);
    }
}

/// Draws `me`'s backpack as numbered slots along the bottom of `area`:
/// classic see-through black squares, the tool in hand lit up and framed.
pub fn draw_hotbar(ctx: &egui::Context, area: egui::Rect, world: &DataModel, me: InstanceId, events: &mut GuiEvents) {
    use crate::classic;
    let tools = backpack(world, me);
    if tools.is_empty() {
        return;
    }
    let equipped = world.player(me).and_then(|p| p.equipped);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("brixo hotbar")));
    let (pointer, clicked) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked()));
    let (w, gap) = (64.0, 4.0);
    let shown = tools.len().min(9);
    let total = shown as f32 * (w + gap) - gap;
    let mut x = (area.center().x - total / 2.0).round();
    for (slot, tool) in tools.iter().enumerate().take(9) {
        let rect = egui::Rect::from_min_size(egui::pos2(x, (area.max.y - w - 70.0).round()), egui::vec2(w, w));
        let held = equipped == Some(*tool);
        let hovered = pointer.is_some_and(|p| rect.contains(p));
        painter.rect_filled(rect, 0.0, if held {
            egui::Color32::from_rgba_premultiplied(40, 40, 40, 190)
        } else if hovered {
            egui::Color32::from_black_alpha(165)
        } else {
            classic::PANEL
        });
        if held {
            classic::edge(&painter, rect, egui::Color32::WHITE);
            classic::edge(&painter, rect.shrink(1.0), egui::Color32::WHITE);
            classic::edge(&painter, rect.expand(1.0), egui::Color32::BLACK);
        } else {
            classic::edge(&painter, rect, egui::Color32::from_gray(if hovered { 200 } else { 120 }));
        }
        classic::text(&painter, rect.left_top() + egui::vec2(5.0, 3.0), egui::Align2::LEFT_TOP, &format!("{}", slot + 1), classic::bold(13.0), egui::Color32::WHITE);
        let name = world.get(*tool).map(|i| i.name.clone()).unwrap_or_default();
        classic::text_wrapped(&painter, rect.center() + egui::vec2(0.0, 5.0), &name, classic::bold(12.0), egui::Color32::WHITE, w - 6.0);
        if hovered {
            events.pointer_on_gui = true;
            if clicked {
                events.hotbar = Some(slot);
            }
        }
        x += w + gap;
    }
}

/// Keys 1-9 this frame, as a hotbar slot.
pub fn hotbar_key(ctx: &egui::Context) -> Option<usize> {
    use egui::Key::*;
    let keys = [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
    ctx.input(|i| keys.iter().position(|k| i.key_pressed(*k)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacons_sit_on_their_target_or_pin_to_the_nearest_edge() {
        let size = egui::vec2(800.0, 600.0);
        // Dead ahead: the middle of the screen, on screen.
        let (at, on) = beacon_spot(glam::Vec4::new(0.0, 0.0, 0.5, 1.0), size, 30.0);
        assert!(on && (at.x - 400.0).abs() < 1e-3 && (at.y - 300.0).abs() < 1e-3);
        // Far off to the right: pinned to the right edge, level with the middle.
        let (at, on) = beacon_spot(glam::Vec4::new(5.0, 0.0, 0.5, 1.0), size, 30.0);
        assert!(!on && (at.x - 770.0).abs() < 1e-3 && (at.y - 300.0).abs() < 1e-3, "{at:?}");
        // Behind and to the left: still the left edge.
        let (at, on) = beacon_spot(glam::Vec4::new(-2.0, 0.0, 0.5, -1.0), size, 30.0);
        assert!(!on && (at.x - 30.0).abs() < 1e-3, "{at:?}");
        // Up and off the top: pinned to the top edge.
        let (at, on) = beacon_spot(glam::Vec4::new(0.0, 4.0, 0.5, 1.0), size, 30.0);
        assert!(!on && (at.y - 30.0).abs() < 1e-3, "{at:?}");
    }

    #[test]
    fn each_player_sees_shared_gui_and_their_own_only() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let shared = dm.create(Class::TextLabel, "Title", root).unwrap();
        let ann = dm.create(Class::Player, "Ann", root).unwrap();
        let bob = dm.create(Class::Player, "Bob", root).unwrap();
        let anns = dm.create(Class::TextButton, "AnnsButton", ann).unwrap();
        let hidden = dm.create(Class::Frame, "Hidden", root).unwrap();
        dm.gui_mut(hidden).unwrap().visible = false;
        let inside_hidden = dm.create(Class::TextLabel, "InHidden", hidden).unwrap();
        assert_eq!(visible_gui(&dm, Some(ann)), vec![shared, anns]);
        assert_eq!(visible_gui(&dm, Some(bob)), vec![shared]);
        assert_eq!(visible_gui(&dm, None), vec![shared], "the editor shows shared GUI");
        assert!(!visible_gui(&dm, Some(ann)).contains(&inside_hidden), "hidden frames hide their contents");
    }
}

// --- chat ------------------------------------------------------------------

/// Recent chat: shown in a box, and as bubbles over the speakers' heads.
#[derive(Default)]
pub struct ChatLog {
    lines: Vec<(std::time::Instant, InstanceId, String, String)>,
}

/// How long a line stays in the chat box, and a bubble over a head.
const CHAT_SHOWN: f32 = 30.0;
const BUBBLE_SHOWN: f32 = 6.0;

impl ChatLog {
    pub fn push(&mut self, from: InstanceId, name: String, text: String) {
        self.lines.push((std::time::Instant::now(), from, name, text));
        if self.lines.len() > 50 {
            self.lines.remove(0);
        }
    }

    /// Draws the chat (top left, under the toolbar and chat bar, like the
    /// classic chat: names in colour, words in white, outlined, no box) and
    /// bubbles over heads.
    pub fn draw(&self, ctx: &egui::Context, area: egui::Rect, world: &DataModel, project: Project) {
        use crate::classic;
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("brixo chat")));
        let font = classic::bold(14.0);
        // Oldest at the top, newest at the bottom, fading out with age.
        let recent: Vec<_> = self.lines.iter().rev().take(8).take_while(|l| l.0.elapsed().as_secs_f32() <= CHAT_SHOWN).collect();
        let mut y = area.min.y + classic::TOP_BAR + classic::CHAT_BAR + 6.0;
        for (at, _, name, text) in recent.into_iter().rev() {
            let age = at.elapsed().as_secs_f32();
            let alpha = (1.0 - (age - CHAT_SHOWN + 3.0).max(0.0) / 3.0).clamp(0.0, 1.0);
            let x = area.min.x + 7.0;
            let label = format!("{name}: ");
            let name_rect = classic::text(&painter, egui::pos2(x, y), egui::Align2::LEFT_TOP, &label, font.clone(), classic::name_color(name).gamma_multiply(alpha));
            let rest = egui::pos2(name_rect.right(), y);
            // The words, wrapped to the chat's width.
            let wrap = (area.width() * 0.25).clamp(200.0, 340.0) - (rest.x - x);
            let layout = |c: egui::Color32| painter.layout(text.clone(), font.clone(), c, wrap.max(80.0));
            let words = layout(egui::Color32::WHITE.gamma_multiply(alpha));
            let outline = layout(egui::Color32::from_black_alpha((215.0 * alpha) as u8));
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
                painter.galley(rest + egui::vec2(dx, dy), outline.clone(), egui::Color32::BLACK);
            }
            let h = words.size().y.max(name_rect.height());
            painter.galley(rest, words, egui::Color32::WHITE);
            y += h + 2.0;
        }
        // Bubbles: each player's latest message, over their head.
        let mut latest: Vec<&(std::time::Instant, InstanceId, String, String)> = Vec::new();
        for line in self.lines.iter().rev() {
            if line.0.elapsed().as_secs_f32() < BUBBLE_SHOWN && !latest.iter().any(|l| l.1 == line.1) {
                latest.push(line);
            }
        }
        for (_, from, _, text) in latest {
            let Some(p) = world.player(*from) else { continue };
            let head = glam::Vec3::new(p.body.position.x, p.body.position.y + 3.8, p.body.position.z);
            let Some(at) = project(head) else { continue };
            // A classic bubble: white, black-edged, with a tail.
            let galley = painter.layout(text.clone(), crate::classic::plain(16.0), egui::Color32::BLACK, 240.0);
            let size = galley.size() + egui::vec2(18.0, 10.0);
            let rect = crate::classic::snap(egui::Rect::from_center_size(at - egui::vec2(0.0, size.y / 2.0 + 8.0), size));
            let tail = vec![rect.center_bottom() + egui::vec2(-7.0, -1.0), rect.center_bottom() + egui::vec2(7.0, -1.0), rect.center_bottom() + egui::vec2(0.0, 9.0)];
            painter.rect(rect, 6.0, egui::Color32::WHITE, egui::Stroke::new(1.0, egui::Color32::BLACK), egui::StrokeKind::Inside);
            painter.add(egui::Shape::convex_polygon(tail.clone(), egui::Color32::WHITE, egui::Stroke::NONE));
            painter.line_segment([tail[0] + egui::vec2(0.0, 1.0), tail[2]], egui::Stroke::new(1.0, egui::Color32::BLACK));
            painter.line_segment([tail[1] + egui::vec2(0.0, 1.0), tail[2]], egui::Stroke::new(1.0, egui::Color32::BLACK));
            painter.galley(rect.min + egui::vec2(9.0, 5.0), galley, egui::Color32::BLACK);
        }
    }
}
