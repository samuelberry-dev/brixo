//! Dying, respawning, and explosions.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, SoundEvent};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
    dm
}

fn script(dm: &mut DataModel, parent: InstanceId, src: &str) {
    let s = dm.create(Class::Script, "S", parent).unwrap();
    dm.script_mut(s).unwrap().source = src.to_string();
}

fn log(game: &Game) -> Vec<String> {
    game.take_log().into_iter().map(|l| format!("{}{}", if l.is_error { "ERROR " } else { "" }, l.text)).collect()
}

#[test]
fn dying_falls_apart_tells_scripts_and_respawns_later() {
    let mut dm = arena();
    let root = dm.root();
    script(&mut dm, root, "on died(p)\n print(p.name + \" died\")\nend");
    let mut game = Game::start(dm);
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    game.take_log();
    game.world().player_mut(me).unwrap().health = 0.0;
    run(&mut game, 0.1);
    let p = *game.world().player(me).unwrap();
    assert!(p.dead > 0.0, "fallen apart");
    assert!(log(&game).iter().any(|l| l == "Player died"), "scripts heard it");
    assert!(game.take_sounds().iter().any(|e| matches!(e, SoundEvent::Play { name, .. } if name == "death")));

    // While dead, input does nothing.
    let at = game.player_position().unwrap();
    game.set_input(brixo_runtime::PlayerInput { move_z: 1.0, ..Default::default() });
    run(&mut game, 1.0);
    assert!((game.player_position().unwrap() - at).length() < 0.5, "the dead don't walk");

    // A few seconds in, back at the spawn with full health.
    run(&mut game, 3.2);
    let p = *game.world().player(me).unwrap();
    assert_eq!((p.dead, p.health), (0.0, p.max_health), "respawned");
}

#[test]
fn explosions_knock_out_players_throw_loose_parts_and_leave_anchored_ones() {
    let mut dm = arena();
    let root = dm.root();
    let loose = dm.create(Class::Part, "Loose", root).unwrap();
    dm.part_mut(loose).unwrap().position = Vec3::new(20.0, 1.0, 0.0);
    dm.part_mut(loose).unwrap().anchored = false;
    let fixed = dm.create(Class::Part, "Fixed", root).unwrap();
    dm.part_mut(fixed).unwrap().position = Vec3::new(18.0, 1.0, 2.0);
    // Wait until everything settles, then blow up just beside the loose
    // part and far from the player (who's at the spawn).
    script(&mut dm, root, "wait(1)\nhit = explode({x = 17, y = 1, z = 0}, 8, 90)\nprint(\"caught \" + len(hit))");
    let mut game = Game::start(dm);
    run(&mut game, 0.9);
    let before = game.world().part(loose).unwrap().position;
    run(&mut game, 0.3);
    assert!(log(&game).contains(&"caught 0".to_string()), "the player was out of range");
    let after = *game.world().part(loose).unwrap();
    assert!(after.position.x - before.x > 1.0 || after.velocity.x > 10.0, "thrown away from the blast: {after:?}");
    assert_eq!(game.world().part(fixed).unwrap().position, Vec3::new(18.0, 1.0, 2.0), "anchored parts stay");
    assert!(game.world().find_first("Explosion").is_some(), "a fireball");
    run(&mut game, 0.6);
    assert!(game.world().find_first("Explosion").is_none(), "that fades away");
}

#[test]
fn explosions_return_the_players_they_catch() {
    let mut dm = arena();
    let root = dm.root();
    script(&mut dm, root, "wait(0.5)\nhit = explode({x = 0, y = 3, z = 0}, 6)\nfor p in hit do\n print(\"caught \" + p.name)\nend");
    let mut game = Game::start(dm);
    run(&mut game, 0.8);
    assert!(log(&game).contains(&"caught Player".to_string()));
    let me = game.player_id().unwrap();
    assert!(game.world().player(me).unwrap().dead > 0.0);
}

fn brick(dm: &mut DataModel, pos: Vec3) -> InstanceId {
    let root = dm.root();
    let b = dm.create(Class::Part, "Brick", root).unwrap();
    let p = dm.part_mut(b).unwrap();
    p.position = pos;
    p.size = Vec3::new(4.0, 2.0, 2.0);
    dm.get_mut(b).unwrap().attributes.insert("breakable".into(), brixo_core::Attribute::Bool(true));
    b
}

