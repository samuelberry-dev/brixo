//! The Toolbox: ready-made things from the Brixo website (karts, doors,
//! pads, the gear kit, and whatever admins add), shown as a grid of
//! pictures like Roblox Studio's. Click one and it's put in the game in
//! front of the camera.
//!
//! Everything loads on a thread: the list, then each picture. When the
//! website can't be reached, the Toolbox shows the built-in items that come
//! with Studio (brixo_samples::toolbox) instead.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use brixo_client::theme::{self, site};
use egui::{Color32, FontId};

/// One thing in the Toolbox.
#[derive(Clone, Debug)]
struct Entry {
    /// "site:12" or "built-in:kart": names its picture.
    key: String,
    name: String,
    category: String,
    description: String,
    /// On the website (fetched when it's inserted)...
    id: Option<i64>,
    /// ...or built into Studio (offline).
    content: Option<Arc<String>>,
}

#[derive(Default)]
struct Shared {
    /// None while loading.
    entries: Option<Vec<Entry>>,
    /// Why the website's toolbox isn't showing (the built-in items are).
    offline: Option<String>,
    pictures: HashMap<String, egui::ColorImage>,
    /// Fetched and ready to put in the game: (name, content).
    ready: Vec<(String, String)>,
    /// Being fetched: its key.
    fetching: Option<String>,
    error: Option<String>,
}

pub struct Toolbox {
    pub open: bool,
    search: String,
    category: String,
    shared: Arc<Mutex<Shared>>,
    textures: HashMap<String, egui::TextureHandle>,
    loaded: bool,
}

impl Default for Toolbox {
    fn default() -> Self {
        Toolbox { open: true, search: String::new(), category: ALL.into(), shared: Arc::default(), textures: HashMap::new(), loaded: false }
    }
}

const ALL: &str = "All";
const TILE: f32 = 70.0;

fn decode(png: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build()
}

/// The built-in items, for when the website can't be reached.
fn built_in(shared: &Arc<Mutex<Shared>>, why: String) {
    let items = brixo_samples::toolbox::items();
    let mut s = shared.lock().unwrap();
    for i in &items {
        if let Some(pic) = decode(i.thumbnail) {
            s.pictures.insert(format!("built-in:{}", i.slug), pic);
        }
    }
    s.entries = Some(
        items
            .into_iter()
            .map(|i| Entry {
                key: format!("built-in:{}", i.slug),
                name: i.name.into(),
                category: i.category.into(),
                description: i.description.into(),
                id: None,
                content: Some(Arc::new(i.content)),
            })
            .collect(),
    );
    s.offline = Some(why);
}

fn load(shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let site = brixo_client::install::site();
        let site = site.trim_end_matches('/');
        let agent = agent();
        let list: Result<Vec<serde_json::Value>, String> =
            agent.get(&format!("{site}/api/toolbox")).call().map_err(|e| e.to_string()).and_then(|r| r.into_json().map_err(|e| e.to_string()));
        let list = match list {
            Ok(l) => l,
            Err(_) => {
                built_in(&shared, format!("Couldn't reach {}: showing the items built into Studio.", site.trim_start_matches("https://")));
                return;
            }
        };
        let entries: Vec<(Entry, bool)> = list
            .iter()
            .filter_map(|v| {
                let id = v["id"].as_i64()?;
                Some((
                    Entry {
                        key: format!("site:{id}"),
                        name: v["name"].as_str()?.to_string(),
                        category: v["category"].as_str().unwrap_or("").to_string(),
                        description: v["description"].as_str().unwrap_or("").to_string(),
                        id: Some(id),
                        content: None,
                    },
                    v["has_thumbnail"].as_bool().unwrap_or(false),
                ))
            })
            .collect();
        shared.lock().unwrap().entries = Some(entries.iter().map(|(e, _)| e.clone()).collect());
        // Then the pictures, one at a time.
        for (e, has) in entries {
            if !has {
                continue;
            }
            let Ok(r) = agent.get(&format!("{site}/api/toolbox/{}/thumbnail", e.id.unwrap())).call() else { continue };
            let mut png = Vec::new();
            if std::io::Read::read_to_end(&mut r.into_reader(), &mut png).is_ok() {
                if let Some(pic) = decode(&png) {
                    shared.lock().unwrap().pictures.insert(e.key.clone(), pic);
                }
            }
        }
    });
}

