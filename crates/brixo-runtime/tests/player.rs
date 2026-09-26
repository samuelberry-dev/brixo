//! The player's character, driven through a running game.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64, input: PlayerInput) {
    game.set_input(input);
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
    game.set_input(PlayerInput::default());
}

fn idle(game: &mut Game, seconds: f64) {
    run(game, seconds, PlayerInput::default());
}

fn forward() -> PlayerInput {
    PlayerInput { move_z: 1.0, ..Default::default() }
}

/// A big anchored floor (top at y = 0) with a spawn pad at the origin
/// (top at y = 1), so the player starts standing at y = 3.5.
fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    {
        let p = dm.part_mut(floor).unwrap();
        p.size = Vec3::new(200.0, 1.0, 200.0);
        p.position = Vec3::new(0.0, -0.5, 0.0);
    }
    let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
    dm
}

fn block(dm: &mut DataModel, name: &str, pos: Vec3, size: Vec3) -> InstanceId {
    let root = dm.root();
    let b = dm.create(Class::Part, name, root).unwrap();
    let p = dm.part_mut(b).unwrap();
    p.position = pos;
    p.size = size;
    b
}

fn script(dm: &mut DataModel, parent: InstanceId, source: &str) {
    let s = dm.create(Class::Script, "Script", parent).unwrap();
    dm.script_mut(s).unwrap().source = source.to_string();
}

fn pos(game: &Game) -> glam::Vec3 {
    game.player_position().expect("there should be a player")
}

fn log_texts(game: &Game) -> Vec<String> {
    game.take_log().into_iter().map(|l| l.text).collect()
}

#[test]
fn the_player_spawns_standing_on_the_spawn_location() {
    let mut game = Game::start(arena());
    idle(&mut game, 1.0);
    let p = pos(&game);
    assert!((p.y - 3.5).abs() < 0.2, "standing height {}", p.y);
    assert!(p.x.abs() < 0.01 && p.z.abs() < 0.01);
    assert!(game.player_grounded());
}

#[test]
fn without_a_spawn_location_the_player_drops_in_at_the_origin() {
    let mut dm = arena();
    let spawn = dm.find_first("Spawn").unwrap();
    dm.remove(spawn);
    let mut game = Game::start(dm);
    idle(&mut game, 2.0);
    assert!((pos(&game).y - 2.5).abs() < 0.2, "on the floor: {}", pos(&game).y);
}

#[test]
fn walking_moves_at_walk_speed() {
    let mut game = Game::start(arena());
    idle(&mut game, 0.5);
    run(&mut game, 1.0, forward());
    let z = pos(&game).z;
    assert!((z - 16.0).abs() < 1.5, "walked {z} studs in a second at speed 16");
}

/// Highest point reached after pressing jump for `hold` seconds.
fn jump_height(hold: f64) -> f32 {
    let mut game = Game::start(arena());
    idle(&mut game, 0.5);
    let start = pos(&game).y;
    let mut peak = start;
    let frames = (hold / FRAME).round() as usize;
    for i in 0..120 {
        let input = PlayerInput { jump: i < frames, ..Default::default() };
        run(&mut game, FRAME, input);
        peak = peak.max(pos(&game).y);
    }
    idle(&mut game, 1.0);
    assert!((pos(&game).y - start).abs() < 0.2, "landed back");
    peak - start
}

#[test]
fn a_tapped_jump_is_a_short_hop() {
    // v^2 / 2g = 22^2 / 70, about 6.9 studs.
    let h = jump_height(0.05);
    assert!((h - 7.0).abs() < 0.8, "tapped jump went {h}");
}

#[test]
fn holding_jump_goes_higher() {
    let tap = jump_height(0.05);
    let hold = jump_height(1.5);
    // 22^2 / (2 * 35 * 0.6), about 11.5 studs.
    assert!(hold > tap + 3.0 && (hold - 11.5).abs() < 1.0, "tap {tap}, hold {hold}");
}

/// A 6-stud-high ledge ending at z = 6, the player standing on it.
fn ledge() -> DataModel {
    let mut dm = arena();
    let spawn = dm.find_first("Spawn").unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 6.5, 0.0);
    block(&mut dm, "Ledge", Vec3::new(0.0, 3.0, 0.0), Vec3::new(12.0, 6.0, 12.0));
    dm
}

