//! Brixo's look for every app's interface: the same navy, blue and gold as
//! the website, so the studio, the player and the site feel like one thing.

use brixo_core::Class;
use egui::{Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const NAVY: Color32 = Color32::from_rgb(20, 27, 38);
pub const PANEL: Color32 = Color32::from_rgb(27, 35, 48);
pub const RAISED: Color32 = Color32::from_rgb(38, 50, 72);
pub const HOVER: Color32 = Color32::from_rgb(47, 61, 87);
pub const BLUE: Color32 = Color32::from_rgb(13, 105, 172);
pub const GOLD: Color32 = Color32::from_rgb(245, 205, 48);
pub const TEXT: Color32 = Color32::from_rgb(234, 240, 247);
pub const DIM: Color32 = Color32::from_rgb(169, 184, 202);

/// Styles an app's whole interface.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = NAVY; // text boxes, the code editor
    v.faint_bg_color = Color32::from_rgb(31, 40, 55);
    v.code_bg_color = NAVY;
    v.hyperlink_color = GOLD;
    v.selection.bg_fill = BLUE;
    v.selection.stroke = Stroke::new(1.0, GOLD);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(58, 75, 99));
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    let radius = CornerRadius::same(5);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, PANEL),
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, HOVER),
        (&mut v.widgets.active, BLUE),
        (&mut v.widgets.open, RAISED),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = radius;
        w.fg_stroke.color = TEXT;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(45, 58, 80));
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(90, 120, 160));
    v.widgets.active.bg_stroke = Stroke::new(1.0, GOLD);

    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(19.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(13.5)),
    ]
    .into();
    ctx.set_style(style);
}

/// The little coloured badge the Explorer shows beside each kind of thing.
pub fn badge(class: Class) -> (&'static str, Color32) {
    match class {
        Class::Workspace => ("W", Color32::from_rgb(63, 134, 201)),
        Class::Part => ("P", Color32::from_rgb(112, 124, 142)),
        Class::SpawnLocation => ("S", BLUE),
        Class::Model => ("M", Color32::from_rgb(46, 139, 87)),
        Class::Folder => ("F", Color32::from_rgb(184, 134, 75)),
        Class::Script => ("{}", Color32::from_rgb(201, 164, 25)),
        Class::Player => ("Pl", Color32::from_rgb(204, 142, 105)),
        Class::TextLabel => ("A", Color32::from_rgb(107, 50, 124)),
        Class::TextButton => ("B", Color32::from_rgb(107, 50, 124)),
        Class::Frame => ("Fr", Color32::from_rgb(107, 50, 124)),
        Class::Tool => ("T", Color32::from_rgb(75, 151, 75)),
        Class::Sound => ("♪", Color32::from_rgb(0, 143, 156)),
    }
}

/// Paints a class badge at the cursor (18x18).
pub fn paint_badge(ui: &mut egui::Ui, class: Class) {
    let (glyph, color) = badge(class);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::proportional(10.5), Color32::WHITE);
}
