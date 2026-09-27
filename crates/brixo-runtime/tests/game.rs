//! Runs real scripts in a real game and checks what happens.

use brixo_core::{Class, Color, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, LogLine};

/// A scene with a part, and a script inside it.
fn scene_with_script(part_name: &str, source: &str) -> (DataModel, InstanceId) {
    let mut dm = DataModel::new();
    let root = dm.root();
    let part = dm.create(Class::Part, part_name, root).unwrap();
    add_script(&mut dm, part, "Script", source);
    (dm, part)
}

fn add_script(dm: &mut DataModel, parent: InstanceId, name: &str, source: &str) -> InstanceId {
    let s = dm.create(Class::Script, name, parent).unwrap();
    dm.script_mut(s).unwrap().source = source.to_string();
    s
}

fn texts(lines: &[LogLine]) -> Vec<String> {
    lines.iter().map(|l| l.text.clone()).collect()
}

#[test]
fn script_runs_on_start_and_knows_self() {
    let (dm, _) = scene_with_script("Coin", "print(\"hi from \" + self.name)");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert_eq!(texts(&log), ["hi from Coin"]);
    assert_eq!(log[0].source, "Coin/Script");
    assert!(!log[0].is_error);
}

#[test]
fn scripts_move_parts_through_live_properties() {
    let (dm, part) = scene_with_script(
        "Box",
        "self.position.y += 5\nself.size.x = 4\nself.color.r = 255\nself.rotation = {y = 90}",
    );
    let game = Game::start(dm);
    assert!(script_log(&game).is_empty(), "no errors expected");
    let world = game.world();
    let p = world.part(part).unwrap();
    assert_eq!(p.position, Vec3::new(0.0, 5.0, 0.0));
    assert_eq!(p.size.x, 4.0);
    assert_eq!(p.color.r, 255);
    assert_eq!(p.rotation, Vec3::new(0.0, 90.0, 0.0));
}

#[test]
fn whole_vectors_and_colors_can_be_assigned() {
    let (dm, part) = scene_with_script(
        "Box",
        "self.position = {x = 1, y = 2, z = 3}\nself.color = {r = 10, g = 20, b = 300}\nprint(self.position, self.color)",
    );
    let game = Game::start(dm);
    assert_eq!(texts(&script_log(&game)), ["(1, 2, 3) color(10, 20, 255)"]);
    assert_eq!(game.world().part(part).unwrap().color, Color::new(10, 20, 255));
}

#[test]
fn wait_pauses_only_its_own_script() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let a = dm.create(Class::Part, "A", root).unwrap();
    let b = dm.create(Class::Part, "B", root).unwrap();
    add_script(&mut dm, a, "Slow", "print(\"a1\")\nwait(1)\nprint(\"a2\")");
    add_script(&mut dm, b, "Fast", "print(\"b1\")");

    let mut game = Game::start(dm);
    assert_eq!(texts(&script_log(&game)), ["a1", "b1"]);
    assert_eq!(game.waiting_tasks(), 1);

    game.step(0.5);
    assert!(script_log(&game).is_empty(), "too early to wake");
    game.step(0.6);
    assert_eq!(texts(&script_log(&game)), ["a2"]);
    assert_eq!(game.waiting_tasks(), 0);
}

#[test]
fn wait_works_inside_loops_and_functions() {
    let src = r#"
count = 0
fn tick()
    wait(1)
    count += 1
    print("count " + count)
end
while count < 3 do
    tick()
end
print("done")
"#;
    let (dm, _) = scene_with_script("P", src);
    let mut game = Game::start(dm);
    let mut all = Vec::new();
    for _ in 0..4 {
        game.step(1.0);
        all.extend(texts(&script_log(&game)));
    }
    assert_eq!(all, ["count 1", "count 2", "count 3", "done"]);
}

#[test]
fn every_fires_on_schedule() {
    let (dm, _) = scene_with_script("P", "n = 0\nevery 1 seconds\n n += 1\n print(\"tick \" + n)\nend");
    let mut game = Game::start(dm);
    for _ in 0..3 {
        game.step(1.0);
    }
    assert_eq!(texts(&script_log(&game)), ["tick 1", "tick 2", "tick 3"]);
}

#[test]
fn a_timer_never_overlaps_its_own_previous_run() {
    let src = "every 1 seconds\n print(\"start\")\n wait(2.5)\n print(\"end\")\nend";
    let (dm, _) = scene_with_script("P", src);
    let mut game = Game::start(dm);
    for _ in 0..5 {
        game.step(1.0);
    }
    // Fires at 1, is still busy at 2 and 3, finishes at 3.5 (seen at 4), fires again at 4 or 5.
    let log = texts(&script_log(&game));
    let starts = log.iter().filter(|t| *t == "start").count();
    assert_eq!(starts, 2, "{log:?}");
}

