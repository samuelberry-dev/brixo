//! Physics behaviour, checked through a running game.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, LogLine};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    let frames = (seconds / FRAME).round() as usize;
    for _ in 0..frames {
        game.step(FRAME);
    }
}

/// An anchored 40x1x40 floor whose top surface is at y = 0.
fn floor(dm: &mut DataModel) -> InstanceId {
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(40.0, 1.0, 40.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    // Keep the player out of the way of these experiments.
    let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(15.0, 0.5, 15.0);
    f
}

fn crate_at(dm: &mut DataModel, name: &str, pos: Vec3) -> InstanceId {
    let root = dm.root();
    let c = dm.create(Class::Part, name, root).unwrap();
    let p = dm.part_mut(c).unwrap();
    p.position = pos;
    p.size = Vec3::new(2.0, 2.0, 2.0);
    p.anchored = false;
    c
}

fn add_script(dm: &mut DataModel, parent: InstanceId, source: &str) {
    let s = dm.create(Class::Script, "Script", parent).unwrap();
    dm.script_mut(s).unwrap().source = source.to_string();
}

fn texts(lines: &[LogLine]) -> Vec<String> {
    lines.iter().map(|l| l.text.clone()).collect()
}

fn y_of(game: &Game, id: InstanceId) -> f32 {
    game.world().part(id).unwrap().position.y
}

#[test]
fn unanchored_parts_fall_and_land() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 10.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 0.3);
    assert!(y_of(&game, c) < 9.5, "should be falling");
    run(&mut game, 3.0);
    // Resting on the floor: its centre is half its height above y = 0.
    assert!((y_of(&game, c) - 1.0).abs() < 0.05, "landed at {}", y_of(&game, c));
}

#[test]
fn anchored_parts_stay_put() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let hover = dm.create(Class::Part, "Hover", root).unwrap();
    dm.part_mut(hover).unwrap().position = Vec3::new(0.0, 10.0, 0.0);
    let mut game = Game::start(dm);
    run(&mut game, 2.0);
    assert_eq!(y_of(&game, hover), 10.0);
}

#[test]
fn falling_follows_gravity() {
    // No floor: after t seconds a dropped part has fallen g * t^2 / 2.
    let mut dm = DataModel::new();
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 100.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 1.0);
    let fallen = 100.0 - y_of(&game, c);
    let expected = brixo_runtime::GRAVITY / 2.0;
    assert!((fallen - expected).abs() < 1.0, "fell {fallen}, expected about {expected}");
}

#[test]
fn stacked_crates_stay_stacked() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let bottom = crate_at(&mut dm, "Bottom", Vec3::new(0.0, 1.0, 0.0));
    let top = crate_at(&mut dm, "Top", Vec3::new(0.0, 3.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 5.0);
    let world = game.world();
    let (b, t) = (world.part(bottom).unwrap(), world.part(top).unwrap());
    assert!((b.position.y - 1.0).abs() < 0.05 && (t.position.y - 3.0).abs() < 0.05);
    assert!(t.position.x.abs() < 0.05 && t.position.z.abs() < 0.05, "top drifted");
}

#[test]
fn an_off_centre_crate_tips_over() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let _base = crate_at(&mut dm, "Base", Vec3::new(0.0, 1.0, 0.0));
    // Hanging 1.6 of its 2 studs over the edge: it must fall off and rotate.
    let tipper = crate_at(&mut dm, "Tipper", Vec3::new(1.6, 3.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    let p = *game.world().part(tipper).unwrap();
    assert!(p.position.y < 2.0, "should have fallen off, y = {}", p.position.y);
    assert!(p.rotation.z.abs() > 5.0 || p.rotation.x.abs() > 5.0, "should have tumbled: {:?}", p.rotation);
}

#[test]
fn scripts_can_teleport_falling_parts() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 1.0, 0.0));
    add_script(&mut dm, c, "wait(1)\nself.position.y = 20");
    let mut game = Game::start(dm);
    run(&mut game, 1.05);
    assert!(y_of(&game, c) > 18.0, "teleported to {}", y_of(&game, c));
    run(&mut game, 3.0);
    assert!((y_of(&game, c) - 1.0).abs() < 0.05, "fell back down");
}

#[test]
fn scripts_can_unanchor_a_part() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let root = dm.root();
    let shelf = dm.create(Class::Part, "Shelf", root).unwrap();
    dm.part_mut(shelf).unwrap().position = Vec3::new(0.0, 8.0, 0.0);
    add_script(&mut dm, shelf, "wait(1)\nself.anchored = false");
    let mut game = Game::start(dm);
    run(&mut game, 0.9);
    assert_eq!(y_of(&game, shelf), 8.0);
    run(&mut game, 3.0);
    assert!((y_of(&game, shelf) - 0.5).abs() < 0.05, "y = {}", y_of(&game, shelf));
}

