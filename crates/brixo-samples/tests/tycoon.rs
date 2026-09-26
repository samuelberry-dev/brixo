//! Plays Coin Tycoon headless: claim a plot, earn, buy, finish.

use brixo_core::Vec3;
use brixo_runtime::{Cue, Game};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn errors(game: &Game) -> Vec<String> {
    game.take_log().into_iter().filter(|l| l.is_error).map(|l| format!("[{}] {}", l.source, l.text)).collect()
}

/// Walks (teleports) the player onto a part and waits a moment.
fn step_on(game: &mut Game, player: brixo_core::InstanceId, part: &str, plot: &str) {
    let at = {
        let w = game.world();
        let plot = w.find_first(plot).unwrap();
        let id = w.walk().into_iter().find(|id| w.get(*id).unwrap().name == part && w.get(*id).unwrap().parent.is_some_and(|p| p == plot || w.get(p).unwrap().parent == Some(plot))).unwrap();
        w.part(id).unwrap().position
    };
    game.world().player_mut(player).unwrap().body.position = Vec3::new(at.x, at.y + 3.0, at.z);
    run(game, 0.6);
}

fn cash(game: &Game, player: brixo_core::InstanceId) -> f64 {
    match game.world().get(player).unwrap().attributes.get("cash") {
        Some(brixo_core::Attribute::Num(n)) => *n,
        other => panic!("no cash: {other:?}"),
    }
}

fn bought(game: &Game, item: &str) -> bool {
    let w = game.world();
    let plot = w.find_first("PlotA").unwrap();
    let id = w.get(plot).unwrap().children.iter().copied().find(|c| w.get(*c).unwrap().name == item).unwrap();
    matches!(w.get(id).unwrap().attributes.get("bought"), Some(brixo_core::Attribute::Bool(true)))
}

#[test]
fn a_whole_game_of_coin_tycoon() {
    let mut game = Game::start_server(brixo_samples::coin_tycoon());
    let e = errors(&game);
    assert!(e.is_empty(), "{e:?}");
    let ann = game.add_player("Ann");
    run(&mut game, 1.0);
    let cues: Vec<Cue> = game.take_sounds().into_iter().filter_map(|e| e.for_player(Some(ann))).collect();
    assert!(cues.contains(&Cue::Music(Some("sunny".into()))), "music plays: {cues:?}");
    assert_eq!(cash(&game, ann), 0.0);

    // Claim plot A: the sign shows Ann's name and a fanfare plays.
    step_on(&mut game, ann, "ClaimPad", "PlotA");
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }
    let owner = {
        let w = game.world();
        w.get(w.find_first("PlotA").unwrap()).unwrap().attributes.get("owner").cloned()
    };
    assert_eq!(owner, Some(brixo_core::Attribute::Str("Ann".into())));
    let cues: Vec<Cue> = game.take_sounds().into_iter().filter_map(|e| e.for_player(Some(ann))).collect();
    assert!(cues.contains(&Cue::Sound("win".into())));

    // Walk off the pad (back to the plaza) and let the dropper earn.
    game.world().player_mut(ann).unwrap().body.position = Vec3::new(0.0, 4.0, 0.0);
    run(&mut game, 12.0);
    let earned = cash(&game, ann);
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }
    assert!(earned >= 15.0, "the dropper, belt and furnace made money: ${earned}");
    assert!(game.take_sounds().iter().any(|e| matches!(e, brixo_runtime::SoundEvent::Play { name, .. } if name == "pop" || name == "coin")));

    // Buy Dropper 2.
    run(&mut game, 3.0);
    step_on(&mut game, ann, "Pad1", "PlotA");
    run(&mut game, 1.0);
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }
    assert!(bought(&game, "Dropper2"), "dropper 2 bought");
    {
        let w = game.world();
        let d2 = w.find_first("Dropper2").unwrap();
        for p in w.parts_under(d2) {
            assert!(w.part(p).unwrap().transparency < 0.01, "faded in");
        }
    }

    // Can't afford the upgrader yet: a buzz, and nothing bought.
    game.world().get_mut(ann).unwrap().attributes.insert("cash".into(), brixo_core::Attribute::Num(10.0));
    game.world().player_mut(ann).unwrap().body.position = Vec3::new(0.0, 4.0, 0.0);
    run(&mut game, 0.3);
    game.take_sounds();
    step_on(&mut game, ann, "Pad2", "PlotA");
    assert!(!bought(&game, "Upgrader"));
    assert!(game.take_sounds().iter().any(|e| matches!(e, brixo_runtime::SoundEvent::Play { name, .. } if name == "error")));

    // Rich now: buy everything in order.
    game.world().get_mut(ann).unwrap().attributes.insert("cash".into(), brixo_core::Attribute::Num(100_000.0));
    for (pad, item) in [("Pad2", "Upgrader"), ("Pad3", "Dropper3"), ("Pad4", "Walls"), ("Pad5", "MegaDropper"), ("Pad6", "Tower")] {
        game.world().player_mut(ann).unwrap().body.position = Vec3::new(0.0, 4.0, 0.0);
        run(&mut game, 0.3);
        step_on(&mut game, ann, pad, "PlotA");
        run(&mut game, 0.8);
        assert!(bought(&game, item), "{item} bought");
    }
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }

    // With everything bought, the plot earns much faster (and some ore is
    // doubled by the upgrader).
    let before = cash(&game, ann);
    game.world().player_mut(ann).unwrap().body.position = Vec3::new(0.0, 4.0, 0.0);
    run(&mut game, 10.0);
    let rate = (cash(&game, ann) - before) / 10.0;
    assert!(rate > 20.0, "earning ${rate}/s with the full tycoon");
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }
}

#[test]
fn someone_elses_plot_is_off_limits() {
    let mut game = Game::start_server(brixo_samples::coin_tycoon());
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.5);
    step_on(&mut game, ann, "ClaimPad", "PlotA");
    game.world().player_mut(ann).unwrap().body.position = Vec3::new(0.0, 4.0, 0.0);
    // Bob can't claim it again, or buy on it.
    game.world().get_mut(bob).unwrap().attributes.insert("cash".into(), brixo_core::Attribute::Num(1000.0));
    step_on(&mut game, bob, "ClaimPad", "PlotA");
    step_on(&mut game, bob, "Pad1", "PlotA");
    assert!(!bought(&game, "Dropper2"));
    { let e = errors(&game); assert!(e.is_empty(), "{e:?}"); }
}
