//! Hinges and motors: doors, wheels, drawbridges and spinners.

use brixo_core::{Class, DataModel, Hinge, InstanceId, Shape, Side, Vec3};
use brixo_runtime::Game;

const FRAME: f64 = 1.0 / 60.0;

fn floor() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(400.0, 1.0, 400.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(-150.0, 0.5, -150.0);
    dm
}

fn part(dm: &mut DataModel, parent: InstanceId, name: &str, pos: Vec3, size: Vec3, anchored: bool) -> InstanceId {
    let id = dm.create(Class::Part, name, parent).unwrap();
    let p = dm.part_mut(id).unwrap();
    p.position = pos;
    p.size = size;
    p.anchored = anchored;
    id
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn at(game: &Game, id: InstanceId) -> Vec3 {
    game.world().part(id).unwrap().position
}

fn turn(game: &Game, id: InstanceId) -> Vec3 {
    game.world().part(id).unwrap().rotation
}

/// A door 4 wide, 7 tall, hinged on its left edge to a post, floating
/// just above the floor so only the hinge holds it.
fn door_scene() -> (DataModel, InstanceId) {
    let mut dm = floor();
    let root = dm.root();
    part(&mut dm, root, "Post", Vec3::new(-2.5, 4.0, 0.0), Vec3::new(1.0, 8.0, 1.0), true);
    let door = part(&mut dm, root, "Door", Vec3::new(0.0, 4.0, 0.0), Vec3::new(4.0, 7.0, 0.4), false);
    let p = dm.part_mut(door).unwrap();
    p.hinge = Hinge::Y;
    p.hinge_at = Side::Left;
    (dm, door)
}

#[test]
fn a_door_swings_open_on_its_hinge_and_stays_hinged() {
    let (mut dm, door) = door_scene();
    dm.part_mut(door).unwrap().swing_to = Some(90.0);
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    let p = at(&game, door);
    // Open 90 degrees: the far edge swung round, the hinged edge stayed put.
    assert!((p.x - -2.0).abs() < 0.2, "turned about its left edge: {p:?}");
    assert!((p.z.abs() - 2.0).abs() < 0.2, "swung round to the side: {p:?}");
    assert!((p.y - 4.0).abs() < 0.2, "didn't sag or fall: {p:?}");
    assert!((turn(&game, door).y.abs() - 90.0).abs() < 3.0, "{:?}", turn(&game, door));
    // Close it again.
    game.world().part_mut(door).unwrap().swing_to = Some(0.0);
    run(&mut game, 3.0);
    let p = at(&game, door);
    assert!(p.x.abs() < 0.2 && p.z.abs() < 0.2, "shut: {p:?}");
}

#[test]
fn a_hinge_without_a_motor_swings_freely() {
    // A sign hanging from a bar by its top edge, let go swung out to one
    // side: it swings through the bottom and settles hanging straight down.
    let mut dm = floor();
    let root = dm.root();
    part(&mut dm, root, "Bar", Vec3::new(0.0, 10.25, 0.0), Vec3::new(4.0, 0.5, 0.5), true);
    let sign = part(&mut dm, root, "Sign", Vec3::new(0.0, 9.0, -1.732), Vec3::new(3.0, 4.0, 0.2), false);
    {
        let p = dm.part_mut(sign).unwrap();
        p.rotation = Vec3::new(60.0, 0.0, 0.0);
        p.hinge = Hinge::X;
        p.hinge_at = Side::Top;
    }
    let mut game = Game::start(dm);
    let mut other_side = false;
    for _ in 0..90 {
        run(&mut game, FRAME);
        other_side |= at(&game, sign).z > 0.8;
    }
    assert!(other_side, "swung through to the other side");
    run(&mut game, 10.0);
    let p = at(&game, sign);
    assert!((p.y - 8.0).abs() < 0.3 && p.z.abs() < 0.3, "hangs straight again: {p:?}");
    assert!(p.x.abs() < 0.05, "stayed on its hinge line: {p:?}");
}

#[test]
fn a_motor_keeps_a_spinner_turning() {
    let mut dm = floor();
    let root = dm.root();
    // A spinning bar on a post: hinged at its middle, turning around its height.
    part(&mut dm, root, "Post", Vec3::new(0.0, 1.5, 0.0), Vec3::new(1.0, 3.0, 1.0), true);
    let bar = part(&mut dm, root, "Bar", Vec3::new(0.0, 3.5, 0.0), Vec3::new(12.0, 1.0, 1.0), false);
    {
        let p = dm.part_mut(bar).unwrap();
        p.hinge = Hinge::Y;
        p.hinge_at = Side::Bottom;
        p.motor_speed = 90.0;
    }
    let mut game = Game::start(dm);
    run(&mut game, 0.5);
    let y1 = turn(&game, bar).y;
    run(&mut game, 0.5);
    let y2 = turn(&game, bar).y;
    let turned = (y2 - y1 + 540.0).rem_euclid(360.0) - 180.0;
    assert!((turned.abs() - 45.0).abs() < 8.0, "about 90 degrees a second: {y1} -> {y2}");
    let p = at(&game, bar);
    assert!(p.x.abs() < 0.1 && p.z.abs() < 0.1 && (p.y - 3.5).abs() < 0.1, "turning in place on the post: {p:?}");
}

#[test]
fn wheels_with_motors_drive_a_car() {
    // A car: a body (the Model's first part) and four cylinder wheels
    // turned on their sides, hinged at their middles, motors on.
    let mut dm = floor();
    let root = dm.root();
    let car = dm.create(Class::Model, "Car", root).unwrap();
    let body = part(&mut dm, car, "Body", Vec3::new(0.0, 2.0, 0.0), Vec3::new(4.0, 1.0, 8.0), false);
    let mut wheels = Vec::new();
    for (x, z) in [(-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5), (2.5, 2.5)] {
        let w = part(&mut dm, car, "Wheel", Vec3::new(x, 1.5, z), Vec3::new(3.0, 1.0, 3.0), false);
        let p = dm.part_mut(w).unwrap();
        p.shape = Shape::Cylinder;
        p.rotation = Vec3::new(0.0, 0.0, 90.0);
        p.hinge = Hinge::Y;
        // The wheels all turn the same way round the same world line, so
        // they share a speed (opposite speeds on each side spin the car
        // on the spot, like a tank).
        p.motor_speed = 360.0;
        wheels.push(w);
    }
    let mut game = Game::start(dm);
    run(&mut game, 1.0);
    let start = at(&game, body);
    run(&mut game, 3.0);
    let end = at(&game, body);
    let moved = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
    assert!(moved > 10.0, "the car drove: {start:?} -> {end:?}");
    assert!(end.y > 1.0 && end.y < 4.0, "on its wheels, not flipped or sunk: {end:?}");
    // The wheels stayed on.
    for w in wheels {
        let p = at(&game, w);
        let d = ((p.x - end.x).powi(2) + (p.z - end.z).powi(2)).sqrt();
        assert!(d < 4.5, "wheel still on the car: {p:?} vs body {end:?}");
    }
}

#[test]
fn hinging_a_models_first_part_swings_the_whole_model() {
    // A two-part door: the panel (first) is hinged, the handle comes along.
    let mut dm = floor();
    let root = dm.root();
    part(&mut dm, root, "Post", Vec3::new(-2.5, 4.0, 0.0), Vec3::new(1.0, 8.0, 1.0), true);
    let model = dm.create(Class::Model, "Door", root).unwrap();
    let panel = part(&mut dm, model, "Panel", Vec3::new(0.0, 4.0, 0.0), Vec3::new(4.0, 7.0, 0.4), false);
    let handle = part(&mut dm, model, "Handle", Vec3::new(1.5, 4.0, 0.4), Vec3::new(0.4, 0.4, 0.4), false);
    {
        let p = dm.part_mut(panel).unwrap();
        p.hinge = Hinge::Y;
        p.hinge_at = Side::Left;
        p.swing_to = Some(-90.0);
    }
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    let (p, h) = (at(&game, panel), at(&game, handle));
    assert!((p.z.abs() - 2.0).abs() < 0.3, "panel open: {p:?}");
    assert!((h.z.abs() - 3.5).abs() < 0.4 && (h.x - -2.0).abs() < 0.8, "handle came along: {h:?}");
}

#[test]
fn scripts_open_doors_and_read_the_angle() {
    let (mut dm, door) = door_scene();
    dm.part_mut(door).unwrap().hinge = Hinge::Off;
    dm.part_mut(door).unwrap().anchored = true;
    let s = dm.create(Class::Script, "Opener", door).unwrap();
    dm.script_mut(s).unwrap().source = "self.hinge = \"y\"\nself.hinge_at = \"left\"\nwait(0.5)\nself.swing_to = 90\nwait(3)\nprint(\"open at \" + self.hinge_angle)\nprint(self.hinge + \" \" + self.hinge_at + \" \" + self.swing_to)\nself.swing_to = nil\n".into();
    let mut game = Game::start(dm);
    run(&mut game, 0.4);
    assert!(!game.world().part(door).unwrap().anchored, "hinging a part loosens it");
    run(&mut game, 3.5);
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    let open = log.iter().find(|l| l.starts_with("open at ")).expect("printed the angle");
    let angle: f32 = open["open at ".len()..].parse().unwrap();
    assert!((angle.abs() - 90.0).abs() < 3.0, "{log:?}");
    assert!(log.iter().any(|l| l == "y left 90"), "{log:?}");
    assert_eq!(game.world().part(door).unwrap().swing_to, None);
}

#[test]
fn hinge_mistakes_are_explained() {
    let mut dm = floor();
    let root = dm.root();
    let p = part(&mut dm, root, "P", Vec3::new(0.0, 3.0, 0.0), Vec3::new(1.0, 1.0, 1.0), true);
    let s = dm.create(Class::Script, "S", p).unwrap();
    dm.script_mut(s).unwrap().source = "self.hinge = \"sideways\"\n".into();
    let s2 = dm.create(Class::Script, "S2", p).unwrap();
    dm.script_mut(s2).unwrap().source = "self.hinge_angle = 5\n".into();
    let mut game = Game::start(dm);
    run(&mut game, 0.1);
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    assert!(log.iter().any(|l| l.contains("\"y\" (height)")), "{log:?}");
    assert!(log.iter().any(|l| l.contains("set swing_to")), "{log:?}");
}

#[test]
fn an_anchored_part_with_a_hinge_stays_put() {
    let (mut dm, door) = door_scene();
    {
        let p = dm.part_mut(door).unwrap();
        p.anchored = true;
        p.motor_speed = 90.0;
    }
    let mut game = Game::start(dm);
    run(&mut game, 1.0);
    let p = at(&game, door);
    assert!(p.x.abs() < 1e-3 && p.z.abs() < 1e-3, "{p:?}");
}
