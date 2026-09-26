//! Plays Flagfall headless: every rule of capture the flag, and the map's
//! routes are walkable.

use brixo_core::{Attribute, DataModel, InstanceId, Vec3};
use brixo_runtime::{Game, PlayerInput};
use std::time::Duration;

const FRAME: f64 = 1.0 / 60.0;

/// A bounded stand-in for `game.world()`: fails with a clear message
/// instead of hanging if the lock is ever held far longer than a step.
#[track_caller]
fn world(game: &Game) -> brixo_runtime::WorldMutexGuard<'_, DataModel> {
    game.world_within(Duration::from_secs(5))
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn check(game: &Game) {
    let e: Vec<_> = game.take_log().into_iter().filter(|l| l.is_error).collect();
    assert!(e.is_empty(), "{e:?}");
}

fn start(caps: f64, seconds: f64) -> Game {
    let mut dm = brixo_samples::flagfall();
    let root = dm.root();
    dm.get_mut(root).unwrap().attributes.insert("caps_to_win".into(), Attribute::Num(caps));
    dm.get_mut(root).unwrap().attributes.insert("match_seconds".into(), Attribute::Num(seconds));
    Game::start_server(dm)
}

fn text(game: &Game, name: &str) -> String {
    let w = world(game);
    let id = w.find_first(name).unwrap_or_else(|| panic!("no {name}"));
    w.gui(id).unwrap().text.clone()
}

fn visible(game: &Game, name: &str) -> bool {
    let w = world(game);
    let id = w.find_first(name).unwrap();
    w.gui(id).unwrap().visible
}

fn attr(game: &Game, id: InstanceId, key: &str) -> Option<Attribute> {
    world(game).get(id).unwrap().attributes.get(key).cloned()
}

fn team(game: &Game, id: InstanceId) -> String {
    match attr(game, id, "team") {
        Some(Attribute::Str(t)) => t,
        other => panic!("no team: {other:?}"),
    }
}

fn pos(game: &Game, id: InstanceId) -> Vec3 {
    let w = world(game);
    match w.player(id) {
        Some(p) => p.body.position,
        None => w.part(id).unwrap().position,
    }
}

fn place(game: &mut Game, who: InstanceId, x: f32, y: f32, z: f32) {
    world(game).player_mut(who).unwrap().body.position = Vec3::new(x, y, z);
}

fn part(game: &Game, name: &str) -> InstanceId {
    world(game).find_first(name).unwrap_or_else(|| panic!("no {name}"))
}

fn stand_top(game: &Game, t: &str) -> Vec3 {
    let w = world(game);
    let p = w.part(w.find_first(&format!("{t} Flag Stand")).unwrap()).unwrap();
    Vec3::new(p.position.x, p.position.y + p.size.y / 2.0, p.position.z)
}

/// Stands `who` right on top of `t`'s flag stand.
fn to_stand(game: &mut Game, who: InstanceId, t: &str) {
    let s = stand_top(game, t);
    place(game, who, s.x + 1.0, s.y + 2.7, s.z + 1.0);
}

fn kill(game: &mut Game, who: InstanceId) {
    world(game).player_mut(who).unwrap().health = 0.0;
}

fn score(game: &Game) -> (String, String) {
    (text(game, "Red Score"), text(game, "Blue Score"))
}

/// Four players, two a side. Returns (red1, red2, blue1, blue2).
fn teams(game: &mut Game) -> (InstanceId, InstanceId, InstanceId, InstanceId) {
    let ids: Vec<InstanceId> = ["Ann", "Bob", "Cat", "Dan"].iter().map(|n| game.add_player(n)).collect();
    run(game, 1.0);
    check(game);
    let reds: Vec<InstanceId> = ids.iter().copied().filter(|id| team(game, *id) == "Red").collect();
    let blues: Vec<InstanceId> = ids.iter().copied().filter(|id| team(game, *id) == "Blue").collect();
    assert_eq!((reds.len(), blues.len()), (2, 2), "even teams");
    (reds[0], reds[1], blues[0], blues[1])
}

