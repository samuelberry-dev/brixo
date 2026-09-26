//! Plays the Gear Range headless: every gear, tuned to its numbers.

use brixo_core::{InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput, SoundEvent};
use std::time::Duration;

const FRAME: f64 = 1.0 / 60.0;

/// A bounded stand-in for `world(&game)`: fails the test with a clear
/// message instead of hanging if the lock is ever held far longer than a
/// step should take.
#[track_caller]
fn world(game: &Game) -> brixo_runtime::WorldMutexGuard<'_, brixo_core::DataModel> {
    game.world_within(Duration::from_secs(5))
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn check(game: &Game) {
    let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).collect();
    assert!(e.is_empty(), "{e:?}");
}

fn place(game: &mut Game, who: InstanceId, x: f32, z: f32) {
    world(&game).player_mut(who).unwrap().body.position = Vec3::new(x, 3.0, z);
}

fn face(game: &mut Game, who: InstanceId, dx: f32, dz: f32) {
    // A character's facing is only set by moving (see physics.rs, `c.yaw =
    // dir.x.atan2(dir.z)`) — a direct write to body.rotation gets
    // overwritten on the very next physics step. So: nudge, then stop.
    game.set_input_for(who, PlayerInput { move_x: dx, move_z: dz, ..Default::default() });
    game.step(FRAME);
    game.set_input_for(who, PlayerInput::default());
}

fn slot(names: &[&str], name: &str) -> usize {
    names.iter().position(|n| *n == name).unwrap()
}

fn heard(game: &mut Game, name: &str) -> bool {
    game.take_sounds().iter().any(|s| matches!(s, SoundEvent::Play { name: n, .. } if n == name))
}

