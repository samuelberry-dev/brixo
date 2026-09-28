//! Brixo Player's own screens and menus, dressed like the website: the
//! home screen and the update notice. In a game, everything's in the
//! classic early-2000s style (brixo_client::classic): the toolbar, the
//! health bar, the game menu (Settings, Help, "Are you sure?"), the
//! output, and the "joining" and "connection lost" boxes.

use brixo_client::install::{self, App};
use brixo_client::classic::{self, ToolbarButton};
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
    /// Help: the keys.
    Controls,
    /// "Are you sure?" before resetting your character, and leaving.
    Reset,
    Leave,
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

/// Shift lock's crosshair in the middle of the screen, and a note in the
/// corner so you know why the mouse is gone.
fn shift_lock_hud(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("shift lock")));
    let c = screen.center();
    p.circle_stroke(c, 7.0, egui::Stroke::new(3.0, Color32::from_black_alpha(110)));
    p.circle_stroke(c, 7.0, egui::Stroke::new(1.6, Color32::WHITE));
    p.circle_filled(c, 1.6, Color32::WHITE);
    classic::text(&p, screen.left_bottom() + egui::vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, "Shift lock on (Shift)", classic::bold(13.0), classic::YELLOW);
}

/// A menu button: big, grey, the menu's full width.
fn menu_button(ui: &mut egui::Ui, label: &str) -> bool {
    let w = ui.available_width();
    classic::button_sized(ui, label, egui::vec2(w, 34.0), 15.0).clicked()
}

/// A label in a fixed-width column on the left.
fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(egui::vec2(110.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(110.0);
        ui.label(text);
    });
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

/// A line of small grey words.
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(classic::plain(12.5)).color(classic::HINT));
}

pub struct Hud<'a> {
    pub session: &'a mut Session,
    pub action: &'a mut Option<Action>,
    pub settings: &'a mut Settings,
    pub tab: &'a mut PauseTab,
    /// Shift lock is on (and the camera is using it).
    pub shift_locked: bool,
}

