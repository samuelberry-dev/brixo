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

// --- The website's look, painted --------------------------------------------

/// The logo's letters and brick colours (the website's too).
pub const BRICKS: [(char, [u8; 3]); 5] =
    [('B', [196, 40, 28]), ('R', [13, 105, 172]), ('I', [224, 168, 0]), ('X', [75, 151, 75]), ('O', [218, 133, 65])];

pub fn rgb(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

pub fn shade(c: [u8; 3], f: f32) -> egui::Color32 {
    let s = |v: u8| ((v as f32 * f).round().clamp(0.0, 255.0)) as u8;
    egui::Color32::from_rgb(s(c[0]), s(c[1]), s(c[2]))
}

/// The website banner: navy, lighter at the top, with a grid of studs
/// (the installer's whole window, the studio's toolbar).
pub fn paint_banner(p: &egui::Painter, rect: egui::Rect) {
    let (top, bottom) = (egui::Color32::from_rgb(42, 90, 146), egui::Color32::from_rgb(13, 42, 74));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    let mut y = rect.top() + 12.0;
    while y < rect.bottom() {
        let mut x = rect.left() + 12.0;
        while x < rect.right() {
            p.circle_filled(egui::pos2(x, y + 0.8), 5.5, egui::Color32::from_black_alpha(45));
            p.circle_filled(egui::pos2(x, y), 5.0, egui::Color32::from_white_alpha(22));
            x += 24.0;
        }
        y += 24.0;
    }
    p.rect_stroke(rect.shrink(0.5), 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(10, 31, 56)), egui::StrokeKind::Inside);
}

/// One brick: studs on top, a lighter top edge, a darker bottom edge.
pub fn paint_brick(p: &egui::Painter, r: egui::Rect, color: [u8; 3], studs: usize) {
    let stud_w = (r.width() * 0.26).min(18.0);
    let stud_h = (r.height() * 0.16).max(3.0);
    for i in 0..studs {
        let cx = r.left() + r.width() * (i as f32 + 0.5) / studs as f32;
        let s = egui::Rect::from_min_size(egui::pos2(cx - stud_w / 2.0, r.top() - stud_h + 1.0), egui::vec2(stud_w, stud_h));
        p.rect_filled(s, 2.0, rgb(color));
        p.rect_filled(egui::Rect::from_min_size(s.min, egui::vec2(stud_w, stud_h * 0.45)), 2.0, shade(color, 1.18));
    }
    p.rect_filled(r.translate(egui::vec2(0.0, 2.5)), 5.0, egui::Color32::from_black_alpha(70));
    p.rect_filled(r, 5.0, shade(color, 0.78));
    p.rect_filled(egui::Rect::from_min_max(r.min, egui::pos2(r.right(), r.bottom() - 3.0)), 5.0, rgb(color));
    p.rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(r.width(), 4.0)), 3.0, shade(color, 1.15));
}


/// The website's colours (app.css), for apps dressed like the site.
pub mod site {
    use egui::Color32;
    pub const NAVY: Color32 = Color32::from_rgb(13, 42, 74);
    pub const NAVY2: Color32 = Color32::from_rgb(28, 65, 110);
    pub const BLUE: Color32 = Color32::from_rgb(37, 102, 176);
    pub const BLUE2: Color32 = Color32::from_rgb(77, 143, 214);
    pub const LINK: Color32 = Color32::from_rgb(11, 85, 168);
    pub const GOLD: Color32 = Color32::from_rgb(245, 205, 48);
    pub const GOLD2: Color32 = Color32::from_rgb(217, 169, 15);
    pub const LINE: Color32 = Color32::from_rgb(127, 164, 204);
    pub const SOFT: Color32 = Color32::from_rgb(238, 244, 251);
    pub const PICKED: Color32 = Color32::from_rgb(214, 230, 248);
    pub const INK: Color32 = Color32::from_rgb(29, 39, 51);
    pub const DIM: Color32 = Color32::from_rgb(93, 107, 124);
    pub const BG: Color32 = Color32::from_rgb(201, 217, 234);
    pub const FIELD: Color32 = Color32::from_rgb(253, 254, 255);
    pub const FIELD_LINE: Color32 = Color32::from_rgb(154, 179, 205);
    /// The guide's code boxes: dark, with a gold edge.
    pub const CODE: Color32 = Color32::from_rgb(22, 38, 58);
    pub const CODE_TEXT: Color32 = Color32::from_rgb(230, 237, 245);
    pub const RED: Color32 = Color32::from_rgb(196, 40, 28);
    pub const TAGLINE: Color32 = Color32::from_rgb(207, 227, 247);
}