#[test]
fn a_falling_part_fires_touched_when_it_lands() {
    let mut dm = DataModel::new();
    let f = floor(&mut dm);
    add_script(&mut dm, f, "on touched(other)\n print(other.name + \" landed\")\nend");
    crate_at(&mut dm, "Crate", Vec3::new(0.0, 6.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    // Exactly once, even though the crate then rests there.
    assert_eq!(texts(&script_log(&game)), ["Crate landed"]);
}

#[test]
fn non_colliding_parts_fall_through_but_still_touch() {
    let mut dm = DataModel::new();
    let f = floor(&mut dm);
    add_script(&mut dm, f, "on touched(other)\n print(\"ghost passed\")\nend");
    let ghost = crate_at(&mut dm, "Ghost", Vec3::new(0.0, 4.0, 0.0));
    dm.part_mut(ghost).unwrap().can_collide = false;
    let mut game = Game::start(dm);
    run(&mut game, 2.0);
    assert!(y_of(&game, ghost) < -5.0, "should fall through, y = {}", y_of(&game, ghost));
    assert_eq!(texts(&script_log(&game)), ["ghost passed"]);
}

#[test]
fn anchored_pickups_are_touched_by_falling_parts() {
    // A coin that doesn't collide, floating in the crate's path.
    let mut dm = DataModel::new();
    floor(&mut dm);
    let root = dm.root();
    let coin = dm.create(Class::Part, "Coin", root).unwrap();
    {
        let p = dm.part_mut(coin).unwrap();
        p.position = Vec3::new(0.0, 4.0, 0.0);
        p.can_collide = false;
    }
    add_script(&mut dm, coin, "on touched(other)\n print(other.name + \" got the coin\")\n destroy(self)\nend");
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 10.0, 0.0));
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    assert_eq!(texts(&script_log(&game)), ["Crate got the coin"]);
    assert!(game.world().get(coin).is_none());
    assert!((y_of(&game, c) - 1.0).abs() < 0.05, "crate should pass the coin and land");
}

#[test]
fn resizing_a_part_changes_its_collision_shape() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 1.0, 0.0));
    add_script(&mut dm, c, "wait(1)\nself.size.y = 6");
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    // Now 6 tall, so it rests with its centre at 3.
    assert!((y_of(&game, c) - 3.0).abs() < 0.1, "y = {}", y_of(&game, c));
}

#[test]
fn cloned_parts_join_the_simulation() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 1.0, 0.0));
    let root = dm.root();
    let ctl = dm.create(Class::Folder, "Control", root).unwrap();
    add_script(&mut dm, ctl, "wait(0.5)\ncopy = clone(find(\"Crate\"))\ncopy.name = \"Copy\"\ncopy.position = {y = 12}");
    let mut game = Game::start(dm);
    run(&mut game, 4.0);
    let world = game.world();
    let copy = world.find_first("Copy").unwrap();
    // The copy fell and landed on top of the original.
    assert!((world.part(copy).unwrap().position.y - 3.0).abs() < 0.1);
    assert!((world.part(c).unwrap().position.y - 1.0).abs() < 0.05);
}

#[test]
fn destroyed_parts_leave_the_simulation() {
    let mut dm = DataModel::new();
    floor(&mut dm);
    let bottom = crate_at(&mut dm, "Bottom", Vec3::new(0.0, 1.0, 0.0));
    let top = crate_at(&mut dm, "Top", Vec3::new(0.0, 3.0, 0.0));
    add_script(&mut dm, bottom, "wait(1)\ndestroy(self)");
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    assert!(game.world().get(bottom).is_none());
    assert!((y_of(&game, top) - 1.0).abs() < 0.05, "top should drop onto the floor");
}

#[test]
fn scripts_can_read_physics_flags() {
    let mut dm = DataModel::new();
    let c = crate_at(&mut dm, "Crate", Vec3::new(0.0, 50.0, 0.0));
    add_script(&mut dm, c, "print(self.anchored, self.can_collide)\nself.can_collide = false\nprint(self.can_collide)\nself.anchored = \"yes\"");
    let game = Game::start(dm);
    let log = script_log(&game);
    assert_eq!(texts(&log[..2]), ["false true", "false"]);
    assert!(log[2].is_error && log[2].text.contains("true or false"));
}

#[test]
fn the_same_scene_plays_out_the_same_way_every_time() {
    // A tumbling pile is chaotic: any difference in setup order shows up.
    fn pile() -> Vec<Vec3> {
        let mut dm = DataModel::new();
        floor(&mut dm);
        let ids: Vec<InstanceId> = (0..12)
            .map(|i| crate_at(&mut dm, "C", Vec3::new((i % 3) as f32 * 1.3, 2.0 + i as f32 * 2.1, 0.0)))
            .collect();
        let mut game = Game::start(dm);
        run(&mut game, 4.0);
        let world = game.world();
        ids.iter().map(|id| world.part(*id).unwrap().position).collect()
    }
    let first = pile();
    for _ in 0..3 {
        assert_eq!(pile(), first);
    }
}

/// Output from scripts, leaving out Brixo's own messages (every game now
/// has a player, and in these floorless scenes it falls off the world).
fn script_log(game: &Game) -> Vec<LogLine> {
    game.take_log().into_iter().filter(|l| l.source != "Brixo").collect()
}
