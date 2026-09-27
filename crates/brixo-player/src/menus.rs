//! Brixo Player's own screens and menus, dressed like the website: the
//! home screen, the pause menu (Game, Settings and Controls), the console,
//! "joining" and "disconnected" boxes, and the HUD bits (the game's name,
//! health, shift lock).

use brixo_client::install::{self, App};
use brixo_client::theme::{self, site, Gloss};
use egui::{Color32, FontId, RichText};

use super::{Action, Backend, Session};
use crate::settings::Settings;

/// Which page of the pause menu is open.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum PauseTab {
    #[default]
    Game,
    Settings,
    Controls,
}

/// A white box with a glossy blue title bar, like the website's, placed
/// by `anchor`.
fn site_box(ctx: &egui::Context, id: &str, anchor: egui::Align2, offset: [f32; 2], width: f32, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    site_box_over(ctx, id, anchor, offset, width, title, false, add)
}

/// A site_box that can darken the whole game behind it (menus).
#[allow(clippy::too_many_arguments)]
fn site_box_over(
    ctx: &egui::Context,
    id: &str,
    anchor: egui::Align2,
    offset: [f32; 2],
    width: f32,
    title: &str,
    dim: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(egui::Id::new(id))
        .anchor(anchor, offset)
        // Above everything: the game's own GUI and the hotbar.
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            if dim {
                // Painted by this same box, so it's under the box and over
                // everything else.
                ui.painter()
                    .with_clip_rect(ctx.screen_rect())
                    .rect_filled(ctx.screen_rect(), 0.0, Color32::from_rgba_unmultiplied(13, 27, 43, 150));
            }
            egui::Frame::NONE
                .fill(Color32::WHITE)
                .stroke(egui::Stroke::new(1.0, site::LINE))
                .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(90) })
                .show(ui, |ui| {
                    ui.set_width(width);
                    theme::title_bar(ui, title, |_| {});
                    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 10)).show(ui, add);
                });
        });
}

/// Shown when Brixo Player isn't in a game: games open from the website's
/// Play buttons, so there's nothing to pick here.
pub fn home_ui(ctx: &egui::Context, message: Option<&str>) {
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let rect = ui.max_rect();
        theme::paint_banner(ui.painter(), rect);
        // The brick logo, big, in the middle.
        let height = 72.0;
        let width = height * 0.86 * 5.0 + height * 0.1 * 4.0;
        let top = rect.top() + rect.height() * 0.28;
        theme::paint_small_logo(ui.painter(), egui::pos2(rect.center().x - width / 2.0, top), height);
        theme::paint_label(
            ui.painter(),
            egui::pos2(rect.center().x, top + 62.0),
            egui::Align2::CENTER_CENTER,
            "B U I L D   I T .   P L A Y   I T .   S H A R E   I T .",
            FontId::proportional(14.0),
            site::TAGLINE,
        );
        let text_rect = egui::Rect::from_center_size(egui::pos2(rect.center().x, top + 150.0), egui::vec2((rect.width() - 60.0).min(640.0), 120.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
            ui.vertical_centered(|ui| {
                let text = message.unwrap_or("Games open from the Brixo website: find one you like and press Play.");
                ui.add(egui::Label::new(RichText::new(text).size(18.0).color(Color32::WHITE)).wrap());
                ui.add_space(18.0);
                let site_url = install::site();
                let label = format!("Open {}", site_url.trim_start_matches("https://").trim_start_matches("http://"));
                if theme::gloss_button(ui, &label, Gloss::Green).clicked() {
                    install::open_url(&site_url);
                }
            });
        });
        ui.painter().text(
            rect.right_bottom() + egui::vec2(-10.0, -8.0),
            egui::Align2::RIGHT_BOTTOM,
            install::VERSION,
            FontId::proportional(11.0),
            Color32::from_white_alpha(70),
        );
    });
}