#[test]
fn players_join_even_teams_with_gear_at_their_own_castle() {
    let mut game = start(3.0, 480.0);
    let (r1, _, b1, _) = teams(&mut game);
    assert_eq!(game.backpack(r1).len(), 6, "the whole gear kit");
    run(&mut game, 0.5);
    // Spawned in their own castle's spawn house (Red west, Blue east).
    assert!(pos(&game, r1).x < -120.0, "red spawns west: {:?}", pos(&game, r1));
    assert!(pos(&game, b1).x > 120.0, "blue spawns east: {:?}", pos(&game, b1));
    // Standing on the spawn pad, not fallen through the world.
    assert!(pos(&game, r1).y > 0.5 && pos(&game, r1).y < 5.0, "{:?}", pos(&game, r1));
    // Team shirts.
    let w = world(&game);
    assert_eq!(w.player(r1).unwrap().shirt_color.r, 196);
    assert_eq!(w.player(b1).unwrap().shirt_color.b, 172);
    drop(w);
    assert_eq!(text(&game, "Red Flag Status"), "Red flag: home");
    assert_eq!(score(&game), ("RED  0".to_string(), "0  BLUE".to_string()));
    check(&game);
}

#[test]
fn a_taken_flag_rides_on_the_carrier_and_slows_them() {
    let mut game = start(3.0, 480.0);
    let (r1, _, _, _) = teams(&mut game);
    to_stand(&mut game, r1, "Blue");
    run(&mut game, 0.3);
    check(&game);
    assert_eq!(text(&game, "Blue Flag Status"), "Blue flag: TAKEN by Ann");
    assert_eq!(attr(&game, part(&game, "Blue Flag Pole"), "carried_by"), Some(Attribute::Str("Ann".into())));
    assert_eq!(world(&game).player(r1).unwrap().walk_speed, 11.5, "carriers are slower");

    // Run a while: the flag stays on their back.
    game.set_input_for(r1, PlayerInput { move_x: -1.0, move_z: 0.0, jump: false });
    run(&mut game, 1.0);
    let (me, pole) = (pos(&game, r1), pos(&game, part(&game, "Blue Flag Pole")));
    let d = ((me.x - pole.x).powi(2) + (me.z - pole.z).powi(2)).sqrt();
    assert!(d < 1.5 && pole.y > me.y, "on their back: player {me:?} pole {pole:?}");
    assert!(me.x < 110.0, "moved away from the stand: {me:?}");
    check(&game);
}

#[test]
fn capture_scores_only_while_your_own_flag_is_home() {
    let mut game = start(3.0, 480.0);
    let (r1, r2, b1, _) = teams(&mut game);
    // Both sides take the other's flag.
    to_stand(&mut game, r1, "Blue");
    to_stand(&mut game, b1, "Red");
    run(&mut game, 0.3);
    assert!(text(&game, "Red Flag Status").contains("TAKEN by"));
    assert!(text(&game, "Blue Flag Status").contains("TAKEN by"));

    // Blue's carrier runs out into the field.
    place(&mut game, b1, 60.0, 3.0, 20.0);
    run(&mut game, 0.3);

    // Ann brings the Blue flag home, but Red's flag is out: no score.
    to_stand(&mut game, r1, "Red");
    run(&mut game, 0.5);
    assert_eq!(score(&game).0, "RED  0", "can't score while your flag is away");

    // Knock out the Blue carrier: Red's flag drops where they fell.
    let fell_at = pos(&game, b1);
    kill(&mut game, b1);
    run(&mut game, 0.3);
    check(&game);
    assert!(text(&game, "Red Flag Status").starts_with("Red flag: DROPPED"), "{}", text(&game, "Red Flag Status"));
    let pole = pos(&game, part(&game, "Red Flag Pole"));
    assert!((pole.x - fell_at.x).abs() < 1.0 && (pole.z - fell_at.z).abs() < 1.0, "dropped where they fell");
    assert_eq!(score(&game).0, "RED  0", "still can't score: Red's flag is down, not home");
    // Bob touches his own dropped flag: straight home.
    let p = pos(&game, part(&game, "Red Flag Pole"));
    place(&mut game, r2, p.x + 1.0, p.y - 1.0, p.z);
    run(&mut game, 0.3);
    check(&game);
    assert_eq!(text(&game, "Red Flag Status"), "Red flag: home");
    assert_eq!(attr(&game, r2, "returns"), Some(Attribute::Num(1.0)));

    // Now Ann can score.
    to_stand(&mut game, r1, "Red");
    run(&mut game, 0.3);
    check(&game);
    assert_eq!(score(&game).0, "RED  1");
    assert_eq!(text(&game, "Blue Flag Status"), "Blue flag: home", "the captured flag goes home");
    assert_eq!(attr(&game, r1, "captures"), Some(Attribute::Num(1.0)));
    assert!(world(&game).find_first("Spark").is_some(), "fireworks");
    assert_eq!(world(&game).player(r1).unwrap().walk_speed, 16.0, "back to full speed");
    run(&mut game, 2.5);
    assert!(world(&game).find_first("Spark").is_none(), "and they burn out");
}

