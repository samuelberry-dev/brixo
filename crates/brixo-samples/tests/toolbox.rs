//! The toolbox's built-in items: each pastes into a game and works there.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

#[track_caller]
fn check(game: &Game) {
    let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(e.is_empty(), "{e:?}");
}

/// A floor, a spawn, and the item pasted in (moved `by`).
fn world_with(slug: &str, by: Vec3) -> (DataModel, Vec<InstanceId>) {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(300.0, 1.0, 300.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, -30.0);
    let item = brixo_samples::toolbox::items().into_iter().find(|i| i.slug == slug).unwrap();
    let pasted = dm.paste_clipboard(&item.content, root).expect("pastes");
    for top in &pasted {
        for id in dm.parts_under(*top).into_iter().chain(dm.part(*top).map(|_| *top)) {
            let q = dm.part_mut(id).unwrap();
            q.position = Vec3::new(q.position.x + by.x, q.position.y + by.y, q.position.z + by.z);
        }
    }
    (dm, pasted)
}

#[test]
fn every_item_pastes_and_runs() {
    let items = brixo_samples::toolbox::items();
    assert!(items.len() >= 12);
    for item in &items {
        assert!(brixo_samples::toolbox::CATEGORIES.contains(&item.category), "{}", item.name);
        let (dm, pasted) = world_with(item.slug, Vec3::new(20.0, 0.0, 20.0));
        assert!(!pasted.is_empty(), "{}", item.name);
        let mut game = Game::start(dm);
        run(&mut game, 2.0);
        let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
        assert!(e.is_empty(), "{}: {e:?}", item.name);
        // Two copies work side by side.
        let (mut dm, _) = world_with(item.slug, Vec3::new(20.0, 0.0, 20.0));
        let root = dm.root();
        dm.paste_clipboard(&item.content, root).unwrap();
        let mut game = Game::start(dm);
        run(&mut game, 1.0);
        let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
        assert!(e.is_empty(), "{} twice: {e:?}", item.name);
    }
}

#[test]
fn the_kart_is_driven_with_f() {
    let (dm, pasted) = world_with("kart", Vec3::new(0.0, 0.0, 0.0));
    let kart = pasted[0];
    let mut game = Game::start(dm);
    let me = game.player_id().unwrap();
    run(&mut game, 0.5);
    game.world().player_mut(me).unwrap().body.position = Vec3::new(4.0, 3.0, 0.0);
    run(&mut game, 0.5);
    let heard = game.key(me, "f");
    run(&mut game, 0.3);
    assert!(heard);
    assert_eq!(game.world().player(me).unwrap().kart, Some(kart), "in");
    game.set_input(PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 1.5);
    game.set_input(PlayerInput::default());
    let w = game.world();
    let c = w.part(brixo_core::kart_chassis(&w, kart).unwrap()).unwrap().position;
    assert!(c.z > 5.0, "drove: {c:?}");
    drop(w);
    game.key(me, "f");
    run(&mut game, 0.3);
    assert!(game.world().player(me).unwrap().kart.is_none(), "out");
    check(&game);
}

#[test]
fn the_car_is_driven_from_its_seat() {
    let (dm, pasted) = world_with("car", Vec3::new(0.0, 0.0, 0.0));
    let seat = game_part(&dm, pasted[0], "Seat");
    let body = game_part(&dm, pasted[0], "Body");
    let mut game = Game::start(dm);
    let me = game.player_id().unwrap();
    run(&mut game, 0.5);
    // Drop onto the seat.
    let s = game.world().part(seat).unwrap().position;
    game.world().player_mut(me).unwrap().body.position = Vec3::new(s.x, s.y + 4.0, s.z);
    run(&mut game, 1.0);
    assert_eq!(game.world().player(me).unwrap().seat, Some(seat), "sat down");
    let start = game.world().part(body).unwrap().position;
    game.set_input(PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 2.0);
    game.set_input(PlayerInput::default());
    let end = game.world().part(body).unwrap().position;
    assert!(end.z > start.z + 6.0, "drove forwards: {start:?} -> {end:?}");
    game.set_input(PlayerInput { move_x: 0.0, move_z: 0.0, jump: true });
    run(&mut game, 0.3);
    game.set_input(PlayerInput::default());
    run(&mut game, 0.3);
    assert!(game.world().player(me).unwrap().seat.is_none(), "got out");
    check(&game);
}

#[test]
fn the_door_opens_when_walked_into() {
    let (dm, pasted) = world_with("swinging-door", Vec3::new(0.0, 0.0, 0.0));
    let door = game_part(&dm, pasted[0], "Door");
    let mut game = Game::start(dm);
    let me = game.player_id().unwrap();
    run(&mut game, 0.5);
    game.world().player_mut(me).unwrap().body.position = Vec3::new(0.0, 3.0, -4.0);
    game.set_input(PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 1.5);
    game.set_input(PlayerInput::default());
    let angle = game.world().part(door).unwrap().rotation.y;
    assert!(angle.abs() > 30.0, "swung open: {angle}");
    check(&game);
}

fn game_part(dm: &DataModel, top: InstanceId, name: &str) -> InstanceId {
    dm.parts_under(top).into_iter().find(|p| dm.get(*p).unwrap().name == name).unwrap()
}

#[test]
fn the_gear_kit_arms_everyone() {
    let (dm, _) = world_with("gear-kit", Vec3::ZERO);
    let mut game = Game::start(dm);
    let me = game.player_id().unwrap();
    run(&mut game, 0.5);
    let tools = brixo_runtime::backpack(&game.world(), me);
    let names: Vec<String> = tools.iter().map(|t| game.world().get(*t).unwrap().name.clone()).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(names.contains(&"Sword".to_string()) && names.contains(&"Rocket Launcher".to_string()), "{names:?}");
    // After being knocked out: still one of each.
    game.world().player_mut(me).unwrap().health = 0.0;
    run(&mut game, 8.0);
    assert!(game.world().player(me).unwrap().health > 0.0, "back");
    assert_eq!(brixo_runtime::backpack(&game.world(), me).len(), 6, "one of each, after respawning");
    // A rocket flies and blows up without complaint.
    let slot = names.iter().position(|n| n == "Rocket Launcher").unwrap();
    game.equip(me, Some(slot));
    run(&mut game, 0.2);
    game.activate(me);
    run(&mut game, 1.0);
    check(&game);
}