/// "A new Brixo Player is out" along the top, until dismissed.
pub fn update_notice(ctx: &egui::Context, latest: Option<&str>, hidden: &mut bool) {
    let Some(latest) = latest else { return };
    if *hidden {
        return;
    }
    egui::Area::new(egui::Id::new("update notice"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 10.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(Color32::WHITE)
                .stroke(egui::Stroke::new(1.0, site::LINE))
                .corner_radius(3)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("A new {} is out ({latest}).", App::Player.title()));
                        if theme::gloss_button(ui, "Get it", Gloss::Blue).clicked() {
                            install::open_url(&format!("{}/download", install::site()));
                        }
                        if ui.small_button("×").clicked() {
                            *hidden = true;
                        }
                    });
                });
        });
}

/// A navy pill of text on the HUD (readable over any world).
fn pill(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(13, 42, 74, 215))
        .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(40)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(size), color).size() + egui::vec2(1.0, 0.0),
                egui::Sense::hover(),
            );
            theme::paint_label(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, text, FontId::proportional(size), color);
        });
}

/// The health bar: a glossy bar on navy, red when it's low.
fn health_bar(ctx: &egui::Context, hp: f32, max: f32) {
    egui::Area::new(egui::Id::new("health"))
        .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -12.0])
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(230.0, 22.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(13, 42, 74, 215));
            let fraction = if max > 0.0 { (hp / max).clamp(0.0, 1.0) } else { 0.0 };
            if fraction > 0.0 {
                let fill = egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2((rect.width() - 4.0) * fraction, rect.height() - 4.0));
                let gloss = if fraction < 0.3 { Gloss::Red } else { Gloss::Green };
                gloss.paint(&p.with_clip_rect(fill), fill, 0.0);
            }
            p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_white_alpha(60)), egui::StrokeKind::Inside);
            theme::paint_label(
                p,
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &format!("Health {} / {}", hp.round(), max.round()),
                FontId::proportional(12.5),
                Color32::WHITE,
            );
        });
}

/// Shift lock's crosshair in the middle of the screen, and a note in the
/// corner so you know why the mouse is gone.
fn shift_lock_hud(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("shift lock")));
    let c = screen.center();
    p.circle_stroke(c, 7.0, egui::Stroke::new(3.0, Color32::from_black_alpha(110)));
    p.circle_stroke(c, 7.0, egui::Stroke::new(1.6, Color32::WHITE));
    p.circle_filled(c, 1.6, Color32::WHITE);
    egui::Area::new(egui::Id::new("shift lock note"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
        .show(ctx, |ui| pill(ui, "Shift lock on (Shift)", 12.5, site::GOLD));
}

/// A key on the Controls page: a little keycap.
fn keycap(ui: &mut egui::Ui, text: &str) {
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), site::INK);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(galley.size().x + 14.0, 20.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect.translate(egui::vec2(0.0, 1.5)), 3.0, Color32::from_rgb(160, 180, 205));
    p.rect_filled(rect, 3.0, site::SOFT);
    p.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, site::LINE), egui::StrokeKind::Inside);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, font, site::INK);
}

/// A small heading inside a box, like the website's.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::hover());
    theme::paint_label(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, &text.to_uppercase(), FontId::proportional(12.0), site::NAVY2);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], egui::Stroke::new(2.0, Color32::from_rgb(219, 231, 244)));
    ui.add_space(4.0);
}

/// A label in a fixed-width column on the left.
fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(egui::vec2(100.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(100.0);
        ui.label(text);
    });
}

/// Tabs across the top of the pause menu.
fn tabs(ui: &mut egui::Ui, tab: &mut PauseTab) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (t, label) in [(PauseTab::Game, "Game"), (PauseTab::Settings, "Settings"), (PauseTab::Controls, "Controls")] {
            let open = *tab == t;
            let font = FontId::proportional(13.5);
            let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), site::INK);
            let (rect, response) = ui.allocate_exact_size(egui::vec2(galley.size().x + 26.0, 26.0), egui::Sense::click());
            let p = ui.painter();
            let corners = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
            let fill = if open {
                Color32::WHITE
            } else if response.hovered() {
                Color32::from_rgb(236, 243, 251)
            } else {
                Color32::from_rgb(222, 233, 245)
            };
            p.rect_filled(rect, corners, fill);
            p.rect_stroke(rect, corners, egui::Stroke::new(1.0, site::LINE), egui::StrokeKind::Inside);
            if open {
                p.line_segment([rect.left_top() + egui::vec2(1.0, 1.0), rect.right_top() + egui::vec2(-1.0, 1.0)], egui::Stroke::new(2.0, site::GOLD2));
            }
            theme::paint_label(p, rect.center(), egui::Align2::CENTER_CENTER, label, font, if open { site::BLUE } else { site::INK });
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *tab = t;
            }
        }
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().line_segment([rect.left_top(), rect.right_top()], egui::Stroke::new(1.0, site::LINE));
    ui.add_space(6.0);
}