#[test]
fn a_dropped_flag_goes_home_by_itself_after_ten_seconds() {
    let mut game = start(3.0, 480.0);
    let (r1, _, _, _) = teams(&mut game);
    to_stand(&mut game, r1, "Blue");
    run(&mut game, 0.3);
    // Carry it out into the field, then get knocked out there.
    place(&mut game, r1, 60.0, 3.0, 20.0);
    run(&mut game, 0.5);
    kill(&mut game, r1);
    run(&mut game, 0.3);
    assert!(text(&game, "Blue Flag Status").starts_with("Blue flag: DROPPED"));
    // Ann respawns, but it's not hers to pick up from her own... it IS an
    // enemy flag, so keep her away: she's in her spawn house now anyway.
    run(&mut game, 9.0);
    assert!(text(&game, "Blue Flag Status").starts_with("Blue flag: DROPPED"), "not yet");
    run(&mut game, 1.5);
    check(&game);
    assert_eq!(text(&game, "Blue Flag Status"), "Blue flag: home");
    let (pole, stand) = (pos(&game, part(&game, "Blue Flag Pole")), stand_top(&game, "Blue"));
    assert!((pole.x - stand.x).abs() < 0.01 && (pole.z - stand.z).abs() < 0.01, "back on its stand");
}

#[test]
fn falling_off_the_world_with_a_flag_sends_it_home() {
    let mut game = start(3.0, 480.0);
    let (r1, _, _, _) = teams(&mut game);
    to_stand(&mut game, r1, "Blue");
    run(&mut game, 0.3);
    place(&mut game, r1, 0.0, -200.0, 200.0);
    run(&mut game, 0.5);
    check(&game);
    assert_eq!(text(&game, "Blue Flag Status"), "Blue flag: home");
}

#[test]
fn first_to_the_cap_wins_and_the_match_starts_over() {
    let mut game = start(2.0, 480.0);
    let (r1, _, _, _) = teams(&mut game);
    for _ in 0..2 {
        to_stand(&mut game, r1, "Blue");
        run(&mut game, 0.3);
        to_stand(&mut game, r1, "Red");
        run(&mut game, 0.3);
    }
    check(&game);
    assert!(visible(&game, "Banner"));
    assert_eq!(text(&game, "Banner"), "Red team wins!");
    // Knock a hole in the ruins: the new match rebuilds them.
    let bricks = |game: &Game| {
        let w = world(game);
        let ruins = w.find_first("Ruins").unwrap();
        w.get(ruins).unwrap().children.len()
    };
    let whole = bricks(&game);
    {
        let mut w = world(&game);
        let ruins = w.find_first("Ruins").unwrap();
        let some: Vec<_> = w.get(ruins).unwrap().children.iter().take(10).copied().collect();
        for id in some {
            w.remove(id);
        }
    }
    assert_eq!(bricks(&game), whole - 10);
    // Frozen while the banner's up: picking up does nothing.
    to_stand(&mut game, r1, "Blue");
    run(&mut game, 0.5);
    assert_eq!(text(&game, "Blue Flag Status"), "Blue flag: home");
    run(&mut game, 8.0);
    check(&game);
    assert!(!visible(&game, "Banner"));
    assert_eq!(score(&game), ("RED  0".to_string(), "0  BLUE".to_string()), "a fresh match");
    assert_eq!(bricks(&game), whole, "the ruins are rebuilt");
    let first = { let w = world(&game); let r = w.find_first("Ruins").unwrap(); w.part(w.get(r).unwrap().children[0]).unwrap().position.y };
    assert!(first > -1.0 && first < 20.0, "in place, not left in the hidden copy's spot: {first}");
    assert_eq!(attr(&game, r1, "captures"), Some(Attribute::Num(0.0)));
    assert!(pos(&game, r1).x < -120.0, "everyone back at spawn");
}

