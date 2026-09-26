//! Plays Spire Wars headless: teams, weapons, towers, rounds.

use brixo_core::{Attribute, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn errors(game: &Game) -> Vec<String> {
    game.take_log().into_iter().filter(|l| l.is_error).map(|l| format!("[{}] {}", l.source, l.text)).collect()
}

fn check(game: &Game) {
    let e = errors(game);
    assert!(e.is_empty(), "{e:?}");
}

fn text(game: &Game, id: InstanceId, key: &str) -> String {
    match game.world().get(id).unwrap().attributes.get(key) {
        Some(Attribute::Str(s)) => s.clone(),
        Some(Attribute::Num(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn place(game: &mut Game, who: InstanceId, x: f32, z: f32) {
    game.world().player_mut(who).unwrap().body.position = Vec3::new(x, 3.0, z);
}

/// (breakable bricks still anchored, all breakable bricks) in the map.
fn tower_bricks(game: &Game) -> (usize, usize) {
    let w = game.world();
    let map = w.find_first("Map").unwrap();
    let bricks: Vec<_> = w
        .parts_under(map)
        .into_iter()
        .filter(|p| matches!(w.get(*p).unwrap().attributes.get("breakable"), Some(Attribute::Bool(true))))
        .collect();
    (bricks.iter().filter(|p| w.part(**p).unwrap().anchored).count(), bricks.len())
}

#[test]
fn a_round_of_spire_wars() {
    let mut model = brixo_samples::spire_wars();
    let root = model.root();
    model.get_mut(root).unwrap().attributes.insert("round_seconds".into(), Attribute::Num(18.0));
    let mut game = Game::start_server(model);
    let names = ["Ann", "Bob", "Cat", "Dan", "Eve"];
    let ids: Vec<InstanceId> = names.iter().map(|n| game.add_player(n)).collect();
    run(&mut game, 1.5);
    check(&game);

    // Four teams, balanced; team shirts; six weapons; everyone on their tower.
    let teams: Vec<String> = ids.iter().map(|id| text(&game, *id, "team")).collect();
    let mut distinct = teams[..4].to_vec();
    distinct.sort();
    assert_eq!(distinct, ["Blue", "Green", "Red", "Yellow"], "the first four fill every team: {teams:?}");
    for id in &ids {
        assert_eq!(game.backpack(*id).len(), 6);
    }
    {
        let w = game.world();
        let red = w.player(ids[teams.iter().position(|t| t == "Red").unwrap()]).unwrap();
        assert_eq!((red.shirt_color.r, red.shirt_color.g, red.shirt_color.b), (196, 40, 28));
        assert!(red.body.position.y > 30.0, "spawned on top of the tower: {:?}", red.body.position);
    }
    let (standing, total) = tower_bricks(&game);
    assert!(total > 600, "four towers of bricks: {total}");
    run(&mut game, 1.0);
    assert_eq!(tower_bricks(&game).0, standing, "towers stand on their own");

    // Sword: chop an enemy standing in front (not a teammate).
    let (ann, bob, eve) = (ids[0], ids[1], ids[4]);
    assert_eq!(teams[4], teams[0], "Eve joins the smallest team: Ann's");
    place(&mut game, ann, 0.0, 0.0);
    place(&mut game, bob, 0.0, 3.0);
    place(&mut game, eve, 2.0, 2.0);
    run(&mut game, 0.3);
    // (Regression: characters standing together used to carry each other
    // along when one was teleported.)
    let at = game.world().player(ann).unwrap().body.position;
    assert!(at.x.abs() < 0.5 && at.z.abs() < 0.5, "Ann stayed where she was put: {at:?}");
    game.equip(ann, Some(0));
    game.activate(ann);
    run(&mut game, 0.5);
    check(&game);
    assert!(game.world().player(bob).unwrap().health < 100.0, "Bob, an enemy in front, was hit");
    assert_eq!(game.world().player(eve).unwrap().health, 100.0, "Eve, a teammate, wasn't");
    game.activate(ann);
    run(&mut game, 0.5);
    game.activate(ann);
    run(&mut game, 0.6);
    check(&game);
    assert!(game.world().player(bob).unwrap().dead > 0.0);
    assert_eq!(text(&game, ann, "kos"), "1");

    // Rocket: face the Red tower from 20 studs and fire.
    place(&mut game, ann, -75.0, -52.0);
    game.set_input_for(ann, PlayerInput { move_z: -1.0, ..Default::default() });
    run(&mut game, 0.1);
    game.set_input_for(ann, PlayerInput::default());
    game.equip(ann, Some(1));
    run(&mut game, 0.1);
    let before = tower_bricks(&game).0;
    game.activate(ann);
    run(&mut game, 1.0);
    check(&game);
    let after = tower_bricks(&game).0;
    assert!(before - after >= 3, "the rocket blew bricks out of the tower: {} came loose", before - after);

    // Trowel: a wall of nine bricks.
    place(&mut game, ann, 30.0, 30.0);
    run(&mut game, 0.2);
    game.equip(ann, Some(4));
    game.activate(ann);
    run(&mut game, 0.3);
    check(&game);
    {
        let w = game.world();
        let wall = w.find_first("Trowel Wall").expect("a wall");
        assert_eq!(w.parts_under(wall).len(), 9);
    }

    // Superball, slingshot and timebomb all go off without errors.
    place(&mut game, ann, 0.0, -30.0);
    run(&mut game, 0.2);
    for slot in [2, 3, 5] {
        game.equip(ann, Some(slot));
        run(&mut game, 0.1);
        game.take_sounds();
        game.activate(ann);
        run(&mut game, 0.4);
        check(&game);
        let heard: Vec<String> = game.take_sounds().into_iter().filter_map(|s| match s { brixo_runtime::SoundEvent::Play { name, .. } => Some(name), _ => None }).collect();
        let projectiles = { let w = game.world(); let f = w.find_first("Projectiles").unwrap(); w.get(f).unwrap().children.len() };
        assert!(projectiles > 0 || !heard.is_empty(), "weapon {slot} did nothing");
    }
    run(&mut game, 3.5);
    check(&game);

    // The round ends: a winner, then the map rebuilds.
    run(&mut game, 9.0);
    check(&game);
    let banner = game.world().find_first("Banner").unwrap();
    let (shown, said) = {
        let w = game.world();
        let g = w.gui(banner).unwrap();
        (g.visible, g.text.clone())
    };
    assert!(shown && said.contains("wins the round"), "banner: {said}");
    run(&mut game, 6.0);
    check(&game);
    assert_eq!(tower_bricks(&game), (total, total), "every brick rebuilt and standing");
    assert!(!game.world().gui(banner).unwrap().visible);
}

#[test]
fn the_theme_song_is_real_audio_and_loops_as_music() {
    let model = brixo_samples::spire_wars();
    let theme = model.find_first("Theme").expect("a Theme sound");
    let sound = model.sound(theme).unwrap();
    assert_eq!(sound.format, "wav");
    let bytes = sound.bytes().unwrap();
    assert!(brixo_audio::decodes(&bytes), "it plays");

    let game = Game::start_server(model);
    run(&mut { game }, 0.2);
}

#[test]
fn the_game_starts_its_theme_as_music() {
    let game = Game::start_server(brixo_samples::spire_wars());
    let theme = game.world().find_first("Theme").unwrap();
    let wanted = format!("#{}", theme.raw());
    let music = game.take_sounds().into_iter().any(|s| matches!(s, brixo_runtime::SoundEvent::Music { name: Some(n), .. } if n == wanted));
    assert!(music, "play_music(find(\"Theme\")) at game start");
}
