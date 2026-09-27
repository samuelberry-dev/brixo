//! The Learn guide's code really works: every ```rovik block parses, and
//! every block marked `run` runs in a real game (a floor, a spawn, a player,
//! and the objects the block says it needs) for a few seconds without an
//! error. Plus: every page renders, and its links and pictures exist.

use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::Game;
use brixo_web::docs;

fn parses(code: &str) -> Result<(), String> {
    let tokens = rovik::lexer::lex(code).map_err(|e| e.to_string())?;
    rovik::parser::parse(tokens).map(|_| ()).map_err(|e| e.to_string())
}

fn class_of(name: &str) -> Class {
    match name.to_lowercase().as_str() {
        "part" => Class::Part,
        "spawnlocation" | "spawn" => Class::SpawnLocation,
        "model" => Class::Model,
        "folder" => Class::Folder,
        "textlabel" => Class::TextLabel,
        "textbutton" => Class::TextButton,
        "frame" => Class::Frame,
        "tool" => Class::Tool,
        "workspace" => Class::Workspace,
        other => panic!("unknown class '{other}' in a docs example"),
    }
}

/// The world an example runs in.
fn world_for(ex: &docs::Example) -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Baseplate", root).unwrap();
    {
        let p = dm.part_mut(floor).unwrap();
        p.size = Vec3::new(200.0, 1.0, 200.0);
        p.position = Vec3::new(0.0, -0.5, 0.0);
    }
    let spawn = dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
    // What it needs, spread out along a line so parts don't overlap.
    let mut n = 0;
    let mut make = |dm: &mut DataModel, class: Class, name: &str| {
        if let Some(existing) = dm.find_first(name) {
            return existing;
        }
        let parent = match class {
            // Tools wait in a Storage folder, like in a real game.
            Class::Tool => dm.find_first("Storage").unwrap_or_else(|| dm.create(Class::Folder, "Storage", root).unwrap()),
            _ => root,
        };
        let id = dm.create(class, name, parent).unwrap();
        n += 1;
        if let Some(p) = dm.part_mut(id) {
            p.position = Vec3::new(12.0 * n as f32, 3.0, 20.0);
            p.size = Vec3::new(4.0, 1.0, 4.0);
        }
        if class == Class::Tool {
            let h = dm.create(Class::Part, "Handle", id).unwrap();
            dm.part_mut(h).unwrap().position = Vec3::new(0.0, -300.0, 0.0);
        }
        id
    };
    for (class, name) in &ex.with {
        make(&mut dm, class_of(class), name);
    }
    let parent = match &ex.inside {
        Some((class, name)) => make(&mut dm, class_of(class), name),
        None => root,
    };
    let s = dm.create(Class::Script, "Example", parent).unwrap();
    dm.script_mut(s).unwrap().source = ex.code.clone();
    dm
}

