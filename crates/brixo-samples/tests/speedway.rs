//! Races Brickport Speedway headless: the lobby, the grid, the lights, bots
//! lapping the whole track (jump and all), items, and the results.

use brixo_core::{Attribute, DataModel, InstanceId};
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

#[test]
fn a_whole_race_with_bots() {
    let mut game = Game::start_server(brixo_samples::speedway::brickport_speedway());
    let me = game.add_player("Ann");
    run(&mut game, 1.0);
    check(&game);
    assert_eq!(phase(&game), "lobby");
    {
        let w = game.world();
        let kart = w.player(me).unwrap().kart.expect("Ann's in a kart in the lobby");
        assert!(brixo_core::is_kart(&w, kart));
        let bots = game.players().iter().filter(|p| w.get(**p).is_some_and(|i| i.attributes.get("bot") == Some(&Attribute::Bool(true))) && w.player(**p).unwrap().kart.is_some()).count();
        assert_eq!(bots, 7, "bots fill the grid");
    }
    // Drive about the pits a little.
    game.set_input_for(me, PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 1.0);
    game.set_input_for(me, PlayerInput::default());

    // The lights, and they're off.
    let mut t = 0.0;
    while phase(&game) != "race" {
        run(&mut game, 0.5);
        t += 0.5;
        assert!(t < 30.0, "the race starts: {}", phase(&game));
    }
    check(&game);
    println!("racing after {t}s");

    // Items, with E: a boost, an oil slick, a rocket at whoever's ahead.
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
    set_item(&mut game, "oil");
    game.key(me, "e");
    run(&mut game, 0.2);
    assert_eq!(effects(&game, "Oil Slick"), 1);
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
    // The HUD shows where you are.
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
        let s = label(&game, "Standings");
        if (t as i32) % 10 == 0 {
            println!("t={t}\n{s}");
        }
        last = s;
        check(&game);
    }
    println!("race over after {t}s: {}", phase(&game));
    assert_eq!(phase(&game), "results", "{last}");
    run(&mut game, 0.5);
    let results = label(&game, "Results");
    println!("{results}");
    assert!(results.contains("1st") && !results.lines().nth(1).unwrap().contains("DNF"), "a bot won: {results}");
    let finished = results.lines().filter(|l| !l.contains("DNF")).count() - 1;
    assert!(finished >= 5, "most bots finished: {results}");
    run(&mut game, 12.0);
    check(&game);
    assert_eq!(phase(&game), "lobby", "back to the lobby");
    assert!(game.world().player(me).unwrap().kart.is_some(), "back in a kart");
}