#[test]
fn a_tie_at_the_whistle_is_sudden_death() {
    let mut game = start(3.0, 3.0);
    let (r1, _, b1, _) = teams(&mut game);
    run(&mut game, 2.5);
    check(&game);
    assert_eq!(text(&game, "Clock"), "SUDDEN DEATH");
    // Next capture wins, even though it's only 1.
    to_stand(&mut game, b1, "Red");
    run(&mut game, 0.3);
    to_stand(&mut game, b1, "Blue");
    run(&mut game, 0.3);
    check(&game);
    assert_eq!(text(&game, "Banner"), "Blue team wins!");
    let _ = r1;
}

#[test]
fn a_lead_at_the_whistle_wins() {
    let mut game = start(3.0, 4.0);
    let (r1, _, _, _) = teams(&mut game);
    to_stand(&mut game, r1, "Blue");
    run(&mut game, 0.3);
    to_stand(&mut game, r1, "Red");
    run(&mut game, 0.3);
    run(&mut game, 3.0);
    check(&game);
    assert_eq!(text(&game, "Banner"), "Red team wins!");
}

#[test]
fn wading_the_river_is_slow() {
    let mut game = start(3.0, 480.0);
    let (r1, _, _, _) = teams(&mut game);
    place(&mut game, r1, 0.0, 0.0, 25.0);
    run(&mut game, 1.0);
    let y = pos(&game, r1).y;
    assert!(y < 0.0 && y > -2.0, "standing on the riverbed: {y}");
    assert_eq!(world(&game).player(r1).unwrap().walk_speed, 16.0 * 0.6);
    // On the bridge: full speed.
    place(&mut game, r1, 0.0, 7.0, 0.0);
    run(&mut game, 0.5);
    assert!(pos(&game, r1).y > 5.0, "on the deck");
    assert_eq!(world(&game).player(r1).unwrap().walk_speed, 16.0);
}

/// Walks `who` in a straight line for `seconds`.
fn walk(game: &mut Game, who: InstanceId, dx: f32, dz: f32, seconds: f64) {
    game.set_input_for(who, PlayerInput { move_x: dx, move_z: dz, jump: false });
    run(game, seconds);
    game.set_input_for(who, PlayerInput::default());
}

#[test]
fn every_route_across_is_walkable() {
    let mut game = start(3.0, 480.0);
    let (r1, r2, _, b2) = teams(&mut game);

    // The Bridge: up the steps, over the river, down the other side.
    place(&mut game, r1, -40.0, 3.0, 0.0);
    walk(&mut game, r1, 1.0, 0.0, 5.5);
    let p = pos(&game, r1);
    assert!(p.x > 30.0 && p.y > 1.0 && p.y < 4.0, "crossed the bridge: {p:?}");

    // The Tunnel: down the stairs by Red's castle, under the river, and up
    // the stairs on Blue's side.
    place(&mut game, r2, -98.0, 3.0, 44.0);
    walk(&mut game, r2, 1.0, 0.0, 3.0);
    let p = pos(&game, r2);
    assert!(p.y < -8.0, "down in the tunnel: {p:?}");
    walk(&mut game, r2, 1.0, 0.0, 11.5);
    let p = pos(&game, r2);
    assert!(p.x > 95.0 && p.y > 1.0, "came up by Blue's castle: {p:?}");

    // The Ruins: across the ford on the stepping stones, dry-shod.
    place(&mut game, r1, -24.0, 3.0, -45.0);
    game.set_input_for(r1, PlayerInput { move_x: 1.0, move_z: 0.0, jump: false });
    let mut lowest = f32::MAX;
    for _ in 0..(3.0 / FRAME) as usize {
        game.step(FRAME);
        let p = pos(&game, r1);
        if p.x.abs() < 9.0 {
            lowest = lowest.min(p.y);
        }
    }
    game.set_input_for(r1, PlayerInput::default());
    let p = pos(&game, r1);
    assert!(p.x > 12.0, "crossed at the ford: {p:?}");
    assert!(lowest > 1.0, "on the stones, not in the water: lowest {lowest}");

    // Into a castle through the gate, and up the stairs onto the wall.
    place(&mut game, b2, 80.0, 3.0, 0.0);
    walk(&mut game, b2, 1.0, 0.0, 2.2);
    let p = pos(&game, b2);
    assert!(p.x > 104.0 && p.y < 4.0, "through the gate: {p:?}");
    place(&mut game, b2, 104.0, 3.0, 6.0);
    walk(&mut game, b2, 0.0, 1.0, 1.6);
    walk(&mut game, b2, -1.0, 0.0, 0.3);
    let p = pos(&game, b2);
    assert!(p.y > 12.0, "up on the wall-walk: {p:?}");
    check(&game);
}