#[test]
fn blowing_out_a_towers_base_brings_the_rest_down() {
    let mut dm = arena();
    // A tower: a column of 8 bricks at x = 30, and a wall of 3 at x = 60.
    let tower: Vec<_> = (0..8).map(|i| brick(&mut dm, Vec3::new(30.0, 1.0 + i as f32 * 2.0, 0.0))).collect();
    let wall: Vec<_> = (0..3).map(|i| brick(&mut dm, Vec3::new(60.0, 1.0 + i as f32 * 2.0, 0.0))).collect();
    let root = dm.root();
    script(&mut dm, root, "wait(0.3)\nexplode({x = 30, y = 1, z = 0}, 3.5, 60)");
    let mut game = Game::start(dm);
    run(&mut game, 0.2);
    assert!(tower.iter().all(|b| game.world().part(*b).unwrap().anchored), "standing before");
    run(&mut game, 0.3);
    {
        let w = game.world();
        assert!(!w.part(tower[0]).unwrap().anchored, "the base was blown out");
        assert!(tower.iter().all(|b| !w.part(*b).unwrap().anchored), "so everything above it comes down");
        assert!(wall.iter().all(|b| w.part(*b).unwrap().anchored), "a wall still on the ground stays up");
    }
    // With its base gone, the column drops onto the ground where it stood.
    run(&mut game, 1.5);
    let top = game.world().part(tower[7]).unwrap().position;
    assert!(top.y < 14.0, "the top of the tower came down: {top:?}");
}

#[test]
fn floating_parts_fly_straight_and_bouncy_ones_bounce() {
    let mut dm = arena();
    let root = dm.root();
    let rocket = dm.create(Class::Part, "Rocket", root).unwrap();
    {
        let p = dm.part_mut(rocket).unwrap();
        p.position = Vec3::new(0.0, 10.0, 20.0);
        p.anchored = false;
        p.floating = true;
        p.velocity = Vec3::new(30.0, 0.0, 0.0);
    }
    let ball = dm.create(Class::Part, "Ball", root).unwrap();
    {
        let p = dm.part_mut(ball).unwrap();
        p.position = Vec3::new(0.0, 12.0, -20.0);
        p.shape = brixo_core::Shape::Ball;
        p.anchored = false;
        p.bounce = 0.9;
    }
    let mut game = Game::start(dm);
    let mut highest_after_bounce: f32 = 0.0;
    let mut hit_floor = false;
    for _ in 0..90 {
        game.step(FRAME);
        let y = game.world().part(ball).unwrap().position.y;
        if y < 1.0 {
            hit_floor = true;
        } else if hit_floor {
            highest_after_bounce = highest_after_bounce.max(y);
        }
    }
    let r = game.world().part(rocket).unwrap().position;
    assert!((r.y - 10.0).abs() < 0.1 && r.x > 30.0, "flew straight: {r:?}");
    assert!(hit_floor && highest_after_bounce > 5.0, "bounced back up to {highest_after_bounce}");
}

#[test]
fn players_respawn_at_their_teams_spawn() {
    let mut dm = arena();
    let root = dm.root();
    let red = dm.create(Class::SpawnLocation, "Red Spawn", root).unwrap();
    dm.part_mut(red).unwrap().position = Vec3::new(50.0, 20.0, 50.0);
    dm.get_mut(red).unwrap().attributes.insert("team".into(), brixo_core::Attribute::Str("Red".into()));
    let mut game = Game::start(dm);
    let me = game.player_id().unwrap();
    run(&mut game, 0.3);
    game.world().get_mut(me).unwrap().attributes.insert("team".into(), brixo_core::Attribute::Str("Red".into()));
    game.world().player_mut(me).unwrap().health = 0.0;
    run(&mut game, brixo_runtime::RESPAWN_TIME as f64 + 0.3);
    let at = game.world().player(me).unwrap().body.position;
    assert!((at.x - 50.0).abs() < 3.0 && (at.z - 50.0).abs() < 3.0 && at.y > 20.0, "on the Red pad: {at:?}");
}

#[test]
fn collapses_only_touch_the_structure_that_was_hit() {
    let mut dm = arena();
    // A floating group of bricks far away (like a hidden rebuild template):
    // not on the ground, but nothing to do with the blast.
    let far: Vec<_> = (0..3).map(|i| brick(&mut dm, Vec3::new(-60.0, -200.0 + i as f32 * 2.0, 0.0))).collect();
    let near = brick(&mut dm, Vec3::new(30.0, 1.0, 0.0));
    let root = dm.root();
    script(&mut dm, root, "wait(0.2)\nexplode({x = 30, y = 1, z = 0}, 3, 50)");
    let mut game = Game::start(dm);
    run(&mut game, 0.4);
    let w = game.world();
    assert!(!w.part(near).unwrap().anchored);
    assert!(far.iter().all(|b| w.part(*b).unwrap().anchored), "the far-away group was left alone");
}