#[test]
fn every_code_example_in_the_guide_parses() {
    let examples = docs::examples();
    assert!(examples.len() > 50, "the guide has plenty of examples: {}", examples.len());
    let mut failures = Vec::new();
    for ex in examples.iter().filter(|e| !e.sketch) {
        match (parses(&ex.code), ex.broken) {
            (Err(e), false) => failures.push(format!("{}: {e}\n{}", ex.page, ex.code)),
            (Ok(()), true) => failures.push(format!("{}: marked broken, but it's fine:\n{}", ex.page, ex.code)),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "examples that don't parse:\n\n{}", failures.join("\n\n"));
}

#[test]
fn every_runnable_example_runs_in_a_real_game_without_errors() {
    let mut failures = Vec::new();
    let mut ran = 0;
    for ex in docs::examples().into_iter().filter(|e| e.run) {
        let mut game = Game::start(world_for(&ex));
        for _ in 0..(3.0 * 60.0) as usize {
            game.step(1.0 / 60.0);
        }
        let errors: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
        if !errors.is_empty() {
            failures.push(format!("{}:\n{}\n--- errors:\n{}", ex.page, ex.code, errors.join("\n")));
        }
        ran += 1;
    }
    assert!(ran > 40, "most examples run for real: {ran}");
    assert!(failures.is_empty(), "examples that fail when run:\n\n{}", failures.join("\n\n"));
}

#[test]
fn every_page_renders_and_its_links_and_pictures_exist() {
    let slugs: Vec<&str> = docs::PAGES.iter().map(|p| p.slug).collect();
    for page in docs::PAGES {
        let html = docs::page(page.slug).expect("the page renders");
        assert!(html.contains(&format!("<h1>{}</h1>", page.title.replace('&', "&amp;"))), "{}", page.slug);
        // Links to other guide pages ("rovik-basics", "/learn/x", "parts#anchored").
        for (i, _) in html.match_indices("href=\"") {
            let target = &html[i + 6..];
            let target = &target[..target.find('"').unwrap()];
            let path = target.split('#').next().unwrap();
            if path.is_empty() || path.starts_with("http") || path.starts_with('/') && !path.starts_with("/learn/") {
                continue;
            }
            let slug = path.trim_start_matches("/learn/");
            if slug.ends_with(".css") || slug.ends_with(".svg") || slug.ends_with(".js") {
                continue;
            }
            if let Some(file) = slug.strip_prefix("samples/") {
                assert!(file.ends_with(".brixo"), "{}: sample downloads are .brixo files: {target}", page.slug);
                continue;
            }
            assert!(slugs.contains(&slug), "{} links to '{target}', which isn't a page", page.slug);
        }
        for (i, _) in html.match_indices("<img src=\"") {
            let src = &html[i + 10..];
            let src = &src[..src.find('"').unwrap()];
            let file = src.strip_prefix("img/").unwrap_or_else(|| panic!("{}: pictures live in img/: {src}", page.slug));
            // (With its fingerprint, so browsers notice a retaken picture.)
            let (file, version) = file.split_once("?v=").unwrap_or_else(|| panic!("{src} has no ?v= fingerprint"));
            assert_eq!(version.len(), 8, "{src}");
            let bytes = docs::image(file).unwrap_or_else(|| panic!("{} shows {src}, which isn't in docs/img", page.slug));
            assert!(bytes.len() > 2000, "{src} is a real picture, not a placeholder");
        }
    }
    let search = docs::search_index();
    let parsed: serde_json::Value = serde_json::from_str(search).expect("the search index is valid JSON");
    assert_eq!(parsed.as_array().unwrap().len(), docs::PAGES.len());
}

// --- The how-tos do what they say ------------------------------------------------
// Beyond "runs without errors": the main recipes, put in a world and played.

/// The example on `page` whose code contains `marker`.
fn recipe(page: &str, marker: &str) -> String {
    docs::examples()
        .into_iter()
        .find(|e| e.page == page && e.code.contains(marker))
        .unwrap_or_else(|| panic!("no example on {page} containing {marker:?}"))
        .code
}

struct World {
    dm: DataModel,
}

impl World {
    fn new() -> World {
        let mut dm = DataModel::new();
        let root = dm.root();
        let floor = dm.create(Class::Part, "Baseplate", root).unwrap();
        {
            let p = dm.part_mut(floor).unwrap();
            p.size = Vec3::new(300.0, 1.0, 300.0);
            p.position = Vec3::new(0.0, -0.5, 0.0);
        }
        let spawn = dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
        dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
        World { dm }
    }
    fn part(&mut self, name: &str, at: (f32, f32, f32), size: (f32, f32, f32)) -> brixo_core::InstanceId {
        let root = self.dm.root();
        let id = self.dm.create(Class::Part, name, root).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = Vec3::new(at.0, at.1, at.2);
        p.size = Vec3::new(size.0, size.1, size.2);
        id
    }
    fn script(&mut self, parent: Option<brixo_core::InstanceId>, code: &str) {
        let parent = parent.unwrap_or(self.dm.root());
        let s = self.dm.create(Class::Script, "Script", parent).unwrap();
        self.dm.script_mut(s).unwrap().source = code.to_string();
    }
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds * 60.0) as usize {
        game.step(1.0 / 60.0);
    }
}

fn no_errors(game: &Game) {
    let e: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(e.is_empty(), "{e:?}");
}

fn teleport(game: &Game, who: brixo_core::InstanceId, x: f32, y: f32, z: f32) {
    game.world().player_mut(who).unwrap().body.position = Vec3::new(x, y, z);
}

