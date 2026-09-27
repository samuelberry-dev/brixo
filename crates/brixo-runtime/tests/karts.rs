//! Karts: driving, drifting, boosts, walls, ramps, bots, and scripts.

use brixo_core::{Attribute, Class, DataModel, InstanceId, Shape, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn part(dm: &mut DataModel, parent: InstanceId, name: &str, at: (f32, f32, f32), size: (f32, f32, f32)) -> InstanceId {
    let id = dm.create(Class::Part, name, parent).unwrap();
    let p = dm.part_mut(id).unwrap();
    p.position = Vec3::new(at.0, at.1, at.2);
    p.size = Vec3::new(size.0, size.1, size.2);
    id
}

/// A kart: chassis, seat, four wheels, facing +Z at `at`.
fn kart(dm: &mut DataModel, name: &str, at: (f32, f32)) -> InstanceId {
    let root = dm.root();
    let k = dm.create(Class::Model, name, root).unwrap();
    dm.get_mut(k).unwrap().attributes.insert("kart".into(), Attribute::Bool(true));
    let (x, z) = at;
    part(dm, k, "Chassis", (x, 1.2, z), (3.0, 0.6, 5.0));
    part(dm, k, "Seat", (x, 1.7, z - 0.5), (1.6, 0.4, 1.6));
    for (dx, dz, n) in [(-1.8, 1.6, "Front Wheel L"), (1.8, 1.6, "Front Wheel R"), (-1.8, -1.6, "Wheel L"), (1.8, -1.6, "Wheel R")] {
        let w = part(dm, k, n, (x + dx, 0.8, z + dz), (1.6, 0.6, 1.6));
        let p = dm.part_mut(w).unwrap();
        p.shape = Shape::Cylinder;
        p.rotation = Vec3::new(0.0, 0.0, 90.0);
    }
    k
}

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    part(&mut dm, root, "Floor", (0.0, -0.5, 0.0), (1000.0, 1.0, 1000.0));
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(-100.0, 0.5, -100.0);
    dm
}

fn chassis(game: &Game, k: InstanceId) -> brixo_core::PartProps {
    let w = game.world();
    let c = brixo_core::kart_chassis(&w, k).unwrap();
    *w.part(c).unwrap()
}

fn num(game: &Game, id: InstanceId, key: &str) -> f64 {
    match game.world().get(id).unwrap().attributes.get(key) {
        Some(Attribute::Num(n)) => *n,
        _ => 0.0,
    }
}

fn seat(game: &mut Game, me: InstanceId, k: InstanceId) {
    game.world().player_mut(me).unwrap().kart = Some(k);
    run(game, 0.2);
}

fn keys(throttle: f32, steer: f32, drift: bool) -> PlayerInput {
    PlayerInput { move_x: steer, move_z: throttle, jump: drift }
}

#[test]
fn a_kart_drives_turns_and_its_parts_ride_along() {
    let mut dm = arena();
    let k = kart(&mut dm, "Kart", (0.0, 0.0));
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    seat(&mut game, me, k);
    let start = chassis(&game, k).position;
    // The driver sits in it.
    let p = game.world().player(me).unwrap().body.position;
    assert!((p.x - start.x).abs() < 1.0 && p.y > start.y + 1.0, "sitting in the seat: {p:?} vs {start:?}");

    game.set_input_for(me, keys(1.0, 0.0, false));
    run(&mut game, 3.0);
    let c = chassis(&game, k);
    assert!(c.position.z > start.z + 80.0, "drove forward: {:?}", c.position);
    assert!(c.position.x.abs() < 1.0, "straight: {:?}", c.position);
    assert!((c.position.y - start.y).abs() < 0.3, "on the ground: {:?}", c.position);
    let speed = num(&game, k, "speed");
    assert!(speed > 60.0 && speed < 75.0, "near top speed: {speed}");
    // The wheels came along (and turned).
    let w = game.world();
    let wheel = w.get(k).unwrap().children.iter().copied().find(|c| w.get(*c).unwrap().name == "Wheel L").unwrap();
    let wp = w.part(wheel).unwrap().position;
    assert!((wp.z - (c.position.z - 1.6)).abs() < 0.2 && (wp.x - (c.position.x - 1.8)).abs() < 0.2, "wheel rode along: {wp:?}");
    drop(w);

    // Turning left: facing more toward +X.
    game.set_input_for(me, keys(1.0, 1.0, false));
    run(&mut game, 0.5);
    let yaw = chassis(&game, k).rotation.y;
    assert!(yaw > 20.0, "turned left: {yaw}");
    // Brakes.
    game.set_input_for(me, keys(-1.0, 0.0, false));
    run(&mut game, 0.65);
    assert!(num(&game, k, "speed") < 8.0, "stopped: {}", num(&game, k, "speed"));
    // Out of the kart: beside it, walking again.
    game.world().player_mut(me).unwrap().kart = None;
    run(&mut game, 0.3);
    game.set_input_for(me, PlayerInput { move_x: 0.0, move_z: -1.0, jump: false });
    let before = game.world().player(me).unwrap().body.position;
    run(&mut game, 0.5);
    let after = game.world().player(me).unwrap().body.position;
    assert!(after.z < before.z - 3.0, "walking: {before:?} -> {after:?}");
}