#[test]
fn you_can_still_jump_just_after_walking_off_a_ledge() {
    let mut game = Game::start(ledge());
    idle(&mut game, 0.5);
    let top = pos(&game).y;
    // Walk until we've just left the edge.
    while game.player_grounded() {
        run(&mut game, FRAME, forward());
    }
    run(&mut game, 0.05, forward()); // a moment of falling
    run(&mut game, 0.05, PlayerInput { jump: true, move_z: 1.0, ..Default::default() });
    idle(&mut game, 0.2);
    assert!(pos(&game).y > top + 2.0, "coyote jump should rise above the ledge: {}", pos(&game).y);
}

#[test]
fn a_jump_pressed_just_before_landing_still_happens() {
    let mut game = Game::start(ledge());
    idle(&mut game, 0.5);
    // Walk off the ledge (the edge is ~0.37s away) and start falling, with
    // no coyote jump. Falls are quick now, so stop walking just past it.
    run(&mut game, 0.42, forward());
    while !game.player_grounded() {
        let y = pos(&game).y;
        // Moments before landing (falling fast now, so 2.5 studs up is
        // about 0.05s away): press jump once, then let go.
        if y < 5.0 {
            run(&mut game, FRAME, PlayerInput { jump: true, ..Default::default() });
            break;
        }
        idle(&mut game, FRAME);
    }
    let mut peak: f32 = 0.0;
    for _ in 0..40 {
        idle(&mut game, FRAME);
        peak = peak.max(pos(&game).y);
    }
    assert!(peak > 6.0, "buffered jump should bounce back up, peaked at {peak}");
}

#[test]
fn walking_off_a_ledge_without_jumping_just_falls() {
    let mut game = Game::start(ledge());
    idle(&mut game, 0.5);
    run(&mut game, 1.0, forward());
    idle(&mut game, 1.0);
    assert!((pos(&game).y - 2.5).abs() < 0.2, "on the floor below: {}", pos(&game).y);
}

#[test]
fn scripts_set_the_face_colors_and_camera() {
    let mut dm = arena();
    let root = dm.root();
    let ctl = dm.create(Class::Folder, "Control", root).unwrap();
    script(&mut dm, ctl, r#"
p = find("Player")
p.face = "surprised"
p.shirt_color = {r = 200, g = 50, b = 50}
p.pants_color.g = 99
camera.mode = "first_person"
print(p.face + " " + p.shirt_color.r + " " + p.pants_color.g + " " + camera.mode)
p.face = "grumpy"
"#);
    let game = Game::start(dm);
    let log = game.take_log();
    assert_eq!(log[0].text, "surprised 200 99 first_person");
    assert!(log[1].is_error && log[1].text.contains("no face called 'grumpy'"), "{:?}", log[1]);
    let world = game.world();
    let p = world.player(world.find_first("Player").unwrap()).unwrap();
    assert_eq!(p.face, brixo_core::Face::Surprised);
    assert_eq!(p.camera_mode, brixo_core::CameraMode::FirstPerson);
    assert_eq!((p.shirt_color.r, p.shirt_color.g, p.shirt_color.b), (200, 50, 50));
}

#[test]
fn players_get_colors_from_the_palettes() {
    use brixo_runtime::{PANTS_COLORS, SHIRT_COLORS, SHOES_COLORS, SKIN_TONES};
    let game = Game::start(arena());
    let world = game.world();
    let p = world.player(world.find_first("Player").unwrap()).unwrap();
    let rgb = |c: brixo_core::Color| (c.r, c.g, c.b);
    assert!(SKIN_TONES.contains(&rgb(p.skin_color)));
    assert!(SHIRT_COLORS.contains(&rgb(p.shirt_color)));
    assert!(PANTS_COLORS.contains(&rgb(p.pants_color)));
    assert!(SHOES_COLORS.contains(&rgb(p.shoes_color)));
}

#[test]
fn walls_stop_the_player() {
    let mut dm = arena();
    block(&mut dm, "Wall", Vec3::new(0.0, 5.0, 8.0), Vec3::new(20.0, 10.0, 2.0));
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 2.0, forward());
    // Wall's near face is at z = 7; the capsule is 1 thick.
    let z = pos(&game).z;
    assert!(z < 6.1 && z > 5.5, "stopped at {z}");
}

#[test]
fn the_player_walks_up_steps_without_jumping() {
    let mut dm = arena();
    // A one-stud step starting at z = 6, next to the spawn pad.
    block(&mut dm, "Step", Vec3::new(0.0, 0.5, 12.0), Vec3::new(10.0, 1.0, 12.0));
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 0.7, forward());
    idle(&mut game, 0.5);
    let p = pos(&game);
    // On top of the step (top at y = 1), not blocked at its edge (z = 6).
    assert!(p.z > 8.0 && (p.y - 3.5).abs() < 0.3, "on the step at {p:?}");
}