#[test]
fn touched_fires_when_parts_start_overlapping() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let coin = dm.create(Class::Part, "Coin", root).unwrap();
    let mover = dm.create(Class::Part, "Mover", root).unwrap();
    dm.part_mut(coin).unwrap().position = Vec3::new(5.0, 0.0, 0.0);
    add_script(&mut dm, coin, "Pickup", "on touched(other)\n print(\"touched by \" + other.name)\n destroy(self)\nend");
    // Slides one unit right each second.
    add_script(&mut dm, mover, "Slide", "while true do\n wait(1)\n self.position.x += 1\nend");

    let mut game = Game::start(dm);
    let mut all = Vec::new();
    for _ in 0..6 {
        game.step(1.0);
        all.extend(texts(&script_log(&game)));
    }
    assert_eq!(all, ["touched by Mover"]);
    assert!(game.world().get(coin).is_none(), "the coin destroyed itself");
}

#[test]
fn parts_already_touching_at_start_do_not_fire() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let a = dm.create(Class::Part, "A", root).unwrap();
    let _b = dm.create(Class::Part, "B", root).unwrap(); // same spot as A
    add_script(&mut dm, a, "S", "on touched(other)\n print(\"touch\")\nend");
    let mut game = Game::start(dm);
    game.step(0.1);
    assert!(script_log(&game).is_empty());
}

#[test]
fn resting_on_a_surface_is_not_touching() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    dm.part_mut(floor).unwrap().size = Vec3::new(10.0, 1.0, 10.0);
    let block = dm.create(Class::Part, "Block", root).unwrap();
    dm.part_mut(block).unwrap().position = Vec3::new(0.0, 2.0, 0.0);
    // Lowers the block until it sits exactly on the floor (y = 1).
    // (The player, dropping in from above, may land on it: that's a real touch.)
    add_script(&mut dm, block, "Drop", "on touched(o)\n if o.class != \"player\" then\n  print(\"touch\")\n end\nend\nwait(0.1)\nself.position.y = 1");
    let mut game = Game::start(dm);
    game.step(0.2);
    game.step(0.2);
    assert!(script_log(&game).is_empty());
}

#[test]
fn an_error_stops_only_that_script() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let a = dm.create(Class::Part, "A", root).unwrap();
    let b = dm.create(Class::Part, "B", root).unwrap();
    add_script(&mut dm, a, "Broken", "print(\"before\")\nprint(missing_thing)\nprint(\"never\")");
    add_script(&mut dm, b, "Fine", "wait(1)\nprint(\"still running\")");
    let mut game = Game::start(dm);
    game.step(1.0);
    let log = script_log(&game);
    assert_eq!(log[0].text, "before");
    assert!(log[1].is_error && log[1].text.contains("line 2") && log[1].source == "A/Broken");
    assert_eq!(log[2].text, "still running");
    assert_eq!(log.len(), 3);
}

#[test]
fn syntax_errors_are_reported_and_skipped() {
    let (dm, _) = scene_with_script("P", "if true then\n print(1)\n");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert_eq!(log.len(), 1);
    assert!(log[0].is_error && log[0].text.contains("missing its 'end'"));
}

#[test]
fn a_runaway_loop_is_stopped_without_freezing() {
    let (dm, _) = scene_with_script("P", "while true do\nend");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert!(log[0].is_error && log[0].text.contains("too long"));
}

#[test]
fn stopping_mid_wait_shuts_down_cleanly() {
    let (dm, _) = scene_with_script("P", "while true do\n wait(1)\nend");
    let mut game = Game::start(dm);
    game.step(1.0);
    assert_eq!(game.waiting_tasks(), 1);
    drop(game); // must not hang or panic
}

#[test]
fn find_clone_and_destroy() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let template = dm.create(Class::Part, "Template", root).unwrap();
    dm.part_mut(template).unwrap().position = Vec3::new(1.0, 1.0, 1.0);
    let ctrl = dm.create(Class::Folder, "Control", root).unwrap();
    add_script(&mut dm, ctrl, "Spawner", r#"
t = find("Template")
copy = clone(t)
copy.name = "Copy"
copy.position.x = 10
print(type(copy), copy, copy.position)
destroy(t)
print(find("Template"), find("Copy"))
"#);
    let game = Game::start(dm);
    assert_eq!(
        texts(&script_log(&game)),
        ["part part \"Copy\" (10, 1, 1)", "nil part \"Copy\""]
    );
    assert!(game.world().get(template).is_none());
}

#[test]
fn using_a_destroyed_part_gives_a_clear_error() {
    let (dm, _) = scene_with_script("P", "destroy(self)\nprint(self.name)");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert!(log[0].is_error && log[0].text.contains("was destroyed"), "{log:?}");
}

