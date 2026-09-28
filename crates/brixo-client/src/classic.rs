//! The in-game look: early 2000s, like Roblox around 2006 to 2010.
//!
//! - Text is Arial-style bold (Liberation Sans, which has Arial's shapes and
//!   widths) with a thin black outline, so it reads over any world.
//! - Panels are see-through black with square corners and a hard edge.
//! - Buttons are grey and bevelled like Windows 2000's: light on the top and
//!   left, dark on the bottom and right, pushed in while pressed.
//! - The health bar is the classic one: a slim upright bar on the right
//!   edge of the screen, green over red, draining from the top, with
//!   "Health" under it in blue.
//!
//! Brixo Player's HUD and Studio's Play mode both use this.

use std::sync::Arc;

use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke};

const BOLD: &str = "classic";
const PLAIN: &str = "classic plain";

/// Loads the classic fonts into `ctx` (every app that draws a HUD calls
/// this once, at start: `theme::apply_site` does).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(BOLD.into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"))));
    fonts.font_data.insert(PLAIN.into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"))));
    // Anything Liberation Sans hasn't got (arrows, symbols, emoji) comes
    // from the usual fonts.
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for name in [BOLD, PLAIN] {
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}

/// Classic bold text.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// Classic plain text.
pub fn plain(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(PLAIN.into()))
}

/// The grey of buttons (Windows 2000's).
pub const FACE: Color32 = Color32::from_rgb(212, 208, 200);
/// See-through black, behind the HUD's panels.
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 125);
/// A dialog's background: darker, and less see-through.
pub const DIALOG: Color32 = Color32::from_rgb(30, 30, 30);
/// The hard edge round a panel.
pub const EDGE: Color32 = Color32::from_rgb(27, 42, 53);
/// The health bar's colours, and its label's.
pub const HEALTH_GREEN: Color32 = Color32::from_rgb(30, 206, 40);
pub const HEALTH_RED: Color32 = Color32::from_rgb(222, 0, 0);
pub const HEALTH_BLUE: Color32 = Color32::from_rgb(20, 60, 255);
/// Your own name on the leaderboard.
pub const YELLOW: Color32 = Color32::from_rgb(255, 238, 0);
/// Soft grey for hints.
pub const HINT: Color32 = Color32::from_rgb(225, 225, 225);

fn is_light(c: Color32) -> bool {
    c.r() as u16 + c.g() as u16 + c.b() as u16 > 300
}

/// Toward white (`f` 0 to 1) or, below 1, darker.
fn mix(c: Color32, white: f32) -> Color32 {
    let m = |v: u8| (v as f32 + (255.0 - v as f32) * white).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

fn dark(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| (v as f32 * f).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Whole pixels, so one-pixel edges come out crisp.
pub fn snap(r: Rect) -> Rect {
    Rect::from_min_max(r.min.round(), r.max.round())
}

/// A one-pixel-wide rectangle's worth of line (crisper than a stroke).
fn px(p: &Painter, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
    p.rect_filled(Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)), 0.0, c);
}

/// A hard one-pixel edge just inside `r`.
pub fn edge(p: &Painter, r: Rect, c: Color32) {
    let r = snap(r);
    px(p, r.left(), r.top(), r.right(), r.top() + 1.0, c);
    px(p, r.left(), r.bottom() - 1.0, r.right(), r.bottom(), c);
    px(p, r.left(), r.top() + 1.0, r.left() + 1.0, r.bottom() - 1.0, c);
    px(p, r.right() - 1.0, r.top() + 1.0, r.right(), r.bottom() - 1.0, c);
}

/// A see-through black panel with a hard edge.
pub fn panel(p: &Painter, r: Rect) {
    let r = snap(r);
    p.rect_filled(r, 0.0, PANEL);
    edge(p, r, Color32::from_black_alpha(170));
}