#[test]
fn touching_a_coin_gives_the_script_the_player() {
    let mut dm = arena();
    let coin = block(&mut dm, "Coin", Vec3::new(0.0, 3.0, 6.0), Vec3::new(1.0, 1.0, 1.0));
    dm.part_mut(coin).unwrap().can_collide = false;
    script(&mut dm, coin, r#"
on touched(other)
    if other.class == "player" then
        print(other.name + " collected it with " + other.health + " health")
        destroy(self)
    end
end"#);
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 1.0, forward());
    assert_eq!(log_texts(&game), ["Player collected it with 100 health"]);
    assert!(game.world().get(coin).is_none());
    assert!(pos(&game).z > 12.0, "walked straight through the coin");
}

#[test]
fn lava_kills_and_the_player_respawns() {
    let mut dm = arena();
    let lava = block(&mut dm, "Lava", Vec3::new(0.0, 0.25, 8.0), Vec3::new(10.0, 0.5, 4.0));
    script(&mut dm, lava, "on touched(other)\n if other.class == \"player\" then\n  other.health = 0\n end\nend");
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    // Walk until the lava gets us, then stop.
    let mut died = false;
    for _ in 0..90 {
        run(&mut game, FRAME, forward());
        if log_texts(&game).iter().any(|t| t == "Player died") {
            died = true;
            break;
        }
    }
    assert!(died, "the lava should have killed the player");
    // Fallen apart for a few seconds, then back at the spawn.
    idle(&mut game, brixo_runtime::RESPAWN_TIME as f64 + 0.3);
    let p = pos(&game);
    assert!(p.z.abs() < 0.5 && (p.y - 3.5).abs() < 0.5, "back at spawn: {p:?}");
    let world = game.world();
    let player = world.find_first("Player").unwrap();
    assert_eq!(world.player(player).unwrap().health, 100.0);
}

#[test]
fn falling_off_the_world_respawns_the_player() {
    let mut dm = arena();
    let floor = dm.find_first("Floor").unwrap();
    dm.part_mut(floor).unwrap().size = Vec3::new(10.0, 1.0, 10.0);
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 1.5, forward()); // off the edge
    idle(&mut game, 3.0);
    let mut log = log_texts(&game);
    assert!(log.iter().any(|t| t == "Player fell off the world"), "{log:?}");
    idle(&mut game, brixo_runtime::RESPAWN_TIME as f64 + 0.3);
    log.extend(log_texts(&game));
    assert!(log.iter().any(|t| t == "Player respawned"), "{log:?}");
}

#[test]
fn scripts_can_change_walk_speed_and_teleport_the_player() {
    let mut dm = arena();
    let root = dm.root();
    let ctl = dm.create(Class::Folder, "Control", root).unwrap();
    script(&mut dm, ctl, "p = find(\"Player\")\np.walk_speed = 0\nwait(1)\np.position = {x = 20, y = 10, z = 20}");
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 0.4, forward());
    assert!(pos(&game).z.abs() < 0.05, "walk speed 0 means no walking");
    idle(&mut game, 2.0);
    let p = pos(&game);
    assert!((p.x - 20.0).abs() < 0.1 && (p.z - 20.0).abs() < 0.1 && (p.y - 2.5).abs() < 0.2, "{p:?}");
}

#[test]
fn walking_into_a_crate_pushes_it() {
    let mut dm = arena();
    let c = block(&mut dm, "Crate", Vec3::new(0.0, 1.0, 6.0), Vec3::new(2.0, 2.0, 2.0));
    dm.part_mut(c).unwrap().anchored = false;
    let mut game = Game::start(dm);
    idle(&mut game, 0.5);
    run(&mut game, 1.5, forward());
    let z = game.world().part(c).unwrap().position.z;
    assert!(z > 7.0, "crate pushed to z = {z}");
}

#[test]
fn things_resting_on_your_head_dont_stop_you_walking() {
    let mut dm = arena();
    // A loose crate dropped onto the spawn, right on the player's head.
    let crate_ = block(&mut dm, "Crate", Vec3::new(0.0, 8.0, 0.0), Vec3::new(1.5, 1.5, 1.5));
    dm.part_mut(crate_).unwrap().anchored = false;
    let mut game = Game::start(dm);
    idle(&mut game, 1.0);
    let head_z = pos(&game).z;
    run(&mut game, 0.6, forward());
    assert!(pos(&game).z - head_z > 7.0, "walked out from under the crate: {:?}", pos(&game));
}

