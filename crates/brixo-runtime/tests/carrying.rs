//! `carried_by`: a part rides along with a player, turning as they turn,
//! stays where they fell while they're knocked out, and lets go when told.

use brixo_core::{Attribute, Class, DataModel, Vec3};
use brixo_runtime::{Game, PlayerInput};
use std::time::Duration;

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

#[test]
fn a_carried_part_rides_on_the_players_back() {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    dm.part_mut(floor).unwrap().size = Vec3::new(200.0, 1.0, 200.0);
    let pack = dm.create(Class::Part, "Backpack", root).unwrap();
    {
        let p = dm.part_mut(pack).unwrap();
        p.position = Vec3::new(50.0, 5.0, 50.0);
        p.size = Vec3::new(1.0, 1.0, 1.0);
        p.can_collide = false;
    }
    for (k, v) in [("carry_y", 1.0), ("carry_z", -1.0)] {
        dm.get_mut(pack).unwrap().attributes.insert(k.into(), Attribute::Num(v));
    }
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    run(&mut game, 0.5);
    game.world_within(Duration::from_secs(5)).get_mut(pack).unwrap().attributes.insert("carried_by".into(), Attribute::Str("Ann".into()));

    // Walking along +X: the pack is a stud up and a stud behind (-X).
    game.set_input_for(ann, PlayerInput { move_x: 1.0, move_z: 0.0, jump: false });
    run(&mut game, 1.0);
    let (me, it) = {
        let w = game.world_within(Duration::from_secs(5));
        (w.player(ann).unwrap().body.position, w.part(pack).unwrap().position)
    };
    assert!((it.x - (me.x - 1.0)).abs() < 0.05 && (it.y - (me.y + 1.0)).abs() < 0.05 && (it.z - me.z).abs() < 0.05, "behind them: {me:?} {it:?}");

    // Turn to walk along +Z: it swings round to stay behind.
    game.set_input_for(ann, PlayerInput { move_x: 0.0, move_z: 1.0, jump: false });
    run(&mut game, 0.5);
    let (me, it) = {
        let w = game.world_within(Duration::from_secs(5));
        (w.player(ann).unwrap().body.position, w.part(pack).unwrap().position)
    };
    assert!((it.z - (me.z - 1.0)).abs() < 0.05 && (it.x - me.x).abs() < 0.05, "still behind: {me:?} {it:?}");

    // Knocked out: it stays put where they fell.
    game.world_within(Duration::from_secs(5)).player_mut(ann).unwrap().health = 0.0;
    run(&mut game, 0.2);
    let fell = game.world_within(Duration::from_secs(5)).part(pack).unwrap().position;
    run(&mut game, 1.0);
    assert_eq!(game.world_within(Duration::from_secs(5)).part(pack).unwrap().position, fell);

    // Let go: it stays where it is even after they're back.
    game.world_within(Duration::from_secs(5)).get_mut(pack).unwrap().attributes.remove("carried_by");
    run(&mut game, 4.0);
    assert_eq!(game.world_within(Duration::from_secs(5)).part(pack).unwrap().position, fell);
}