/// A bevelled face: raised (light top and left, dark bottom and right), or
/// sunken (the other way round), two pixels deep.
pub fn bevel(p: &Painter, r: Rect, face: Color32, sunken: bool) {
    let r = snap(r);
    p.rect_filled(r, 0.0, face);
    let (hi, hi2, lo2, lo) = (mix(face, 0.75), mix(face, 0.3), dark(face, 0.62), dark(face, 0.3));
    let (tl, tl2, br2, br) = if sunken { (lo2, lo, hi2, hi) } else { (hi, hi2, lo2, lo) };
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    // Outer ring.
    px(p, l, t, rr - 1.0, t + 1.0, tl);
    px(p, l, t, l + 1.0, b - 1.0, tl);
    px(p, l, b - 1.0, rr, b, br);
    px(p, rr - 1.0, t, rr, b, br);
    // Inner ring.
    px(p, l + 1.0, t + 1.0, rr - 2.0, t + 2.0, tl2);
    px(p, l + 1.0, t + 1.0, l + 2.0, b - 2.0, tl2);
    px(p, l + 1.0, b - 2.0, rr - 1.0, b - 1.0, br2);
    px(p, rr - 2.0, t + 1.0, rr - 1.0, b - 1.0, br2);
}

/// The website's glossy finish in any colour, for games' own labels and
/// buttons: a lighter top half, a darker bottom half, a shine along the
/// top and a darker edge. `hot` (0 to 1) brightens it for a hovered
/// button; `pressed` turns the shine over, so it looks pushed in.
pub fn gloss(p: &Painter, r: Rect, c: Color32, hot: f32, pressed: bool) {
    let r = snap(r);
    if r.width() < 2.0 || r.height() < 2.0 {
        return;
    }
    let c = if hot > 0.0 { mix(c, 0.14 * hot) } else { c };
    let (top, mid, below, bottom) = if pressed {
        (dark(c, 0.72), dark(c, 0.86), c, mix(c, 0.12))
    } else {
        (mix(c, 0.5), mix(c, 0.2), dark(c, 0.86), mix(c, 0.06))
    };
    let m = r.center().y.round();
    crate::theme::paint_gradient(p, Rect::from_min_max(r.min, egui::pos2(r.right(), m)), top, mid);
    crate::theme::paint_gradient(p, Rect::from_min_max(egui::pos2(r.left(), m), r.max), below, bottom);
    if !pressed {
        px(p, r.left() + 1.0, r.top() + 1.0, r.right() - 1.0, r.top() + 2.0, Color32::from_white_alpha(120));
    }
    edge(p, r, dark(c, 0.42));
}

/// A game's Frame: a softer gloss (lighter at the top, darker at the
/// bottom) with the website banner's faint studs.
pub fn gloss_panel(p: &Painter, r: Rect, c: Color32) {
    let r = snap(r);
    if r.width() < 2.0 || r.height() < 2.0 {
        return;
    }
    crate::theme::paint_gradient(p, r, mix(c, 0.16), dark(c, 0.82));
    let studs = p.with_clip_rect(r.shrink(2.0));
    let mut y = r.top() + 12.0;
    while y < r.bottom() {
        let mut x = r.left() + 12.0;
        while x < r.right() {
            studs.circle_filled(egui::pos2(x, y + 0.8), 5.5, Color32::from_black_alpha(35));
            studs.circle_filled(egui::pos2(x, y), 5.0, Color32::from_white_alpha(16));
            x += 24.0;
        }
        y += 24.0;
    }
    px(p, r.left() + 1.0, r.top() + 1.0, r.right() - 1.0, r.top() + 2.0, Color32::from_white_alpha(80));
    edge(p, r, dark(c, 0.42));
}

/// Bold text with the website's little drop shadow: a dark one under light
/// text, a light one under dark text.
pub fn shadow_text(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) {
    let shadow = if is_light(color) { Color32::from_black_alpha(130) } else { Color32::from_white_alpha(110) };
    p.text(pos + egui::vec2(0.0, 1.0), align, text, font.clone(), shadow);
    p.text(pos, align, text, font, color);
}

