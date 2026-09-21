//! Several players in one game, the way a server runs it.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
    dm
}

fn with_script(source: &str) -> DataModel {
    let mut dm = arena();
    let root = dm.root();
    let s = dm.create(Class::Script, "Rules", root).unwrap();
    dm.script_mut(s).unwrap().source = source.to_string();
    dm
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn texts(game: &Game) -> Vec<String> {
    game.take_log().into_iter().map(|l| l.text).collect()
}

fn pos(game: &Game, id: InstanceId) -> glam::Vec3 {
    game.position_of(id).expect("player has a character")
}

#[test]
fn a_server_game_starts_empty_and_players_join() {
    let mut game = Game::start_server(arena());
    assert!(game.players().is_empty());
    assert_eq!(game.player_id(), None);
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 1.0);
    assert_eq!(game.players(), [ann, bob]);
    // Both standing on the spawn pad, apart from each other.
    for id in [ann, bob] {
        assert!((pos(&game, id).y - 3.5).abs() < 0.2, "on the pad");
    }
    assert!(pos(&game, ann).distance(pos(&game, bob)) > 2.0, "not inside each other");
}

#[test]
fn scripts_hear_players_join_and_leave() {
    let mut game = Game::start_server(with_script(
        "on player_joined(p)\n print(p.name + \" joined, \" + len(players()) + \" playing\")\nend\n\
         on player_left(p)\n print(p.name + \" left\")\nend",
    ));
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    game.remove_player(ann);
    run(&mut game, 0.2);
    assert_eq!(texts(&game), ["Ann joined, 1 playing", "Bob joined, 2 playing", "Ann left"]);
    assert!(game.world().get(ann).is_none(), "Ann's character is gone");
    assert!(game.position_of(ann).is_none(), "and so is her physics capsule");
    assert_eq!(game.players(), [bob]);
}

#[test]
fn a_single_player_game_announces_its_player_too() {
    let game = Game::start(with_script("on player_joined(p)\n print(\"hi \" + p.name)\nend"));
    assert_eq!(texts(&game), ["hi Player"]);
}

#[test]
fn each_player_moves_with_their_own_input() {
    let mut game = Game::start_server(arena());
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.5);
    let (a0, b0) = (pos(&game, ann), pos(&game, bob));
    game.set_input_for(ann, PlayerInput { move_z: 1.0, ..Default::default() });
    run(&mut game, 1.0);
    assert!(pos(&game, ann).z - a0.z > 12.0, "Ann walked");
    assert!(pos(&game, bob).distance(b0) < 0.1, "Bob stayed put");
}

#[test]
fn every_player_respawns_on_their_own() {
    let mut game = Game::start_server(with_script(
        "on player_joined(p)\n if p.name == \"Bob\" then\n  p.health = 0\n end\nend",
    ));
    let _ann = game.add_player("Ann");
    let _bob = game.add_player("Bob");
    run(&mut game, 0.1);
    let log = texts(&game);
    assert!(log.contains(&"Bob died and respawned".to_string()), "{log:?}");
    assert!(!log.iter().any(|t| t.starts_with("Ann")), "{log:?}");
}

#[test]
fn server_scripts_set_each_players_camera() {
    let mut game = Game::start_server(with_script(
        "on player_joined(p)\n p.camera_mode = \"first_person\"\n print(p.camera_mode)\nend",
    ));
    let ann = game.add_player("Ann");
    assert_eq!(texts(&game), ["first_person"]);
    let world = game.world();
    assert_eq!(world.player(ann).unwrap().camera_mode, brixo_core::CameraMode::FirstPerson);
}
