//! Chat, and the character riding moving things.

use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::{filter_chat, Game, PlayerInput};

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

#[test]
fn chat_is_trimmed_filtered_capped_and_rate_limited() {
    let mut game = Game::start_server(arena());
    let ann = game.add_player("Ann");
    assert_eq!(game.chat(ann, "  hello there!  ").as_deref(), Some("hello there!"));
    assert_eq!(game.chat(ann, "too fast"), None, "spam limit");
    run(&mut game, 0.5);
    assert_eq!(game.chat(ann, "you NOOB, shut up").as_deref(), Some("you NOOB, shut up"), "gaming talk is fine");
    run(&mut game, 0.5);
    assert_eq!(game.chat(ann, "FUCK off").as_deref(), Some("#### off"));
    run(&mut game, 0.5);
    let long = "a".repeat(500);
    assert_eq!(game.chat(ann, &long).unwrap().len(), 120);
    run(&mut game, 0.5);
    assert_eq!(game.chat(ann, "   "), None, "nothing to say");
    let got: Vec<String> = game.take_chat().into_iter().map(|(_, name, t)| format!("{name}: {t}")).collect();
    assert_eq!(got.len(), 4);
    assert_eq!(got[0], "Ann: hello there!");
}

#[test]
fn the_filter_only_matches_whole_words() {
    assert_eq!(filter_chat("hello, shellfish and class"), "hello, shellfish and class");
    // Mild words get through; real swearing doesn't, in any case.
    assert_eq!(filter_chat("HELL yes, gg noob"), "HELL yes, gg noob");
    assert_eq!(filter_chat("SHIT!"), "####!");
}

#[test]
fn the_character_rides_conveyors_and_moving_platforms() {
    // A conveyor under the spawn.
    let mut dm = arena();
    let spawn = dm.find_first("Spawn").unwrap();
    let pad = dm.part_mut(spawn).unwrap();
    pad.velocity = Vec3::new(8.0, 0.0, 0.0);
    pad.size = Vec3::new(40.0, 1.0, 6.0); // long enough to ride for a while
    let mut game = Game::start(dm);
    run(&mut game, 1.5);
    assert!(game.player_position().unwrap().x > 5.0, "carried: {:?}", game.player_position());

    // A platform a script slides along.
    let mut dm = arena();
    let root = dm.root();
    let plat = dm.create(Class::Part, "Platform", root).unwrap();
    {
        let p = dm.part_mut(plat).unwrap();
        p.position = Vec3::new(0.0, 5.0, 30.0);
        p.size = Vec3::new(8.0, 1.0, 8.0);
    }
    let s = dm.create(Class::Script, "Slide", plat).unwrap();
    dm.script_mut(s).unwrap().source = "wait(1)\nevery 0.02 seconds\n self.position.x += 0.2\nend".into();
    let spawn = dm.find_first("Spawn").unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 6.0, 30.0); // spawn onto the platform
    dm.part_mut(spawn).unwrap().can_collide = false;
    dm.part_mut(spawn).unwrap().transparency = 1.0;
    let mut game = Game::start(dm);
    run(&mut game, 0.8);
    let x0 = game.player_position().unwrap().x;
    run(&mut game, 2.0);
    let x1 = game.player_position().unwrap().x;
    let plat_x = game.world().part(plat).unwrap().position.x;
    assert!(plat_x > 10.0 && (x1 - x0) > 0.8 * plat_x, "rode along: moved {} while the platform moved {plat_x}", x1 - x0);
}

#[test]
fn animation_state_follows_the_character() {
    let mut game = Game::start(arena());
    run(&mut game, 0.5);
    let me = game.player_id().unwrap();
    assert!(!game.world().player(me).unwrap().airborne);
    game.set_input(PlayerInput { move_z: 1.0, ..Default::default() });
    run(&mut game, 0.4);
    assert!((game.world().player(me).unwrap().speed - 16.0).abs() < 1.5);
    game.set_input(PlayerInput { jump: true, ..Default::default() });
    run(&mut game, 0.1);
    assert!(game.world().player(me).unwrap().airborne);
}