/// Text with a thin outline: black round light text, none round dark text.
/// Gives back where the text went.
pub fn text(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    if is_light(color) {
        let outline = Color32::from_black_alpha((color.a() as f32 * 0.85) as u8);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
            p.text(pos + egui::vec2(dx, dy), align, text, font.clone(), outline);
        }
    }
    p.text(pos, align, text, font, color)
}

/// Outlined text wrapped to `width`, each line centred, centred on `center`.
pub fn text_wrapped(p: &Painter, center: Pos2, text: &str, font: FontId, color: Color32, width: f32) {
    let layout = |c: Color32| {
        let mut job = egui::text::LayoutJob::simple(text.to_string(), font.clone(), c, width);
        job.halign = egui::Align::Center;
        p.layout_job(job)
    };
    let galley = layout(color);
    let at = center - egui::vec2(0.0, galley.size().y / 2.0);
    if is_light(color) {
        let outline = layout(Color32::from_black_alpha(215));
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
            p.galley(at + egui::vec2(dx, dy), outline.clone(), Color32::BLACK);
        }
    }
    p.galley(at, galley, color);
}

/// A grey bevelled button, at least `min_width` wide.
pub fn button(ui: &mut egui::Ui, label: &str, min_width: f32) -> egui::Response {
    button_sized(ui, label, egui::vec2(min_width, 24.0), 13.0)
}

/// A grey bevelled button of at least `size`, with `font_size` text.
pub fn button_sized(ui: &mut egui::Ui, label: &str, size: egui::Vec2, font_size: f32) -> egui::Response {
    let font = bold(font_size);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::BLACK).size().x + 20.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width.max(size.x), size.y), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let pressed = enabled && response.is_pointer_button_down_on();
        let face = if enabled && response.hovered() && !pressed { mix(FACE, 0.35) } else { FACE };
        let p = ui.painter();
        edge(p, rect.expand(1.0), Color32::from_black_alpha(160));
        bevel(p, rect, face, pressed);
        let at = rect.center() + if pressed { egui::vec2(1.0, 1.0) } else { egui::Vec2::ZERO };
        let ink = if enabled { Color32::BLACK } else { Color32::from_gray(128) };
        if !enabled {
            p.text(at + egui::vec2(1.0, 1.0), Align2::CENTER_CENTER, label, font.clone(), Color32::WHITE);
        }
        p.text(at, Align2::CENTER_CENTER, label, font, ink);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Where the health bar goes on a screen of `area`: a slim upright bar at
/// the right edge, a little below the middle (under the leaderboard).
pub fn health_bar_rect(area: Rect) -> Rect {
    let h = (area.height() * 0.25).clamp(70.0, 180.0);
    let w = 16.0;
    let right = area.right() - 18.0;
    let bottom = area.center().y + area.height() * 0.22;
    snap(Rect::from_min_max(egui::pos2(right - w, bottom - h), egui::pos2(right, bottom)))
}

/// The classic health bar: green over red, the green going down from the
/// top as you're hurt, and "Health" under it in blue.
pub fn health_bar(ctx: &egui::Context, area: Rect, hp: f32, max: f32) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("classic health")));
    let bar = health_bar_rect(area);
    let fraction = if max > 0.0 { (hp / max).clamp(0.0, 1.0) } else { 0.0 };
    p.rect_filled(bar, 0.0, HEALTH_RED);
    let inside = bar.shrink(1.0);
    let green_top = (inside.bottom() - inside.height() * fraction).round();
    if fraction > 0.0 {
        let g = Rect::from_min_max(egui::pos2(inside.left(), green_top), inside.max);
        p.rect_filled(g, 0.0, HEALTH_GREEN);
        // A lighter strip down the left: the bar's shine.
        px(&p, g.left(), g.top(), g.left() + 3.0, g.bottom(), mix(HEALTH_GREEN, 0.35));
    }
    px(&p, inside.left(), inside.top(), inside.left() + 3.0, green_top.max(inside.top()), mix(HEALTH_RED, 0.3));
    edge(&p, bar, Color32::BLACK);
    // Under the bar, kept on screen.
    let label_at = egui::pos2(bar.center().x.min(area.right() - 30.0), bar.bottom() + 12.0);
    // Blue, like the original: a pale edge keeps it readable on dark worlds.
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        p.text(label_at + egui::vec2(dx, dy), Align2::CENTER_CENTER, "Health", bold(15.0), Color32::from_white_alpha(150));
    }
    p.text(label_at, Align2::CENTER_CENTER, "Health", bold(15.0), HEALTH_BLUE);
}