#[test]
fn the_kill_brick_knocks_you_out() {
    let mut w = World::new();
    let brick = w.part("Kill Brick", (0.0, 0.5, 20.0), (6.0, 1.0, 6.0));
    w.script(Some(brick), &recipe("howto-kill-brick", "other.health = 0"));
    let mut game = Game::start(w.dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    teleport(&game, me, 0.0, 3.6, 20.0);
    run(&mut game, 0.3);
    no_errors(&game);
    assert!(game.world().player(me).unwrap().dead > 0.0);
}

#[test]
fn coins_count_up_and_show_on_the_label() {
    let mut w = World::new();
    w.script(None, &recipe("howto-coins", "label.name = \"Coin Label\""));
    let coin = w.part("Coin", (0.0, 3.0, 20.0), (2.0, 2.0, 2.0));
    w.dm.part_mut(coin).unwrap().can_collide = false;
    w.script(Some(coin), &recipe("howto-coins", "other.coins += 1"));
    let mut game = Game::start(w.dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    teleport(&game, me, 0.0, 3.0, 20.0);
    run(&mut game, 0.3);
    no_errors(&game);
    let world = game.world();
    assert!(world.get(coin).is_none(), "collected");
    let label = world.get(me).unwrap().children.iter().copied().find(|c| world.get(*c).unwrap().name == "Coin Label").unwrap();
    assert_eq!(world.gui(label).unwrap().text, "Coins: 1");
}

#[test]
fn the_jump_pad_throws_you_up_and_the_teleporter_moves_you() {
    let mut w = World::new();
    let pad = w.part("Jump Pad", (0.0, 0.2, 20.0), (4.0, 0.4, 4.0));
    w.script(Some(pad), &recipe("howto-pads", "y = 80"));
    let a = w.part("Teleporter A", (30.0, 0.2, 0.0), (4.0, 0.4, 4.0));
    w.part("Teleporter B", (80.0, 0.2, 40.0), (4.0, 0.4, 4.0));
    w.script(Some(a), &recipe("howto-teleporter", "find(\"Teleporter B\")"));
    let mut game = Game::start(w.dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    teleport(&game, me, 0.0, 2.9, 20.0);
    let mut peak: f32 = 0.0;
    for _ in 0..60 {
        game.step(1.0 / 60.0);
        peak = peak.max(game.world().player(me).unwrap().body.position.y);
    }
    assert!(peak > 15.0, "launched: peak {peak}");
    run(&mut game, 2.0);
    teleport(&game, me, 30.0, 2.9, 0.0);
    run(&mut game, 0.3);
    no_errors(&game);
    let at = game.world().player(me).unwrap().body.position;
    assert!((at.x - 80.0).abs() < 2.0 && (at.z - 40.0).abs() < 2.0, "arrived at B: {at:?}");
}

#[test]
fn checkpoints_bring_you_back_where_you_got_to() {
    let mut w = World::new();
    let cp = w.part("Checkpoint 1", (40.0, 0.5, 0.0), (6.0, 1.0, 6.0));
    w.script(Some(cp), &recipe("howto-checkpoints", "stage = 1"));
    w.script(None, &recipe("howto-checkpoints", "on respawned(p)"));
    let mut game = Game::start(w.dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    teleport(&game, me, 40.0, 3.6, 0.0);
    run(&mut game, 0.3);
    game.world().player_mut(me).unwrap().health = 0.0;
    run(&mut game, 4.6);
    no_errors(&game);
    let at = game.world().player(me).unwrap().body.position;
    assert!((at.x - 40.0).abs() < 2.0, "back at the checkpoint, not the start: {at:?}");
}

#[test]
fn the_team_game_splits_teams_and_hands_out_swords() {
    let mut w = World::new();
    let root = w.dm.root();
    for (name, x) in [("Red Spawn", -60.0), ("Blue Spawn", 60.0)] {
        let s = w.dm.create(Class::SpawnLocation, name, root).unwrap();
        w.dm.part_mut(s).unwrap().position = Vec3::new(x, 0.5, 0.0);
    }
    let score = w.dm.create(Class::TextLabel, "Score", root).unwrap();
    let _ = score;
    let storage = w.dm.create(Class::Folder, "Storage", root).unwrap();
    let sword = w.dm.create(Class::Tool, "Sword", storage).unwrap();
    let h = w.dm.create(Class::Part, "Handle", sword).unwrap();
    w.dm.part_mut(h).unwrap().position = Vec3::new(0.0, -300.0, 0.0);
    w.script(Some(sword), &recipe("howto-teams", "self.grip = \"up\""));
    w.script(None, &recipe("howto-teams", "TO_WIN = 10"));
    let mut game = Game::start_server(w.dm);
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.5);
    no_errors(&game);
    let team = |id| match game.world().get(id).unwrap().attributes.get("team") {
        Some(brixo_core::Attribute::Str(t)) => t.clone(),
        _ => String::new(),
    };
    assert_eq!((team(ann).as_str(), team(bob).as_str()), ("Red", "Blue"));
    assert_eq!(game.backpack(ann).len(), 1, "a sword each");
    // Ann chops Bob four times: he's out, and Red scores.
    teleport(&game, ann, 0.0, 3.0, 0.0);
    teleport(&game, bob, 0.0, 3.0, 3.0);
    run(&mut game, 0.2);
    game.equip(ann, Some(0));
    for _ in 0..4 {
        game.activate_at(ann, Some(Vec3::new(0.0, 3.0, 3.0)));
        run(&mut game, 0.6);
    }
    no_errors(&game);
    assert!(game.world().player(bob).unwrap().dead > 0.0, "Bob's out");
    let score = game.world().find_first("Score").unwrap();
    assert_eq!(game.world().gui(score).unwrap().text, "RED 1   vs   0 BLUE");
}

#[test]
fn the_laser_gun_hits_who_you_click_on() {
    let mut w = World::new();
    let root = w.dm.root();
    let storage = w.dm.create(Class::Folder, "Storage", root).unwrap();
    let gun = w.dm.create(Class::Tool, "Laser Gun", storage).unwrap();
    let h = w.dm.create(Class::Part, "Handle", gun).unwrap();
    w.dm.part_mut(h).unwrap().position = Vec3::new(0.0, -300.0, 0.0);
    w.script(Some(gun), &recipe("howto-gun", "fn first_hit"));
    w.script(None, "on player_joined(p)\n g = clone(find(\"Laser Gun\"))\n g.parent = p\nend");
    let mut game = Game::start_server(w.dm);
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.5);
    teleport(&game, ann, 0.0, 3.0, 0.0);
    teleport(&game, bob, 0.0, 3.0, 30.0);
    run(&mut game, 0.2);
    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    let fired = game.activate_at(ann, Some(Vec3::new(0.0, 3.5, 29.4)));
    run(&mut game, 0.5);
    assert!(fired);
    no_errors(&game);
    assert_eq!(game.world().player(bob).unwrap().health, 66.0, "34 damage");
    // A miss, well wide of him, doesn't hurt.
    run(&mut game, 1.0);
    game.activate_at(ann, Some(Vec3::new(40.0, 3.0, 30.0)));
    run(&mut game, 0.5);
    assert_eq!(game.world().player(bob).unwrap().health, 66.0, "missed");
}

#[test]
fn the_shop_takes_coins_and_gives_the_upgrade() {
    let mut w = World::new();
    let root = w.dm.root();
    let button = w.dm.create(Class::TextButton, "Buy Speed", root).unwrap();
    w.script(Some(button), &recipe("howto-shop", "p.walk_speed += 4"));
    let mut game = Game::start(w.dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    game.world().get_mut(me).unwrap().attributes.insert("coins".into(), brixo_core::Attribute::Num(30.0));
    game.click(me, button);
    run(&mut game, 0.2);
    no_errors(&game);
    let p = *game.world().player(me).unwrap();
    assert_eq!(p.walk_speed, 20.0);
    assert!(matches!(game.world().get(me).unwrap().attributes.get("coins"), Some(brixo_core::Attribute::Num(n)) if *n == 5.0));
    // Too poor for a second one.
    game.click(me, button);
    run(&mut game, 0.2);
    assert_eq!(game.world().player(me).unwrap().walk_speed, 20.0);
}
