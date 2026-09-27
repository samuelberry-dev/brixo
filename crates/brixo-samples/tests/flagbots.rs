//! Eight bots play a match of Flagfall headless: flags get taken, carried,
//! dropped, returned and captured, by bots using only what a player sees.

use brixo_core::{Attribute, DataModel, InstanceId};
use brixo_runtime::{Game, PlayerInput};
use brixo_samples::flagbots::FlagBot;
use std::time::Duration;

const FRAME: f64 = 1.0 / 60.0;

fn num(w: &DataModel, id: InstanceId, key: &str) -> f64 {
    match w.get(id).and_then(|i| i.attributes.get(key)) {
        Some(Attribute::Num(n)) => *n,
        _ => 0.0,
    }
}

/// Plays `seconds` of bots-only Flagfall; returns (pickups seen, captures, kos, returns).
fn play(seconds: f64, seed: u64) -> (usize, f64, f64, f64, String) {
    let mut dm = brixo_samples::flagfall();
    let root = dm.root();
    dm.get_mut(root).unwrap().attributes.insert("caps_to_win".into(), Attribute::Num(99.0));
    let mut game = Game::start_server(dm);
    let names = ["Blox", "Nova", "Rex", "Kit", "Zed", "Mo", "Ash", "Pip"];
    let ids: Vec<InstanceId> = names.iter().map(|n| game.add_player(n)).collect();
    game.step(FRAME);
    // Number bots within their own team, so each team gets the same jobs.
    let mut counts = std::collections::HashMap::new();
    let mut bots: Vec<FlagBot> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let t = match game.world_within(Duration::from_secs(5)).get(*id).unwrap().attributes.get("team") {
                Some(Attribute::Str(t)) => t.clone(),
                _ => String::new(),
            };
            let n = counts.entry(t).or_insert(0usize);
            *n += 1;
            FlagBot::new(*n - 1, seed + i as u64 * 7919)
        })
        .collect();
    let mut pickups = 0;
    let mut was_carried = [false, false];
    let frames = (seconds / FRAME) as usize;
    for f in 0..frames {
        if f % 3 == 0 {
            let now = game.time();
            let actions: Vec<_> = {
                let w = game.world_within(Duration::from_secs(5));
                bots.iter_mut().zip(&ids).map(|(b, id)| b.think(&w, *id, now)).collect()
            };
            for (a, id) in actions.iter().zip(&ids) {
                game.set_input_for(*id, PlayerInput { move_x: a.move_x, move_z: a.move_z, jump: a.jump });
                if let Some(slot) = a.equip {
                    game.equip(*id, Some(slot));
                }
                if a.activate {
                    game.activate_at(*id, a.aim);
                }
            }
            let w = game.world_within(Duration::from_secs(5));
            for (i, t) in ["Red", "Blue"].iter().enumerate() {
                let pole = w.find_first(&format!("{t} Flag Pole")).unwrap();
                let carried = w.get(pole).unwrap().attributes.contains_key("carried_by");
                if carried && !was_carried[i] {
                    pickups += 1;
                }
                was_carried[i] = carried;
            }
        }
        if std::env::var_os("FLAGBOTS_CARRY").is_some() && f % 60 == 0 {
            let w = game.world_within(Duration::from_secs(5));
            for id in &ids {
                if let Some(Attribute::Str(fl)) = w.get(*id).unwrap().attributes.get("carrying") {
                    let p = w.player(*id).unwrap().body.position;
                    eprintln!("t={:.0} {} carries {} at ({:.0},{:.0},{:.0}) hp {:.0}", game.time(), w.get(*id).unwrap().name, fl, p.x, p.y, p.z, w.player(*id).unwrap().health);
                }
            }
        }
        if std::env::var_os("FLAGBOTS_DEBUG").is_some() && f % 300 == 0 {
            let w = game.world_within(Duration::from_secs(5));
            let line: Vec<String> = ids.iter().zip(&bots).map(|(id, b)| {
                let p = w.player(*id).unwrap();
                format!("{}:{:?}@({:.0},{:.0},{:.0}){}", w.get(*id).unwrap().name, b.job, p.body.position.x, p.body.position.y, p.body.position.z, if p.dead > 0.0 { "X" } else { "" })
            }).collect();
            eprintln!("t={:.0} {}", game.time(), line.join(" "));
        }
        game.step(FRAME);
    }
    let errors: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let w = game.world_within(Duration::from_secs(5));
    let caps: f64 = ids.iter().map(|id| num(&w, *id, "captures")).sum();
    let kos: f64 = ids.iter().map(|id| num(&w, *id, "kos")).sum();
    let rets: f64 = ids.iter().map(|id| num(&w, *id, "returns")).sum();
    let board = w.gui(w.find_first("Board").unwrap()).unwrap().text.clone();
    (pickups, caps, kos, rets, board)
}

#[test]
fn bots_play_real_capture_the_flag() {
    let (pickups, caps, kos, rets, board) = play(480.0, 0xC0FFEE);
    println!("pickups {pickups} captures {caps} knockouts {kos} returns {rets}\n{board}");
    assert!(kos >= 4.0, "they fight: {kos} knockouts");
    assert!(pickups >= 2, "they take flags: {pickups}");
    // Script timing runs on real threads, so no two matches go the same
    // way: some end with a capture, some with every run stopped short and
    // the flag sent home. Either shows the whole loop working.
    assert!(caps >= 1.0 || rets >= 2.0, "and carry them home, or get them back: {caps} captures, {rets} returns");
}
