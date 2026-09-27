//! Races Brickport Speedway headless: the lobby (getting in and out of
//! karts), the flyover and lights, bots lapping the whole track (jump and
//! all), items, the results, sitting a race out, and the Drift Park.

use brixo_core::{Attribute, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};

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

fn text(w: &DataModel, id: InstanceId, key: &str) -> Option<String> {
    match w.get(id)?.attributes.get(key) {
        Some(Attribute::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn phase(game: &Game) -> String {
    let w = game.world();
    text(&w, w.root(), "phase").unwrap_or_default()
}

fn label(game: &Game, name: &str) -> String {
    let w = game.world();
    let id = w.find_first(name).unwrap();
    w.gui(id).unwrap().text.clone()
}

/// The game, with a short lobby.
fn speedway(lobby: f64) -> Game {
    let mut dm = brixo_samples::speedway::brickport_speedway();
    let root = dm.root();
    dm.get_mut(root).unwrap().attributes.insert("lobby_seconds".into(), Attribute::Num(lobby));
    Game::start_server(dm)
}

fn stand_by(game: &Game, me: InstanceId, part: &str) {
    let w = game.world();
    let kart = w.find_first(part).unwrap();
    let c = brixo_core::kart_chassis(&w, kart).unwrap();
    let at = w.part(c).unwrap().position;
    drop(w);
    game.world().player_mut(me).unwrap().body.position = Vec3::new(at.x, at.y + 2.0, at.z + 4.0);
}

fn wait_for(game: &mut Game, what: &str, most: f64) -> f64 {
    let mut t = 0.0;
    while phase(game) != what {
        run(game, 0.5);
        t += 0.5;
        assert!(t < most, "waiting for {what}: still {}", phase(game));
    }
    t
}

#[test]
fn a_whole_race_with_bots() {
    let mut game = speedway(4.0);
    let me = game.add_player("Ann");
    run(&mut game, 1.0);
    check(&game);
    assert_eq!(phase(&game), "lobby");
    assert!(game.world().player(me).unwrap().kart.is_none(), "on foot to start");

    // F by a kart in the pits: in. F again: out.
    stand_by(&game, me, "Red Kart");
    run(&mut game, 0.3);
    assert!(game.key(me, "f"));
    run(&mut game, 0.3);
    let red = game.world().find_first("Red Kart").unwrap();
    assert_eq!(game.world().player(me).unwrap().kart, Some(red), "in the kart");
    game.key(me, "f");
    run(&mut game, 0.3);
    assert!(game.world().player(me).unwrap().kart.is_none(), "out again");
    game.key(me, "f");
    run(&mut game, 0.3);
    assert_eq!(game.world().player(me).unwrap().kart, Some(red), "and back in");
    // Drive about the pits a little.
    game.set_input_for(me, PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 1.0);
    game.set_input_for(me, PlayerInput::default());

    // The flyover: the camera's on the flyover camera, then back on the kart.
    let waited = wait_for(&mut game, "intro", 20.0);
    println!("intro after {waited}s");
    run(&mut game, 1.0);
    {
        let w = game.world();
        assert_eq!(brixo_core::camera_part(&w, me), w.find_first("Flyover Camera"), "watching the flyover");
        let bots = game.players().iter().filter(|p| w.get(**p).is_some_and(|i| i.attributes.get("bot") == Some(&Attribute::Bool(true))) && w.player(**p).unwrap().kart.is_some()).count();
        assert_eq!(bots, 7, "bots fill the grid");
    }
    let waited = wait_for(&mut game, "race", 30.0);
    println!("racing {waited}s later");
    assert!(brixo_core::camera_part(&game.world(), me).is_none(), "the camera's back behind the kart");
    check(&game);

    // Items, with E: a boost, a mine, a rocket at whoever's ahead, a shield.
    let kart = game.world().player(me).unwrap().kart.unwrap();
    let set_item = |game: &mut Game, what: &str| {
        game.world().get_mut(me).unwrap().attributes.insert("item".into(), Attribute::Str(what.into()));
    };
    let effects = |game: &Game, name: &str| {
        let w = game.world();
        let fx = w.find_first("Effects").unwrap();
        w.get(fx).unwrap().children.iter().filter(|c| w.get(**c).unwrap().name == name).count()
    };
    set_item(&mut game, "boost");
    assert!(game.key(me, "e"));
    run(&mut game, 0.2);
    assert_eq!(game.world().get(kart).unwrap().attributes.get("boosting"), Some(&Attribute::Bool(true)), "boosting");
    assert!(text(&game.world(), me, "item").is_none(), "used up");
    set_item(&mut game, "mine");
    game.key(me, "e");
    run(&mut game, 0.2);
    assert_eq!(effects(&game, "Spike Mine"), 1, "a mine");
    set_item(&mut game, "rocket");
    game.key(me, "e");
    run(&mut game, 0.2);
    assert_eq!(effects(&game, "Rocket"), 1, "a rocket flies");
    set_item(&mut game, "shield");
    game.key(me, "e");
    run(&mut game, 0.2);
    assert_eq!(effects(&game, "Shield"), 1, "a shield");
    run(&mut game, 4.5);
    assert_eq!(effects(&game, "Rocket"), 0, "the rocket's gone");
    check(&game);
    {
        let w = game.world();
        let hud = w.get(me).unwrap().children.iter().copied().find(|c| w.get(*c).unwrap().name == "Race HUD").unwrap();
        let g = w.gui(hud).unwrap();
        assert!(g.visible && g.text.contains("LAP 1/3"), "{}", g.text);
    }

    // Watch the bots go round.
    let mut t = 0.0;
    let mut last = String::new();
    while phase(&game) == "race" && t < 200.0 {
        run(&mut game, 1.0);
        t += 1.0;
        last = label(&game, "Standings");
        check(&game);
    }
    println!("race over after {t}s");
    assert_eq!(phase(&game), "results", "{last}");
    run(&mut game, 0.5);
    let results = label(&game, "Results");
    println!("{results}");
    assert!(results.contains("1st") && !results.lines().nth(1).unwrap().contains("DNF"), "a bot won: {results}");
    let finished = results.lines().filter(|l| !l.contains("DNF")).count() - 1;
    assert!(finished >= 5, "most bots finished: {results}");
    wait_for(&mut game, "lobby", 20.0);
    check(&game);
    run(&mut game, 0.5);
    assert!(game.world().player(me).unwrap().kart.is_none(), "on foot in the lobby");

    // Sitting the next one out: no race starts without anyone in it.
    assert!(game.key(me, "g"));
    run(&mut game, 10.0);
    assert_eq!(phase(&game), "lobby", "waits for a racer");
    check(&game);
}

#[test]
fn the_drift_park_is_open_during_a_race() {
    let mut game = speedway(2.0);
    let racer = game.add_player("Ann");
    let watcher = game.add_player("Bo");
    run(&mut game, 0.5);
    game.key(watcher, "g");
    wait_for(&mut game, "race", 60.0);
    check(&game);
    assert!(game.world().player(watcher).unwrap().kart.is_none());
    assert!(game.world().player(racer).unwrap().kart.is_some(), "Ann races");
    // A race kart: no. A practice kart: yes.
    stand_by(&game, watcher, "Practice Kart 1");
    run(&mut game, 0.3);
    game.key(watcher, "f");
    run(&mut game, 0.3);
    let practice = game.world().find_first("Practice Kart 1").unwrap();
    assert_eq!(game.world().player(watcher).unwrap().kart, Some(practice), "practising");
    // Driving out of the park puts you back in it.
    game.set_input_for(watcher, PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 6.0);
    let w = game.world();
    let c = w.part(brixo_core::kart_chassis(&w, practice).unwrap()).unwrap().position;
    let (x0, x1, z0, z1) = brixo_samples::speedway::DRIFT;
    assert!(c.x > x0 - 6.0 && c.x < x1 + 6.0 && c.z > z0 - 6.0 && c.z < z1 + 6.0, "still in the park: {c:?}");
    drop(w);
    check(&game);
}

#[test]
fn a_lap_without_catching_on_the_road() {
    // Driven like a bot, a whole lap: the kart never catches on the little
    // lips where road pieces meet (a sudden stop, a shove sideways, a hop).
    let mut dm = brixo_samples::speedway::brickport_speedway();
    let race = dm.find_first("Race").unwrap();
    dm.remove(race);
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    let kart = game.world().find_first("Red Kart").unwrap();
    game.world().player_mut(me).unwrap().kart = Some(kart);
    game.step(FRAME);
    {
        let mut w = game.world();
        let c = brixo_core::kart_chassis(&w, kart).unwrap();
        let p = w.part_mut(c).unwrap();
        p.position = Vec3::new(-30.0, 1.4, 0.0);
        p.rotation = Vec3::new(0.0, 90.0, 0.0);
    }
    game.step(FRAME);
    game.set_autopilot(me, true);
    let mut last: Option<(Vec3, f32)> = None;
    let mut went = 0.0;
    let mut caught = Vec::new();
    for _ in 0..(60 * 45) {
        game.step(FRAME);
        let w = game.world();
        let p = *w.part(brixo_core::kart_chassis(&w, kart).unwrap()).unwrap();
        let speed = match w.get(kart).unwrap().attributes.get("speed") {
            Some(Attribute::Num(n)) => *n as f32,
            _ => 0.0,
        };
        if let Some((l, was)) = last {
            if was - speed > 15.0 {
                caught.push((p.position, was, speed));
            }
            went += ((p.position.x - l.x).powi(2) + (p.position.z - l.z).powi(2)).sqrt();
        }
        last = Some((p.position, speed));
    }
    assert!(went > 2300.0, "a whole lap: {went}");
    // (Glancing off a wall on a tight bend is fair; catching on the road
    // all the way round isn't. It used to be a dozen times a lap.)
    assert!(caught.len() <= 2, "caught on the road: {caught:?}");
    check(&game);
}