/// Styles an app like the website: white boxes with blue edges, dark ink,
/// small text (Brixo Studio).
pub fn apply_site(ctx: &egui::Context) {
    // The in-game HUD's fonts, too.
    crate::classic::install_fonts(ctx);
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::light();
    v.override_text_color = Some(site::INK);
    v.panel_fill = Color32::WHITE;
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = site::FIELD; // text boxes
    v.faint_bg_color = site::SOFT;
    v.code_bg_color = site::SOFT;
    v.hyperlink_color = site::LINK;
    v.selection.bg_fill = site::PICKED;
    v.selection.stroke = Stroke::new(1.0, site::NAVY);
    v.window_stroke = Stroke::new(1.0, site::LINE);
    v.window_corner_radius = CornerRadius::same(3);
    v.menu_corner_radius = CornerRadius::same(3);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(50) };
    v.popup_shadow = v.window_shadow;
    v.text_cursor.stroke = Stroke::new(2.0, site::BLUE);
    let radius = CornerRadius::same(3);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, Color32::WHITE),
        (&mut v.widgets.inactive, Color32::from_rgb(231, 236, 241)),
        (&mut v.widgets.hovered, Color32::from_rgb(220, 233, 247)),
        (&mut v.widgets.active, site::BLUE2),
        (&mut v.widgets.open, Color32::from_rgb(220, 233, 247)),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = radius;
        w.fg_stroke.color = site::INK;
    }
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, site::FIELD_LINE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, site::LINE);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, site::BLUE2);
    v.widgets.active.bg_stroke = Stroke::new(1.0, site::GOLD2);
    v.widgets.active.fg_stroke.color = Color32::WHITE;
    v.widgets.open.bg_stroke = Stroke::new(1.0, site::BLUE2);

    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size.y = 22.0;
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(17.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.5)),
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Monospace, FontId::monospace(13.5)),
    ]
    .into();
    ctx.set_style(style);
}

/// A vertical gradient over `rect`.
pub fn paint_gradient(p: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

/// The website's glossy finishes: a lighter top half, a darker bottom half.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gloss {
    Blue,
    Green,
    Red,
    Gold,
    Gray,
    /// A box's title bar.
    Title,
}

impl Gloss {
    /// (top, middle, just below the middle, bottom, border, text).
    fn colors(self) -> ([u8; 3], [u8; 3], [u8; 3], [u8; 3], [u8; 3], Color32) {
        match self {
            Gloss::Blue => ([140, 192, 243], [77, 143, 214], [47, 115, 192], [63, 130, 207], [29, 79, 140], Color32::WHITE),
            Gloss::Green => ([166, 233, 138], [98, 192, 70], [63, 154, 40], [79, 174, 51], [45, 110, 28], Color32::WHITE),
            Gloss::Red => ([247, 165, 156], [224, 87, 74], [196, 40, 28], [210, 58, 45], [138, 29, 20], Color32::WHITE),
            Gloss::Gold => ([255, 240, 168], [247, 216, 74], [233, 185, 24], [242, 198, 46], [154, 122, 5], Color32::from_rgb(59, 44, 0)),
            Gloss::Gray => ([255, 255, 255], [231, 236, 241], [211, 218, 226], [223, 229, 236], [122, 134, 148], Color32::from_rgb(37, 48, 60)),
            Gloss::Title => ([106, 166, 230], [61, 130, 207], [47, 115, 192], [55, 119, 194], [47, 100, 160], Color32::WHITE),
        }
    }

    /// Paints the finish over `rect`, brighter while `hot`.
    pub fn paint(self, p: &egui::Painter, rect: egui::Rect, hot: f32) {
        let (a, b, c, d, border, _) = self.colors();
        let lift = |x: [u8; 3]| shade(x, 1.0 + hot * 0.08);
        let mid = rect.center().y.round();
        paint_gradient(p, egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), mid)), lift(a), lift(b));
        paint_gradient(p, egui::Rect::from_min_max(egui::pos2(rect.left(), mid), rect.max), lift(c), lift(d));
        if self != Gloss::Title {
            p.line_segment(
                [rect.left_top() + egui::vec2(1.0, 1.5), rect.right_top() + egui::vec2(-1.0, 1.5)],
                Stroke::new(1.0, Color32::from_white_alpha(110)),
            );
            p.rect_stroke(rect, 3.0, Stroke::new(1.0, rgb(border)), egui::StrokeKind::Inside);
        }
    }

    pub fn text_color(self) -> Color32 {
        self.colors().5
    }
}

