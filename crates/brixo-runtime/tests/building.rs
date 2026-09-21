//! Models (welded groups) and part shapes, in a running game.

use brixo_core::{Class, DataModel, InstanceId, Shape, Vec3};
use brixo_runtime::Game;

const FRAME: f64 = 1.0 / 60.0;

fn floor() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(100.0, 1.0, 100.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    // Keep the player out of the way.
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(40.0, 0.5, 40.0);
    dm
}

fn part(dm: &mut DataModel, parent: InstanceId, pos: Vec3, size: Vec3, anchored: bool) -> InstanceId {
    let id = dm.create(Class::Part, "P", parent).unwrap();
    let p = dm.part_mut(id).unwrap();
    p.position = pos;
    p.size = size;
    p.anchored = anchored;
    id
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn at(game: &Game, id: InstanceId) -> Vec3 {
    game.world().part(id).unwrap().position
}

#[test]
fn a_model_falls_as_one_piece() {
    let mut dm = floor();
    let root = dm.root();
    let table = dm.create(Class::Model, "Table", root).unwrap();
    // A top resting on one leg, off centre: welded, it tips over as one
    // piece instead of the top sliding off.
    let top = part(&mut dm, table, Vec3::new(0.0, 6.0, 0.0), Vec3::new(6.0, 1.0, 2.0), false);
    let leg = part(&mut dm, table, Vec3::new(2.5, 3.5, 0.0), Vec3::new(1.0, 4.0, 1.0), false);
    let mut game = Game::start(dm);
    let dist = |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    let d0 = dist(at(&game, top), at(&game, leg));
    run(&mut game, 3.0);
    let (t, l) = (at(&game, top), at(&game, leg));
    let d = dist(t, l);
    assert!((d - d0).abs() < 0.1, "welded parts keep their distance: {d0} -> {d}");
    assert!(t.y < 5.0, "and the table fell over: top at {t:?}");
}

#[test]
fn parts_welded_to_an_anchored_part_hang_in_the_air() {
    let mut dm = floor();
    let root = dm.root();
    let sign = dm.create(Class::Model, "Sign", root).unwrap();
    let _post = part(&mut dm, sign, Vec3::new(0.0, 4.0, 0.0), Vec3::new(1.0, 8.0, 1.0), true);
    let board = part(&mut dm, sign, Vec3::new(2.5, 7.0, 0.0), Vec3::new(4.0, 2.0, 0.5), false);
    let mut game = Game::start(dm);
    run(&mut game, 2.0);
    assert!((at(&game, board).y - 7.0).abs() < 0.1, "the board stays up: {:?}", at(&game, board));
}

#[test]
fn loose_parts_outside_models_are_not_welded() {
    let mut dm = floor();
    let root = dm.root();
    let _base = part(&mut dm, root, Vec3::new(0.0, 4.0, 0.0), Vec3::new(1.0, 8.0, 1.0), true);
    let board = part(&mut dm, root, Vec3::new(2.5, 7.0, 0.0), Vec3::new(4.0, 2.0, 0.5), false);
    let mut game = Game::start(dm);
    run(&mut game, 2.0);
    assert!(at(&game, board).y < 2.0, "not in a model, so it falls");
}

#[test]
fn a_ball_rests_on_its_radius_and_a_cylinder_on_its_end() {
    let mut dm = floor();
    let root = dm.root();
    let ball = part(&mut dm, root, Vec3::new(0.0, 5.0, 0.0), Vec3::new(3.0, 3.0, 3.0), false);
    dm.part_mut(ball).unwrap().shape = Shape::Ball;
    let can = part(&mut dm, root, Vec3::new(8.0, 5.0, 0.0), Vec3::new(2.0, 4.0, 2.0), false);
    dm.part_mut(can).unwrap().shape = Shape::Cylinder;
    let mut game = Game::start(dm);
    run(&mut game, 2.5);
    assert!((at(&game, ball).y - 1.5).abs() < 0.1, "ball at {:?}", at(&game, ball));
    assert!((at(&game, can).y - 2.0).abs() < 0.1, "cylinder at {:?}", at(&game, can));
}

#[test]
fn a_ball_rolls_down_a_wedge() {
    let mut dm = floor();
    let root = dm.root();
    // A ramp rising toward +Z; a ball dropped on its high end rolls to -Z.
    let ramp = part(&mut dm, root, Vec3::new(0.0, 2.0, 0.0), Vec3::new(6.0, 4.0, 10.0), true);
    dm.part_mut(ramp).unwrap().shape = Shape::Wedge;
    let ball = part(&mut dm, root, Vec3::new(0.0, 6.0, 3.5), Vec3::new(1.0, 1.0, 1.0), false);
    dm.part_mut(ball).unwrap().shape = Shape::Ball;
    let mut game = Game::start(dm);
    run(&mut game, 3.0);
    assert!(at(&game, ball).z < -5.0, "rolled off the low end: {:?}", at(&game, ball));
}

#[test]
fn scripts_can_change_a_parts_shape() {
    let mut dm = floor();
    let root = dm.root();
    let p = part(&mut dm, root, Vec3::new(0.0, 5.0, 0.0), Vec3::new(2.0, 2.0, 2.0), true);
    let s = dm.create(Class::Script, "S", p).unwrap();
    dm.script_mut(s).unwrap().source = "self.shape = \"ball\"\nprint(self.shape)\nself.shape = \"star\"".into();
    let game = Game::start(dm);
    let log = game.take_log();
    assert_eq!(log[0].text, "ball");
    assert!(log[1].is_error && log[1].text.contains("no shape called 'star'"));
    assert_eq!(game.world().part(p).unwrap().shape, Shape::Ball);
}