/// What was clicked on the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarButton {
    Menu,
    Help,
    Fullscreen,
}

/// The toolbar in the top-left corner: Menu, Help and Fullscreen, grey
/// buttons in a row.
pub fn toolbar(ctx: &egui::Context, area: Rect) -> Option<ToolbarButton> {
    let mut clicked = None;
    egui::Area::new(egui::Id::new("classic toolbar")).fixed_pos(area.left_top() + egui::vec2(5.0, 5.0)).order(egui::Order::Middle).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, which) in [("Menu", ToolbarButton::Menu), ("Help", ToolbarButton::Help), ("Fullscreen", ToolbarButton::Fullscreen)] {
                if button_sized(ui, label, egui::vec2(0.0, 22.0), 12.5).clicked() {
                    clicked = Some(which);
                }
            }
        });
    });
    clicked
}

/// How tall the toolbar and the chat bar under it are, with gaps: where the
/// chat's lines start.
pub const TOP_BAR: f32 = 30.0;
pub const CHAT_BAR: f32 = 22.0;

/// The chat bar: "To chat click here or press "/" key" until it's open,
/// then a box to type in. Gives back what was said on Enter. `top_left`
/// is where it goes.
pub fn chat_bar(ctx: &egui::Context, top_left: Pos2, width: f32, open: &mut bool, typed: &mut String) -> Option<String> {
    let mut said = None;
    egui::Area::new(egui::Id::new("classic chat bar")).fixed_pos(top_left).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, CHAT_BAR), egui::Sense::click());
        let p = ui.painter().clone();
        if !*open {
            p.rect_filled(rect, 0.0, Color32::from_black_alpha(if response.hovered() { 205 } else { 175 }));
            edge(&p, rect, Color32::from_black_alpha(170));
            p.text(rect.left_center() + egui::vec2(7.0, 0.0), Align2::LEFT_CENTER, "To chat click here or press \"/\" key", plain(13.0), HINT);
            if response.on_hover_cursor(egui::CursorIcon::Text).clicked() {
                *open = true;
            }
            return;
        }
        p.rect_filled(rect, 0.0, Color32::from_black_alpha(185));
        edge(&p, rect, Color32::from_gray(150));
        let inner = rect.shrink2(egui::vec2(6.0, 2.0));
        let edit = ui.put(
            inner,
            egui::TextEdit::singleline(typed)
                .frame(false)
                .font(plain(14.0))
                .text_color(Color32::WHITE)
                .desired_width(inner.width())
                .hint_text(egui::RichText::new("Say something, then Enter (Esc to cancel)").color(Color32::from_gray(140))),
        );
        // Check for Enter before keeping the focus: losing focus is how a
        // text box reports Enter.
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = std::mem::take(typed);
            if !text.trim().is_empty() {
                said = Some(text);
            }
            *open = false;
        } else {
            edit.request_focus();
        }
    });
    said
}