#[test]
fn drifting_charges_a_boost() {
    let mut dm = arena();
    let k = kart(&mut dm, "Kart", (0.0, 0.0));
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    seat(&mut game, me, k);
    game.set_input_for(me, keys(1.0, 0.0, false));
    run(&mut game, 3.0);
    // Hold drift and steer: a hop, then a long slide.
    game.set_input_for(me, keys(1.0, 1.0, true));
    run(&mut game, 0.1);
    assert!(num(&game, k, "drift") > 0.0, "drifting");
    run(&mut game, 2.0);
    assert_eq!(num(&game, k, "drift"), 2.0, "charged all the way");
    // Let go: a boost, faster than top speed.
    game.set_input_for(me, keys(1.0, 0.0, false));
    run(&mut game, 0.6);
    assert!(matches!(game.world().get(k).unwrap().attributes.get("boosting"), Some(Attribute::Bool(true))));
    assert!(num(&game, k, "speed") > 75.0, "boosted: {}", num(&game, k, "speed"));
}

#[test]
fn walls_stop_karts_and_ramps_throw_them() {
    let mut dm = arena();
    let root = dm.root();
    part(&mut dm, root, "Wall", (0.0, 3.0, 60.0), (40.0, 6.0, 2.0));
    let ramp = part(&mut dm, root, "Ramp", (30.0, 2.0, 40.0), (10.0, 4.0, 12.0));
    dm.part_mut(ramp).unwrap().shape = Shape::Wedge;
    let k = kart(&mut dm, "Kart", (0.0, 0.0));
    let j = kart(&mut dm, "Jumper", (30.0, 0.0));
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    let bob = game.add_player("Bob");
    seat(&mut game, me, k);
    seat(&mut game, bob, j);
    game.set_input_for(me, keys(1.0, 0.0, false));
    game.set_input_for(bob, keys(1.0, 0.0, false));
    let mut highest: f32 = 0.0;
    for _ in 0..180 {
        game.step(FRAME);
        highest = highest.max(chassis(&game, j).position.y);
    }
    let c = chassis(&game, k).position;
    assert!(c.z < 59.0 && c.z > 52.0, "stopped at the wall: {c:?}");
    assert!(highest > 6.0, "the ramp threw it: {highest}");
    let landed = chassis(&game, j).position;
    assert!(landed.y < 2.0 && landed.z > 55.0, "landed beyond: {landed:?}");
}

#[test]
fn scripts_boost_spin_place_and_seat() {
    let mut dm = arena();
    let k = kart(&mut dm, "Kart", (0.0, 0.0));
    let root = dm.root();
    let pad = part(&mut dm, root, "Boost Pad", (0.0, 0.1, 40.0), (8.0, 0.2, 6.0));
    dm.part_mut(pad).unwrap().can_collide = false;
    let s = dm.create(Class::Script, "Pad", pad).unwrap();
    dm.script_mut(s).unwrap().source = "on touched(other)\n    if other.class == \"player\" and other.kart != nil then\n        boost(other.kart, 1)\n        print(\"boost \" + other.name + \" \" + other.kart.driver.name)\n    end\nend\n".into();
    let s2 = dm.create(Class::Script, "Keys", root).unwrap();
    dm.script_mut(s2).unwrap().source = "on key(p, k)\n    print(p.name + \" pressed \" + k)\n    if k == \"e\" then\n        spin_out(p.kart)\n    end\n    if k == \"r\" then\n        place_kart(p.kart, {x = 50, y = 1.2, z = 50}, 90)\n    end\nend\n".into();
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    seat(&mut game, me, k);
    game.set_input_for(me, keys(1.0, 0.0, false));
    run(&mut game, 1.5);
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    assert!(log.iter().any(|l| l == "boost Ann Ann"), "{log:?}");
    assert!(matches!(game.world().get(k).unwrap().attributes.get("boosting"), Some(Attribute::Bool(true))));

    assert!(game.key(me, "E"));
    assert!(!game.key(me, "w"), "not a script key");
    run(&mut game, 0.2);
    assert!(matches!(game.world().get(k).unwrap().attributes.get("spinning"), Some(Attribute::Bool(true))));
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    assert!(log.iter().any(|l| l == "Ann pressed e"), "{log:?}");
    game.set_input_for(me, keys(0.0, 0.0, false));
    game.key(me, "r");
    run(&mut game, 0.3);
    let c = chassis(&game, k);
    assert!((c.position.x - 50.0).abs() < 0.5 && (c.position.z - 50.0).abs() < 0.5, "placed: {:?}", c.position);
    assert!((c.rotation.y - 90.0).abs() < 1.0);
}

