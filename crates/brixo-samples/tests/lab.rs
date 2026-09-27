//! Plays the Test Lab headless: everything in it works, with no script errors.

use brixo_core::{DataModel, InstanceId, Lighting, Vec3};
use brixo_runtime::Game;

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

#[track_caller]
fn check(game: &Game) {
    let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).collect();
    assert!(e.is_empty(), "{e:?}");
}

/// Something called `name` somewhere inside `under`.
fn named(w: &DataModel, under: InstanceId, name: &str) -> InstanceId {
    let mut stack = vec![under];
    while let Some(id) = stack.pop() {
        let inst = w.get(id).unwrap();
        if inst.name == name && id != under {
            return id;
        }
        stack.extend(inst.children.iter().rev());
    }
    panic!("no {name}")
}

fn press(game: &mut Game, me: InstanceId, under: InstanceId, name: &str) {
    let b = named(&game.world(), under, name);
    assert!(game.click(me, b), "couldn't click {name}");
    run(game, 0.1);
}

fn at(game: &Game, name: &str) -> Vec3 {
    let w = game.world();
    let id = w.find_first(name).unwrap();
    w.part(id).unwrap().position
}

#[test]
fn everything_in_the_test_lab_works() {
    let mut game = Game::start_server(brixo_samples::lab::test_lab());
    let me = game.add_player("Admin");
    run(&mut game, 1.0);
    check(&game);

    // Stats, gear and the controls panel.
    assert_eq!(brixo_core::leaderboard_columns(&game.world()), ["coins", "kos", "visits"]);
    {
        let w = game.world();
        let names: Vec<String> = w.get(me).unwrap().children.iter().map(|c| w.get(*c).unwrap().name.clone()).collect();
        for n in ["Sword", "Rocket Launcher", "Paintball Gun", "Lab Toggle", "Lab Panel"] {
            assert!(names.contains(&n.to_string()), "{n} in {names:?}");
        }
    }
    let panel = named(&game.world(), me, "Lab Panel");
    assert!(!game.world().gui(panel).unwrap().visible, "folded away to start");
    press(&mut game, me, me, "Lab Toggle");
    assert!(game.world().gui(panel).unwrap().visible);

    // Lighting buttons.
    press(&mut game, me, panel, "Night");
    assert_eq!(Lighting::of(&game.world()).time_of_day, 23.0);
    press(&mut game, me, panel, "Fog");
    assert_eq!(Lighting::of(&game.world()).fog_end, 110.0);
    press(&mut game, me, panel, "Clear");
    press(&mut game, me, panel, "Brighter");
    assert_eq!(Lighting::of(&game.world()).brightness, 1.25);
    press(&mut game, me, panel, "Alien sky");
    assert!(Lighting::of(&game.world()).sky_color.is_some());
    press(&mut game, me, panel, "Blue sky");
    assert_eq!(Lighting::of(&game.world()).sky_color, None);
    press(&mut game, me, panel, "Day cycle");
    let t0 = Lighting::of(&game.world()).time_of_day;
    run(&mut game, 1.0);
    assert!(Lighting::of(&game.world()).time_of_day > t0, "the day goes round");
    press(&mut game, me, panel, "Sunny");
    press(&mut game, me, panel, "Stop music");
    check(&game);

    // Drive a car: Forward goes +Z, Left turns left (toward +X), Stop stops.
    let car = game.world().find_first("Car 1").unwrap();
    let body_id = named(&game.world(), car, "Body");
    let body = |g: &Game| g.world().part(body_id).unwrap().position;
    let yaw = |g: &Game| g.world().part(body_id).unwrap().rotation.y;
    // Its buttons show only when someone's near.
    let fwd = named(&game.world(), car, "Forward");
    assert!(!game.world().gui(fwd).unwrap().visible, "hidden from afar");
    let near = body(&game);
    game.world().player_mut(me).unwrap().body.position = Vec3::new(near.x - 6.0, 3.0, near.z);
    run(&mut game, 0.5);
    assert!(game.world().gui(fwd).unwrap().visible, "shown up close");
    let start = body(&game);
    press(&mut game, me, car, "Forward");
    run(&mut game, 2.0);
    let moved = body(&game);
    assert!(moved.z > start.z + 5.0, "drove forward: {start:?} -> {moved:?}");
    assert!((moved.x - start.x).abs() < 2.0, "straight: {start:?} -> {moved:?}");
    press(&mut game, me, car, "Left");
    let yaw0 = yaw(&game);
    run(&mut game, 1.0);
    let yaw1 = yaw(&game);
    assert!(yaw1 > yaw0 + 15.0, "turned left: {yaw0} -> {yaw1}");
    press(&mut game, me, car, "Stop");
    run(&mut game, 0.5);
    let a = body(&game);
    run(&mut game, 0.5);
    let b = body(&game);
    assert!(((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt() < 0.3, "stopped: {a:?} -> {b:?}");

    // The drawbridge goes up and back down.
    let flat = at(&game, "Drawbridge");
    let root = game.world().root();
    press(&mut game, me, root, "Raise bridge");
    run(&mut game, 2.0);
    let up = at(&game, "Drawbridge");
    assert!(up.y > flat.y + 3.0, "raised: {flat:?} -> {up:?}");
    press(&mut game, me, root, "Raise bridge"); // (the same button, now saying "Lower bridge")
    run(&mut game, 2.5);
    let down = at(&game, "Drawbridge");
    let target = {
        let w = game.world();
        let id = w.find_first("Drawbridge").unwrap();
        w.part(id).unwrap().swing_to
    };
    assert!((down.y - flat.y).abs() < 0.3, "lowered again: {flat:?} -> {down:?}, swing_to {target:?}");

    // The windmill and spinner turn; the tower explodes and rebuilds.
    let sail = {
        let w = game.world();
        let id = w.find_first("Sail Hub").unwrap();
        w.part(id).unwrap().rotation
    };
    assert!(sail.z.abs() > 5.0 || sail.x.abs() > 5.0, "the windmill turns: {sail:?}");
    let bricks = |g: &Game| {
        let w = g.world();
        w.find_first("Tower").map(|t| w.get(t).unwrap().children.iter().filter(|b| w.part(**b).is_some_and(|p| p.anchored)).count()).unwrap_or(0)
    };
    assert_eq!(bricks(&game), 32);
    press(&mut game, me, root, "Explode");
    run(&mut game, 1.0);
    assert!(bricks(&game) < 32, "blown apart");
    {
        // In pieces, not one lump: the loose bricks are spread out.
        let w = game.world();
        let tower = w.find_first("Tower").unwrap();
        let loose: Vec<Vec3> = w.get(tower).unwrap().children.iter().filter_map(|b| w.part(*b)).filter(|p| !p.anchored).map(|p| p.position).collect();
        let spread = loose.iter().map(|p| p.x).fold(f32::MIN, f32::max) - loose.iter().map(|p| p.x).fold(f32::MAX, f32::min);
        assert!(spread > 8.0, "scattered: {spread}");
    }
    press(&mut game, me, root, "Rebuild");
    assert_eq!(bricks(&game), 32, "rebuilt");
    {
        let w = game.world();
        let tower = w.find_first("Tower").unwrap();
        let brick = w.get(tower).unwrap().children[0];
        assert!(w.part(brick).unwrap().position.y > 0.0, "above ground, not down in Storage");
    }

    // Coins count.
    let coin = at(&game, "Coin");
    game.world().player_mut(me).unwrap().body.position = Vec3::new(coin.x, coin.y, coin.z);
    run(&mut game, 0.3);
    assert!(matches!(game.world().get(me).unwrap().attributes.get("coins"), Some(brixo_core::Attribute::Num(n)) if *n >= 1.0));
    check(&game);
}