#[test]
fn destroying_a_script_stops_its_waiting_tasks() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let a = dm.create(Class::Part, "A", root).unwrap();
    let looper = add_script(&mut dm, a, "Loop", "while true do\n wait(1)\n print(\"loop\")\nend");
    let b = dm.create(Class::Part, "B", root).unwrap();
    let _ = looper;
    add_script(&mut dm, b, "Killer", "wait(1.5)\ndestroy(find(\"Loop\"))");
    let mut game = Game::start(dm);
    let mut all = Vec::new();
    for _ in 0..4 {
        game.step(1.0);
        all.extend(texts(&script_log(&game)));
    }
    // Prints at 1 and at 2 (the killer wakes at 2 too, after Loop), then stops.
    assert!(all.len() <= 2 && !all.is_empty(), "{all:?}");
    assert_eq!(game.waiting_tasks(), 0);
}

#[test]
fn unknown_events_are_reported() {
    let (dm, _) = scene_with_script("P", "on exploded()\n print(1)\nend");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert!(log[0].is_error && log[0].text.contains("isn't an event"));
}

#[test]
fn misspelled_fields_suggest_the_real_one() {
    let (dm, _) = scene_with_script("P", "print(self.colour)");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert!(log[0].is_error && log[0].text.contains("Did you mean 'color'?"), "{log:?}");
}

#[test]
fn time_counts_game_seconds() {
    let (dm, _) = scene_with_script("P", "wait(2)\nprint(time())");
    let mut game = Game::start(dm);
    game.step(1.0);
    game.step(1.0);
    assert_eq!(texts(&script_log(&game)), ["2"]);
}

#[test]
fn disabled_scripts_do_not_run() {
    let (mut dm, part) = scene_with_script("P", "print(\"should not run\")");
    let script = dm.get(part).unwrap().children[0];
    dm.script_mut(script).unwrap().enabled = false;
    let game = Game::start(dm);
    assert!(script_log(&game).is_empty());
}

/// Output from scripts, leaving out Brixo's own messages (every game now
/// has a player, and in these floorless scenes it falls off the world).
fn script_log(game: &Game) -> Vec<LogLine> {
    game.take_log().into_iter().filter(|l| l.source != "Brixo").collect()
}

#[test]
fn saved_data_survives_leaving_and_coming_back() {
    use std::sync::Arc;
    let store = Arc::new(brixo_runtime::MemoryStore::default());
    let scene = || {
        let mut dm = DataModel::new();
        let root = dm.root();
        let s = dm.create(Class::Script, "Coins", root).unwrap();
        dm.script_mut(s).unwrap().source = "on player_joined(p)\n    p.coins = load(p, \"coins\") or 0\n    p.visits = (load(p, \"stats\") or {visits = 0}).visits + 1\n    save(p, \"stats\", {visits = p.visits, best = [1, 2, 3], name = p.name})\nend\non player_left(p)\n    save(p, \"coins\", p.coins)\nend\n".into();
        dm
    };
    let coins = |g: &Game, p| match g.world().get(p).unwrap().attributes.get("coins") {
        Some(brixo_core::Attribute::Num(n)) => *n,
        other => panic!("coins: {other:?}"),
    };
    // First visit: nothing saved yet.
    let mut game = Game::start_server(scene());
    game.set_save_store(store.clone());
    let ann = game.add_player_saved("Ann", None, Some("user-7"));
    game.step(0.1);
    assert_eq!(coins(&game, ann), 0.0);
    game.world().get_mut(ann).unwrap().attributes.insert("coins".into(), brixo_core::Attribute::Num(42.0));
    game.remove_player(ann);
    let errors: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(errors.is_empty(), "{errors:?}");
    drop(game);
    // A new server later: Ann (the same account, another name) has her coins.
    let mut game = Game::start_server(scene());
    game.set_save_store(store.clone());
    let ann = game.add_player_saved("Ann2", None, Some("user-7"));
    game.step(0.1);
    assert_eq!(coins(&game, ann), 42.0);
    let visits = match game.world().get(ann).unwrap().attributes.get("visits") {
        Some(brixo_core::Attribute::Num(n)) => *n,
        other => panic!("visits: {other:?}"),
    };
    assert_eq!(visits, 2.0, "maps and lists come back too");
    // Someone else's data is their own.
    let bob = game.add_player_saved("Bob", None, Some("user-9"));
    game.step(0.1);
    assert_eq!(coins(&game, bob), 0.0);
}

#[test]
fn save_explains_what_it_cant_keep() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let s = dm.create(Class::Script, "Bad", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n    save(p, \"where\", p)\nend\n".into();
    let mut game = Game::start(dm);
    game.step(0.1);
    let errors: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(errors.iter().any(|e| e.contains("can't save a part or a player")), "{errors:?}");
    // Too much: one player gets 64 KB in a game.
    let mut dm = DataModel::new();
    let root = dm.root();
    let s = dm.create(Class::Script, "Big", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n    big = \"\"\n    for i in 1..700 do\n        big = big + \"0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789\"\n    end\n    save(p, \"big\", big)\nend\n".into();
    let mut game = Game::start(dm);
    game.step(0.1);
    let errors: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(errors.iter().any(|e| e.contains("too much to save")), "{errors:?}");
}