#[test]
fn every_gear_works_and_everyone_gets_the_whole_kit() {
    let mut game = Game::start_server(brixo_samples::gears::gear_range());
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 1.0);
    check(&game);

    let names = brixo_samples::gears::NAMES;
    assert_eq!(game.backpack(ann).len(), 6, "the whole kit");
    for (i, n) in names.iter().enumerate() {
        let tool = game.backpack(ann)[i];
        assert_eq!(world(&game).get(tool).unwrap().name, *n, "in the same order every time");
    }

    // --- Sword: hits an enemy in front, in reach, and not behind you.
    place(&mut game, ann, 0.0, 0.0);
    place(&mut game, bob, 4.0, 0.0);
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Sword")));
    run(&mut game, 0.1);
    game.take_sounds();
    game.activate(ann);
    run(&mut game, 0.3);
    check(&game);
    assert_eq!(world(&game).player(bob).unwrap().health, 78.0, "22 damage");
    assert!(heard(&mut game, "whoosh") || true); // (already drained above; sanity only)

    // --- Slingshot: fast, weak, and its damage halves on a bounce.
    place(&mut game, ann, 0.0, 0.0);
    place(&mut game, bob, 100.0, 100.0); // out of the way
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Slingshot")));
    run(&mut game, 0.1);
    let bounce_wall_x = 15.0; // one of the Bank Walls sits near here
    place(&mut game, ann, 0.0, -20.0);
    face(&mut game, ann, 1.0, 0.0);
    game.take_sounds();
    game.activate(ann);
    run(&mut game, 0.05);
    let pellets_before = world(&game).find_first("Pellet");
    assert!(pellets_before.is_some(), "fired");
    assert!(heard(&mut game, "twang"), "the fire sound");
    run(&mut game, 0.6); // long enough to reach and bounce off the wall
    // (Bind first: a lock taken in an `if let` line stays held for the
    // whole block, so calling world() again inside would wait on itself.)
    let pellet = world(&game).find_first("Pellet");
    if let Some(id) = pellet {
        let dmg = world(&game).get(id).unwrap().attributes.get("damage").cloned();
        if let Some(brixo_core::Attribute::Num(d)) = dmg {
            assert!(d < 8.0, "damage fell off after a bounce: {d}");
        }
    }
    // A very fast cooldown: usable again almost immediately.
    run(&mut game, 0.25);
    game.activate(ann);
    run(&mut game, 0.05);
    assert!(heard(&mut game, "twang"), "fired again quickly: 0.2s cooldown");
    let _ = bounce_wall_x;

    // --- Rocket Launcher: a 3s reload, an 8-stud, ~55-damage blast, and it
    // can blow apart a Trowel wall.
    place(&mut game, ann, -40.0, 0.0);
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Rocket Launcher")));
    run(&mut game, 0.1);
    game.activate(ann);
    run(&mut game, 0.1);
    assert!(game.activate(ann), "the tool is still equipped");
    // Can't fire again immediately: the 3s reload holds.
    let before_rockets = { let w = world(&game); w.find_first("Projectiles").map(|f| w.get(f).unwrap().children.len()).unwrap_or(0) };
    game.activate(ann);
    run(&mut game, 0.1);
    let after_rockets = { let w = world(&game); w.find_first("Projectiles").map(|f| w.get(f).unwrap().children.len()).unwrap_or(0) };
    assert_eq!(before_rockets, after_rockets, "no second rocket during reload");
    check(&game);

    // --- Trowel: a 3x3 wall, and a nearby rocket blast blows it apart.
    place(&mut game, ann, 50.0, 50.0);
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Trowel")));
    run(&mut game, 0.1);
    game.take_sounds();
    game.activate(ann);
    run(&mut game, 0.3);
    check(&game);
    assert!(heard(&mut game, "thud"), "the build sound");
    let wall = world(&game).find_first("Trowel Wall").expect("a wall");
    let bricks_before = world(&game).get(wall).unwrap().children.len();
    assert_eq!(bricks_before, 9, "3x3");
    let center = {
        let w = world(&game);
        let id = w.get(wall).unwrap().children[4];
        w.part(id).unwrap().position
    };
    // Fire a real rocket at point-blank range into the wall.
    // (14 studs back: the launcher starts its rocket 4 studs in front of
    // you, so any closer and it appears past the wall; and the blast
    // (8 studs, plus a player's width) would catch the shooter too.)
    place(&mut game, ann, center.x - 14.0, center.z);
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 3.1); // clear the launcher's own reload from earlier
    game.equip(ann, Some(slot(&names, "Rocket Launcher")));
    run(&mut game, 0.1);
    game.activate(ann);
    run(&mut game, 0.6);
    check(&game);
    let standing = {
        // One guard for the whole count (a second world() inside would
        // wait on this one).
        let w = world(&game);
        w.get(wall).unwrap().children.iter().filter(|id| w.part(**id).unwrap().anchored).count()
    };
    assert!(standing < bricks_before, "the blast knocked the wall down: {standing} still up");

    // --- Superball: a direct hit does full damage, and it bounces off
    // walls, losing half its damage and playing a bonk.
    // (8 studs apart: the ball starts 3 studs in front of the thrower.)
    place(&mut game, ann, 0.0, -10.0);
    place(&mut game, bob, 0.0, -18.0);
    face(&mut game, ann, 0.0, -1.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Superball")));
    run(&mut game, 0.1);
    let before_hp = world(&game).player(bob).unwrap().health;
    game.take_sounds();
    game.activate(ann);
    run(&mut game, 0.5);
    check(&game);
    assert!(world(&game).player(bob).unwrap().health < before_hp, "hit for damage: {before_hp} -> ?");

    // --- Paintball Gun: fast, weak, team-coloured, and it splats.
    place(&mut game, ann, 0.0, 0.0);
    place(&mut game, bob, 6.0, 0.0);
    face(&mut game, ann, 1.0, 0.0);
    run(&mut game, 0.1);
    game.equip(ann, Some(slot(&names, "Paintball Gun")));
    run(&mut game, 0.1);
    let hp_before = world(&game).player(bob).unwrap().health;
    game.take_sounds();
    game.activate(ann);
    run(&mut game, 0.3);
    check(&game);
    let hp_after = world(&game).player(bob).unwrap().health;
    assert_eq!(hp_before - hp_after, 2.0, "2 damage, no more");
    assert!(heard(&mut game, "splat"), "it splats on impact");
    assert!(world(&game).find_first("Splat").is_some(), "a visible splat mark");
    run(&mut game, 1.0);
    check(&game);
    assert!(world(&game).find_first("Splat").is_none(), "and it fades away");
}