#[test]
fn bots_drive_round_the_racing_line() {
    let mut dm = arena();
    let root = dm.root();
    // A square loop, 120 across, as the racing line.
    let line = dm.create(Class::Folder, "Line", root).unwrap();
    let corners = [(0.0, 0.0), (0.0, 120.0), (120.0, 120.0), (120.0, 0.0)];
    let mut n = 1;
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        for step in 0..6 {
            let t = step as f32 / 6.0;
            let p = part(&mut dm, line, &n.to_string(), (a.0 + (b.0 - a.0) * t, 0.5, a.1 + (b.1 - a.1) * t), (1.0, 1.0, 1.0));
            dm.part_mut(p).unwrap().can_collide = false;
            dm.part_mut(p).unwrap().transparency = 1.0;
            n += 1;
        }
    }
    dm.get_mut(root).unwrap().attributes.insert("racing_line".into(), Attribute::Str("Line".into()));
    let k = kart(&mut dm, "Bot Kart", (0.0, 5.0));
    let s = dm.create(Class::Script, "Race", root).unwrap();
    dm.script_mut(s).unwrap().source = "b = add_bot(\"Bolt\")\nb.kart = find(\"Bot Kart\")\nprint(b.name + \" \" + b.bot)\n".into();
    let mut game = Game::start_server(dm);
    run(&mut game, 0.2);
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    assert!(log.iter().any(|l| l == "Bolt true"), "{log:?}");
    // It goes all the way round (passing near each corner).
    let mut seen = [false; 4];
    for f in 0..(20.0 / FRAME) as usize {
        game.step(FRAME);
        let p = chassis(&game, k).position;
        if f % 60 == 0 && std::env::var("DBG").is_ok() {
            eprintln!("{p:?} speed {}", num(&game, k, "speed"));
        }
        for (i, c) in corners.iter().enumerate() {
            if ((p.x - c.0).powi(2) + (p.z - c.1).powi(2)).sqrt() < 40.0 {
                seen[i] = true;
            }
        }
    }
    assert!(seen.iter().all(|s| *s), "went round: {seen:?}");
    let bot = game.world().find_first("Bolt").unwrap();
    assert!(game.is_bot(bot));
}

#[test]
fn placing_a_kart_as_the_game_starts() {
    let mut dm = arena();
    let k = kart(&mut dm, "Kart", (0.0, 0.0));
    let root = dm.root();
    let s = dm.create(Class::Script, "Place", root).unwrap();
    dm.script_mut(s).unwrap().source = "place_kart(find(\"Kart\"), {x = 40, y = 1.2, z = -20}, 90)\n".into();
    let mut game = Game::start_server(dm);
    run(&mut game, 0.5);
    let c = chassis(&game, k);
    assert!((c.position.x - 40.0).abs() < 0.5 && (c.position.z + 20.0).abs() < 0.5, "placed: {:?}", c.position);
    assert!((c.rotation.y - 90.0).abs() < 1.0, "facing +X: {:?}", c.rotation);
    // Its wheels came too.
    let w = game.world();
    let wheel = w.get(k).unwrap().children.iter().copied().find(|c| w.get(*c).unwrap().name == "Front Wheel L").unwrap();
    let wp = w.part(wheel).unwrap().position;
    assert!((wp.x - 41.6).abs() < 0.3 && (wp.z + 18.2).abs() < 0.3, "wheel with it: {wp:?}");
}