pub fn game_ui(ctx: &egui::Context, hud: Hud) {
    let Hud { session: s, action, settings, tab, shift_locked } = hud;
    let screen = ctx.screen_rect();

    // The toolbar, top left: Menu, Help, Fullscreen.
    if !s.lost {
        match classic::toolbar(ctx, screen) {
            Some(ToolbarButton::Menu) => {
                s.paused = !s.paused;
                *tab = PauseTab::Game;
            }
            Some(ToolbarButton::Help) => {
                s.paused = true;
                *tab = PauseTab::Controls;
            }
            Some(ToolbarButton::Fullscreen) => *action = Some(Action::Fullscreen),
            None => {}
        }
    }

    // Health: the classic bar on the right.
    let health = match &s.backend {
        Backend::Local(game) => game.player_id().and_then(|id| game.world().player(id).map(|p| (p.health, p.max_health))),
        Backend::Online(net) => net.me.and_then(|id| net.world.player(id).map(|p| (p.health, p.max_health))),
    };
    if let Some((hp, max)) = health {
        classic::health_bar(ctx, screen, hp, max);
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
        brixo_client::leaderboard::draw(ctx, screen, &s.view, me, &mut s.board_open);
    }

    if s.console {
        let output = &s.output;
        egui::Area::new(egui::Id::new("console")).anchor(egui::Align2::RIGHT_BOTTOM, [-50.0, -80.0]).order(egui::Order::Tooltip).show(ctx, |ui| {
            egui::Frame::NONE.fill(classic::DIALOG).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_width(540.0);
                classic::dark_widgets(ui);
                let (bar, _) = ui.allocate_exact_size(egui::vec2(540.0, 20.0), egui::Sense::hover());
                classic::text(ui.painter(), bar.left_center(), egui::Align2::LEFT_CENTER, "Output (F9)", classic::bold(14.0), Color32::WHITE);
                egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if output.is_empty() {
                        note(ui, "Nothing yet. What the game's scripts print shows up here.");
                    }
                    for line in output {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(RichText::new(format!("[{}]", line.source)).monospace().color(Color32::from_gray(150)));
                            let text = RichText::new(&line.text).monospace();
                            ui.label(if line.is_error { text.color(Color32::from_rgb(255, 90, 80)) } else { text });
                        });
                    }
                });
            });
        });
    }

    if !s.lost && matches!(&s.backend, Backend::Online(n) if n.me.is_none()) {
        classic::dialog(ctx, "joining", "Joining...", 300.0, false, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0).color(Color32::WHITE));
                ui.label(format!("Joining {}", s.name));
            });
        });
    }

    if s.lost {
        let never_joined = matches!(&s.backend, Backend::Online(n) if n.me.is_none());
        classic::dialog(ctx, "disconnected", if never_joined { "Couldn't join" } else { "Connection lost" }, 340.0, true, |ui| {
            ui.add(egui::Label::new(if never_joined {
                "That Play link expired or was already used. Press Play on the website again."
            } else {
                "The server closed, or the connection was lost."
            }).wrap());
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if classic::button(ui, "OK", 90.0).clicked() {
                    *action = Some(Action::Leave);
                }
            });
        });
        return;
    }

    if s.paused {
        let name = s.name.clone();
        let mut resume = false;
        let (title, width) = match *tab {
            PauseTab::Game => ("Game Menu", 300.0),
            PauseTab::Settings => ("Settings", 400.0),
            PauseTab::Controls => ("Help", 470.0),
            PauseTab::Reset => ("Reset Character?", 300.0),
            PauseTab::Leave => ("Leave Game?", 300.0),
        };
        classic::dialog(ctx, "pause", title, width, true, |ui| {
            match *tab {
                PauseTab::Game => {
                    ui.vertical_centered(|ui| note(ui, &name));
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.y = 7.0;
                    if menu_button(ui, "Reset Character") {
                        *tab = PauseTab::Reset;
                    }
                    if menu_button(ui, "Settings") {
                        *tab = PauseTab::Settings;
                    }
                    if menu_button(ui, "Help") {
                        *tab = PauseTab::Controls;
                    }
                    if menu_button(ui, "Leave Game") {
                        *tab = PauseTab::Leave;
                    }
                    ui.add_space(10.0);
                    if menu_button(ui, "Resume Game") {
                        resume = true;
                    }
                }
                PauseTab::Reset | PauseTab::Leave => {
                    let leaving = *tab == PauseTab::Leave;
                    ui.add(egui::Label::new(if leaving {
                        "Are you sure you want to leave this game?"
                    } else {
                        "Are you sure you want to reset your character? You'll be knocked out, and come back at a spawn."
                    }).wrap());
                    ui.add_space(14.0);
                    ui.columns(2, |cols| {
                        cols[0].vertical_centered(|ui| {
                            if classic::button_sized(ui, "Yes", egui::vec2(110.0, 30.0), 14.0).clicked() {
                                *action = Some(if leaving { Action::Leave } else { Action::Reset });
                                resume = true;
                            }
                        });
                        cols[1].vertical_centered(|ui| {
                            if classic::button_sized(ui, "No", egui::vec2(110.0, 30.0), 14.0).clicked() {
                                *tab = PauseTab::Game;
                            }
                        });
                    });
                }
                PauseTab::Settings => {
                    classic::heading(ui, "Camera");
                    ui.checkbox(&mut settings.shift_lock, "Shift turns on shift lock");
                    note(ui, "Shift lock hides the mouse, turns you with the camera, and looks over your shoulder: handy for aiming.");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        row_label(ui, "Camera speed");
                        ui.add(egui::Slider::new(&mut settings.camera_speed, 0.25..=2.5).step_by(0.05).fixed_decimals(2).suffix("x"));
                    });
                    classic::heading(ui, "Sound");
                    percent_slider(ui, "Music", &mut settings.music);
                    percent_slider(ui, "Sounds", &mut settings.sounds);
                    ui.add_space(12.0);
                    if classic::button(ui, "Back", 90.0).clicked() {
                        *tab = PauseTab::Game;
                    }
                }
                PauseTab::Controls => {
                    classic::heading(ui, "Moving");
                    controls(ui, &[
                        (&["W", "A", "S", "D"], "Walk (or the arrow keys Up and Down)"),
                        (&["Space"], "Jump (hold to jump higher)"),
                        (&["Shift"], "Shift lock on or off"),
                    ]);
                    classic::heading(ui, "Camera");
                    controls(ui, &[
                        (&["Right-drag"], "Look around (or swipe sideways on a trackpad)"),
                        (&["Left", "Right"], "Turn"),
                        (&["PgUp", "PgDn"], "Tilt (fn + Up/Down on a Mac)"),
                        (&["Scroll"], "Zoom (or pinch, or I and O). All the way in is first person"),
                    ]);
                    classic::heading(ui, "Playing");
                    controls(ui, &[
                        (&["1", "…", "9"], "Hold a tool; click to use it"),
                        (&["/", "Enter"], "Chat"),
                        (&["Tab"], "Fold the leaderboard away (and back)"),
                        (&["F11"], "Fullscreen"),
                        (&["F9"], "Output (what the game's scripts print)"),
                        (&["Esc"], "The menu"),
                    ]);
                    ui.add_space(12.0);
                    if classic::button(ui, "Back", 90.0).clicked() {
                        *tab = PauseTab::Game;
                    }
                }
            }
        });
        if resume {
            s.paused = false;
            *tab = PauseTab::Game;
        }
    }
}

fn controls(ui: &mut egui::Ui, rows: &[(&[&str], &str)]) {
    egui::Grid::new(ui.next_auto_id()).num_columns(2).min_col_width(118.0).spacing([10.0, 5.0]).show(ui, |ui| {
        for (keys, what) in rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in *keys {
                    classic::keycap(ui, k);
                }
            });
            ui.label(*what);
            ui.end_row();
        }
    });
}