impl Toolbox {
    /// What's been fetched and is ready to put in the game: (name, content).
    pub fn take_ready(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.shared.lock().unwrap().ready)
    }

    /// Asks for an item: at once if it's built in, otherwise fetched.
    fn pick(&mut self, e: &Entry) {
        let mut s = self.shared.lock().unwrap();
        if let Some(c) = &e.content {
            s.ready.push((e.name.clone(), c.to_string()));
            return;
        }
        if s.fetching.is_some() {
            return;
        }
        s.fetching = Some(e.key.clone());
        s.error = None;
        drop(s);
        let (shared, id, name) = (self.shared.clone(), e.id.unwrap(), e.name.clone());
        std::thread::spawn(move || {
            let site = brixo_client::install::site();
            let got: Result<serde_json::Value, String> = agent()
                .get(&format!("{}/api/toolbox/{id}", site.trim_end_matches('/')))
                .call()
                .map_err(|e| e.to_string())
                .and_then(|r| r.into_json().map_err(|e| e.to_string()));
            let mut s = shared.lock().unwrap();
            s.fetching = None;
            match got.ok().and_then(|v| v["content"].as_str().map(str::to_string)) {
                Some(content) => s.ready.push((name, content)),
                None => s.error = Some(format!("Couldn't get {name} from the website. Try again?")),
            }
        });
    }

    /// Draws the Toolbox panel's insides.
    pub fn ui(&mut self, ui: &mut egui::Ui, enabled: bool) {
        if !self.loaded {
            self.loaded = true;
            load(self.shared.clone());
        }
        // Pictures that have arrived become textures.
        {
            let mut s = self.shared.lock().unwrap();
            for (key, pic) in s.pictures.drain() {
                let t = ui.ctx().load_texture(format!("toolbox {key}"), pic, egui::TextureOptions::LINEAR);
                self.textures.insert(key, t);
            }
        }
        let (entries, offline, fetching, error) = {
            let s = self.shared.lock().unwrap();
            (s.entries.clone(), s.offline.clone(), s.fetching.clone(), s.error.clone())
        };

        // The category list and the search box, like Roblox Studio's.
        let mut categories: Vec<String> = vec![ALL.into()];
        for e in entries.iter().flatten() {
            if !categories.contains(&e.category) {
                categories.push(e.category.clone());
            }
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("toolbox category").width((ui.available_width() - 44.0).max(60.0)).selected_text(&self.category).show_ui(ui, |ui| {
                theme::menu_items(ui);
                for c in &categories {
                    ui.selectable_value(&mut self.category, c.clone(), c);
                }
            });
            if ui.small_button("⟳").on_hover_text("Load the Toolbox again").clicked() {
                *self.shared.lock().unwrap() = Shared::default();
                self.textures.clear();
                self.loaded = false;
            }
        });
        ui.add_sized([ui.available_width(), 20.0], egui::TextEdit::singleline(&mut self.search).hint_text("Search"));
        ui.add_space(4.0);

        let Some(entries) = entries else {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0).color(site::BLUE));
                ui.weak("Loading the Toolbox...");
            });
            return;
        };
        let words = self.search.to_lowercase();
        let shown: Vec<&Entry> = entries
            .iter()
            .filter(|e| self.category == ALL || e.category == self.category)
            .filter(|e| words.is_empty() || (e.name.to_lowercase() + " " + &e.description.to_lowercase()).contains(words.trim()))
            .collect();

        let mut picked = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
            let across = ((ui.available_width() + 4.0) / (TILE + 8.0)).floor().max(1.0) as usize;
            if shown.is_empty() {
                ui.weak("Nothing matches.");
            }
            for row in shown.chunks(across) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for e in row {
                        if tile(ui, e, self.textures.get(&e.key), enabled, fetching.as_deref() == Some(e.key.as_str())) {
                            picked = Some((*e).clone());
                        }
                    }
                });
                ui.add_space(2.0);
            }
        });
        if let Some(e) = picked {
            self.pick(&e);
        }
        if let Some(err) = error {
            ui.colored_label(site::RED, err);
        } else if !enabled {
            ui.weak("Stop Play to insert things.");
        } else if let Some(why) = offline {
            ui.weak(why);
        } else {
            ui.weak("Click something to put it in front of the camera.");
        }
    }
}

/// One item: its picture in a frame, its name under it. Gives back true
/// when clicked.
fn tile(ui: &mut egui::Ui, e: &Entry, picture: Option<&egui::TextureHandle>, enabled: bool, busy: bool) -> bool {
    let size = egui::vec2(TILE + 4.0, TILE + 34.0);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let p = ui.painter();
    let hot = enabled && response.hovered();
    if hot {
        // Windows XP's list selection: pale blue with a blue edge.
        p.rect_filled(rect, 2.0, Color32::from_rgb(214, 230, 248));
        p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, site::BLUE2), egui::StrokeKind::Inside);
    }
    let pic = egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2(TILE, TILE));
    p.rect_filled(pic, 0.0, Color32::from_rgb(221, 230, 238));
    match picture {
        Some(t) => {
            p.image(t.id(), pic, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.text(pic.center(), egui::Align2::CENTER_CENTER, &e.category, FontId::proportional(10.0), site::DIM);
        }
    }
    p.rect_stroke(pic, 0.0, egui::Stroke::new(1.0, if hot { site::BLUE2 } else { site::FIELD_LINE }), egui::StrokeKind::Inside);
    if busy {
        p.rect_filled(pic, 0.0, Color32::from_white_alpha(140));
        p.text(pic.center(), egui::Align2::CENTER_CENTER, "...", FontId::proportional(18.0), site::INK);
    }
    let label = egui::Rect::from_min_max(egui::pos2(rect.left(), pic.bottom() + 2.0), rect.max);
    let galley = p.layout(e.name.clone(), FontId::proportional(11.5), site::INK, label.width());
    p.galley(egui::pos2(label.center().x - galley.size().x / 2.0, label.top()), galley, site::INK);
    let response = if e.description.is_empty() { response } else { response.on_hover_text(format!("{}\n\n{}", e.name, e.description)) };
    response.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default }).clicked()
}