/// A player's name colour in chat, from their name: the classic eight.
pub fn name_color(name: &str) -> Color32 {
    const COLORS: [Color32; 8] = [
        Color32::from_rgb(253, 41, 67),   // bright red
        Color32::from_rgb(1, 162, 255),   // bright blue
        Color32::from_rgb(2, 184, 87),    // earth green
        Color32::from_rgb(180, 128, 255), // bright violet
        Color32::from_rgb(255, 152, 20),  // bright orange
        Color32::from_rgb(255, 230, 40),  // bright yellow
        Color32::from_rgb(255, 150, 210), // light reddish violet
        Color32::from_rgb(215, 197, 154), // brick yellow
    ];
    let mut h: u32 = 0;
    for b in name.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    COLORS[(h % 8) as usize]
}

/// A dialog in the middle of the screen: dark, square, a white bold title
/// and a line under it. `dim` darkens the game behind it (menus).
pub fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, dim: bool, add: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(egui::Id::new(id)).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).order(egui::Order::Tooltip).show(ctx, |ui| {
        if dim {
            ui.painter().with_clip_rect(ctx.screen_rect()).rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha(110));
        }
        let frame = egui::Frame::NONE.fill(DIALOG).inner_margin(egui::Margin::same(14));
        let shown = frame.show(ui, |ui| {
            ui.set_width(width);
            dark_widgets(ui);
            let (bar, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
            text(ui.painter(), bar.center(), Align2::CENTER_CENTER, title, bold(19.0), Color32::WHITE);
            let (line, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
            px(ui.painter(), line.left(), line.center().y, line.right(), line.center().y + 1.0, Color32::from_gray(95));
            ui.add_space(4.0);
            add(ui);
        });
        let r = shown.response.rect;
        edge(ui.painter(), r, Color32::from_gray(110));
        edge(ui.painter(), r.expand(1.0), Color32::BLACK);
    });
}

/// Restyles egui's own widgets (sliders, check boxes) for a dark dialog:
/// square, grey, white text.
pub fn dark_widgets(ui: &mut egui::Ui) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(Color32::from_gray(235));
    let square = egui::CornerRadius::ZERO;
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = square;
    }
    v.widgets.inactive.bg_fill = Color32::from_gray(70);
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(70);
    v.widgets.hovered.bg_fill = Color32::from_gray(95);
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(95);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(20));
    v.selection.bg_fill = Color32::from_rgb(10, 36, 106); // Windows 2000's highlight blue
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.slider_trailing_fill = true;
    ui.style_mut().visuals = v;
    ui.style_mut().override_font_id = Some(plain(14.0));
}

/// A small heading inside a dialog.
pub fn heading(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::hover());
    text(ui.painter(), rect.left_center(), Align2::LEFT_CENTER, label, bold(14.0), YELLOW);
    ui.add_space(2.0);
}

/// A key, as a little grey keycap.
pub fn keycap(ui: &mut egui::Ui, label: &str) {
    let font = bold(11.5);
    let w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::BLACK).size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w.max(20.0), 20.0), egui::Sense::hover());
    bevel(ui.painter(), rect, FACE, false);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, Color32::BLACK);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_health_bar_stands_on_the_right_edge() {
        let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let r = health_bar_rect(area);
        assert!(r.height() > r.width() * 5.0, "upright: {r:?}");
        assert!(r.right() > 1240.0 && r.right() <= 1280.0, "at the right edge: {r:?}");
        assert!(r.top() > 200.0, "below the leaderboard: {r:?}");
        let tiny = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 200.0));
        assert!(health_bar_rect(tiny).height() >= 70.0);
    }

    #[test]
    fn names_keep_their_colour() {
        assert_eq!(name_color("Ann"), name_color("Ann"));
        let colors: std::collections::HashSet<_> = ["Ann", "Bob", "Cat", "Dan", "Eve", "Fay", "Gus", "Hal"].iter().map(|n| name_color(n)).collect();
        assert!(colors.len() > 3, "names get different colours");
    }

    #[test]
    fn the_fonts_load() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            let size = ctx.fonts(|f| f.layout_no_wrap("Health".into(), bold(15.0), Color32::WHITE).size());
            assert!(size.x > 30.0, "{size:?}");
        });
    }
}