/// Bold text with the website's little drop shadow (white text gets a dark
/// one, dark text a light one).
pub fn paint_label(p: &egui::Painter, pos: egui::Pos2, align: egui::Align2, text: &str, font: FontId, color: Color32) {
    let dark_text = color.r() as u16 + color.g() as u16 + color.b() as u16 <= 300;
    let (offset, shadow) =
        if dark_text { (egui::vec2(0.0, 1.0), Color32::from_white_alpha(110)) } else { (egui::vec2(0.0, 1.0), Color32::from_black_alpha(80)) };
    // The font has no bold: two copies half a pixel apart fatten it.
    for d in [egui::vec2(0.0, 0.0), egui::vec2(0.6, 0.0)] {
        p.text(pos + offset + d, align, text, font.clone(), shadow);
    }
    for d in [egui::vec2(0.0, 0.0), egui::vec2(0.6, 0.0)] {
        p.text(pos + d, align, text, font.clone(), color);
    }
}

/// A glossy button, like the website's `.btn`.
pub fn gloss_button(ui: &mut egui::Ui, text: &str, gloss: Gloss) -> egui::Response {
    let font = FontId::proportional(13.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    let size = egui::vec2(galley.size().x + 22.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let hot = if !enabled {
            0.0
        } else if response.is_pointer_button_down_on() {
            -0.8
        } else if response.hovered() {
            1.0
        } else {
            0.0
        };
        let p = ui.painter();
        gloss.paint(p, rect, hot);
        paint_label(p, rect.center(), egui::Align2::CENTER_CENTER, text, font, gloss.text_color());
        if !enabled {
            p.rect_filled(rect, 3.0, Color32::from_white_alpha(110));
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A box's glossy blue title bar across the whole width, in capitals, like
/// the website's boxes. `right` adds controls at its right end.
pub fn title_bar(ui: &mut egui::Ui, title: &str, right: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    Gloss::Title.paint(ui.painter(), rect, 0.0);
    paint_label(
        ui.painter(),
        rect.left_center() + egui::vec2(9.0, 0.0),
        egui::Align2::LEFT_CENTER,
        &title.to_uppercase(),
        FontId::proportional(12.5),
        Color32::WHITE,
    );
    let inner = rect.shrink2(egui::vec2(6.0, 1.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        on_blue(ui);
        right(ui);
    });
}

/// Restyles widgets for sitting on the blue bars and the navy banner:
/// see-through white tabs with white text, like the website's tab bar.
/// A selected one is white with blue text (the open tab).
pub fn on_blue(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    // No override: a selected tab takes its text colour from `selection`.
    v.override_text_color = None;
    for (w, alpha) in [(&mut v.widgets.inactive, 46), (&mut v.widgets.hovered, 90), (&mut v.widgets.active, 120), (&mut v.widgets.open, 90)] {
        w.bg_fill = Color32::from_white_alpha(alpha);
        w.weak_bg_fill = Color32::from_white_alpha(alpha);
        w.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(115));
        w.fg_stroke.color = Color32::WHITE;
    }
    v.widgets.noninteractive.fg_stroke.color = Color32::WHITE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(60));
    // Switched-off controls fade toward the blue, not toward white.
    v.widgets.noninteractive.weak_bg_fill = site::BLUE;
    v.selection.bg_fill = Color32::WHITE;
    v.selection.stroke = Stroke::new(1.0, site::BLUE);
    v.extreme_bg_color = Color32::from_black_alpha(60);
}

/// "B R I X O" in small bricks with `left_center` at the left middle;
/// gives back how wide it is.
pub fn paint_small_logo(p: &egui::Painter, left_center: egui::Pos2, height: f32) -> f32 {
    let (w, gap) = (height * 0.86, height * 0.1);
    for (i, (letter, color)) in BRICKS.iter().enumerate() {
        let dy = if i % 2 == 1 { height * 0.05 } else { 0.0 };
        let r = egui::Rect::from_min_size(
            egui::pos2(left_center.x + i as f32 * (w + gap), left_center.y - height / 2.0 + dy + height * 0.06),
            egui::vec2(w, height * 0.9),
        );
        paint_brick(p, r, *color, 1);
        let c = r.center() + egui::vec2(0.0, 0.5);
        let font = FontId::proportional(height * 0.72);
        p.text(c + egui::vec2(1.2, 1.5), egui::Align2::CENTER_CENTER, letter, font.clone(), Color32::from_black_alpha(90));
        for d in [egui::vec2(0.0, 0.0), egui::vec2(0.7, 0.0), egui::vec2(0.35, 0.4)] {
            p.text(c + d, egui::Align2::CENTER_CENTER, letter, font.clone(), Color32::WHITE);
        }
    }
    5.0 * w + 4.0 * gap
}

/// Flat menu items (no box round each), like a plain list of links.
pub fn menu_items(ui: &mut egui::Ui) {
    *ui.visuals_mut() = ui.ctx().style().visuals.clone();
    let v = ui.visuals_mut();
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    ui.set_min_width(120.0);
}