/// A 0-to-1 volume as a slider showing percent.
fn percent_slider(ui: &mut egui::Ui, label: &str, value: &mut f32) {
    ui.horizontal(|ui| {
        row_label(ui, label);
        let mut pct = (*value * 100.0).round();
        if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%").step_by(5.0).fixed_decimals(0)).changed() {
            *value = pct / 100.0;
        }
    });
}

pub struct Hud<'a> {
    pub session: &'a mut Session,
    pub action: &'a mut Option<Action>,
    pub settings: &'a mut Settings,
    pub tab: &'a mut PauseTab,
    /// Shift lock is on (and the camera is using it).
    pub shift_locked: bool,
    /// Names of everyone in the game.
    pub players: Vec<String>,
}

pub fn game_ui(ctx: &egui::Context, hud: Hud) {
    let Hud { session: s, action, settings, tab, shift_locked, players } = hud;

    // Game name, top left.
    egui::Area::new(egui::Id::new("game name"))
        .anchor(egui::Align2::LEFT_TOP, [12.0, 10.0])
        .show(ctx, |ui| pill(ui, &s.name, 14.0, Color32::WHITE));

    // Health, bottom left.
    let health = match &s.backend {
        Backend::Local(game) => game.player_id().and_then(|id| game.world().player(id).map(|p| (p.health, p.max_health))),
        Backend::Online(net) => net.me.and_then(|id| net.world.player(id).map(|p| (p.health, p.max_health))),
    };
    if let Some((hp, max)) = health {
        health_bar(ctx, hp, max);
    }

    if shift_locked && !s.paused && !s.lost {
        shift_lock_hud(ctx);
    }

    // The leaderboard, top right (if the game has one, or company).
    if !s.lost {
        let me = match &s.backend {
            Backend::Local(game) => game.player_id(),
            Backend::Online(net) => net.me,
        };
        brixo_client::leaderboard::draw(ctx, ctx.screen_rect(), &s.view, me, &mut s.board_open);
    }

    if s.console {
        let output = &s.output;
        site_box(ctx, "console", egui::Align2::RIGHT_BOTTOM, [-12.0, -44.0], 540.0, "Console (F9)", |ui| {
            egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if output.is_empty() {
                    ui.label(RichText::new("Nothing yet. What the game's scripts print shows up here.").color(site::DIM));
                }
                for line in output {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new(format!("[{}]", line.source)).monospace().color(site::DIM));
                        let text = RichText::new(&line.text).monospace();
                        ui.label(if line.is_error { text.color(site::RED) } else { text });
                    });
                }
            });
        });
    }

    if !s.lost && matches!(&s.backend, Backend::Online(n) if n.me.is_none()) {
        site_box(ctx, "joining", egui::Align2::CENTER_CENTER, [0.0, 0.0], 300.0, "Joining", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0).color(site::BLUE));
                ui.label(RichText::new(format!("Joining {}...", s.name)).size(15.0));
            });
        });
    }

    if s.lost {
        let never_joined = matches!(&s.backend, Backend::Online(n) if n.me.is_none());
        site_box_over(ctx, "disconnected", egui::Align2::CENTER_CENTER, [0.0, 0.0], 360.0, "Disconnected", true, |ui| {
            ui.label(if never_joined {
                "Couldn't join: that Play link expired or was already used. Press Play on the website again."
            } else {
                "The server closed, or the connection was lost."
            });
            ui.add_space(10.0);
            if theme::gloss_button(ui, "Close", Gloss::Blue).clicked() {
                *action = Some(Action::Leave);
            }
        });
        return;
    }

    if s.paused {
        let name = s.name.clone();
        let mut resume = false;
        site_box_over(ctx, "pause", egui::Align2::CENTER_CENTER, [0.0, 0.0], 460.0, "Paused", true, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
            theme::paint_label(ui.painter(), rect.left_center(), egui::Align2::LEFT_CENTER, &name, FontId::proportional(20.0), site::NAVY);
            ui.add_space(8.0);
            tabs(ui, tab);
            // Every tab the same height, so the box doesn't jump about.
            let body_top = ui.cursor().top();
            match *tab {
                PauseTab::Game => {
                    heading(ui, &format!("In this game ({})", players.len()));
                    egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                        for p in &players {
                            ui.horizontal(|ui| {
                                theme::paint_badge(ui, brixo_core::Class::Player);
                                ui.label(p);
                            });
                        }
                    });
                    heading(ui, "Stuck?");
                    ui.horizontal(|ui| {
                        if theme::gloss_button(ui, "Reset character", Gloss::Gray).clicked() {
                            *action = Some(Action::Reset);
                            resume = true;
                        }
                        ui.label(RichText::new("Knocks you out, and you come back at a spawn.").small().color(site::DIM));
                    });
                }
                PauseTab::Settings => {
                    heading(ui, "Camera");
                    ui.checkbox(&mut settings.shift_lock, "Shift turns on shift lock");
                    ui.label(
                        RichText::new("Shift lock hides the mouse, turns you with the camera, and looks over your shoulder: handy for aiming.")
                            .small()
                            .color(site::DIM),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        row_label(ui, "Camera speed");
                        ui.add(egui::Slider::new(&mut settings.camera_speed, 0.25..=2.5).step_by(0.05).fixed_decimals(2).suffix("x"));
                    });
                    heading(ui, "Sound");
                    percent_slider(ui, "Music", &mut settings.music);
                    percent_slider(ui, "Sounds", &mut settings.sounds);
                }
                PauseTab::Controls => {
                    heading(ui, "Moving");
                    controls(ui, &[
                        (&["W", "A", "S", "D"], "Walk (or the arrow keys Up and Down)"),
                        (&["Space"], "Jump (hold to jump higher)"),
                        (&["Shift"], "Shift lock on or off"),
                    ]);
                    heading(ui, "Camera");
                    controls(ui, &[
                        (&["Right-drag"], "Look around (or swipe sideways on a trackpad)"),
                        (&["Left", "Right"], "Turn"),
                        (&["PgUp", "PgDn"], "Tilt (fn + Up/Down on a Mac)"),
                        (&["Scroll"], "Zoom (or pinch, or I and O). All the way in is first person"),
                    ]);
                    heading(ui, "Playing");
                    controls(ui, &[
                        (&["1", "…", "9"], "Hold a tool; click to use it"),
                        (&["/", "Enter"], "Chat"),
                        (&["F9"], "Console"),
                        (&["Tab"], "Fold the leaderboard away (and back)"),
                        (&["Esc"], "This menu"),
                    ]);
                }
            }
            let used = ui.cursor().top() - body_top;
            ui.add_space((290.0 - used).max(0.0) + 12.0);
            ui.horizontal(|ui| {
                if theme::gloss_button(ui, "Resume", Gloss::Green).clicked() {
                    resume = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::gloss_button(ui, "Leave game", Gloss::Red).clicked() {
                        *action = Some(Action::Leave);
                    }
                });
            });
        });
        if resume {
            s.paused = false;
        }
    }
}

fn controls(ui: &mut egui::Ui, rows: &[(&[&str], &str)]) {
    egui::Grid::new(ui.next_auto_id()).num_columns(2).min_col_width(118.0).spacing([10.0, 6.0]).show(ui, |ui| {
        for (keys, what) in rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in *keys {
                    keycap(ui, k);
                }
            });
            ui.label(*what);
            ui.end_row();
        }
    });
}
