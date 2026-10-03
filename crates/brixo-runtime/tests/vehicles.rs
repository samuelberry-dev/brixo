//! Vehicles: seats you walk into, your keys going to the seat, and a car
//! built from hinges and motors that a script drives from them.

use brixo_core::{Class, DataModel, Hinge, InstanceId, Shape, Vec3};
use brixo_runtime::{Game, PlayerInput};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn part(dm: &mut DataModel, parent: InstanceId, name: &str, pos: Vec3, size: Vec3) -> InstanceId {
    let id = dm.create(Class::Part, name, parent).unwrap();
    let p = dm.part_mut(id).unwrap();
    p.position = pos;
    p.size = size;
    p.anchored = false;
    id
}

/// The driving script the Toolbox car uses: tank steering, from the seat.
pub const CAR_SCRIPT: &str = "seat = nil
left = []
right = []
for c in self.children do
    if c.name == \"Seat\" then
        seat = c
    elseif c.name == \"Left Wheel\" then
        push(left, c)
    elseif c.name == \"Right Wheel\" then
        push(right, c)
    end
end
-- Degrees a second at full throttle. (Wheels turned the other way round
-- drive backwards: flip the sign.)
speed = -300
-- How hard it turns: the two sides run this much apart.
turning = 2
every 0.05 seconds
    go = seat.throttle
    turn = seat.steer
    if go == nil then
        go = 0
    end
    if turn == nil then
        turn = 0
    end
    for w in left do
        w.motor_speed = (go - turn * turning) * speed
    end
    for w in right do
        w.motor_speed = (go + turn * turning) * speed
    end
end
";

/// A floor, a spawn, and a car: body first, a seat on top facing +Z, four
/// hinged wheels, and the driving script.
fn car_world() -> (DataModel, InstanceId, InstanceId) {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(400.0, 1.0, 400.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(-30.0, 0.5, -30.0);
    let car = dm.create(Class::Model, "Car", root).unwrap();
    let body = part(&mut dm, car, "Body", Vec3::new(0.0, 2.0, 0.0), Vec3::new(4.0, 1.0, 8.0));
    let seat = part(&mut dm, car, "Seat", Vec3::new(0.0, 3.0, -1.5), Vec3::new(2.0, 1.0, 2.0));
    dm.part_mut(seat).unwrap().seat = true;
    for (x, z) in [(-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5), (2.5, 2.5)] {
        let name = if x < 0.0 { "Left Wheel" } else { "Right Wheel" };
        let w = part(&mut dm, car, name, Vec3::new(x, 1.5, z), Vec3::new(3.0, 1.0, 3.0));
        let p = dm.part_mut(w).unwrap();
        p.shape = Shape::Cylinder;
        p.rotation = Vec3::new(0.0, 0.0, 90.0);
        p.hinge = Hinge::Y;
    }
    let script = dm.create(Class::Script, "Drive", car).unwrap();
    dm.script_mut(script).unwrap().source = CAR_SCRIPT.into();
    (dm, body, seat)
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

#[test]
fn walk_into_a_seat_drive_with_its_keys_and_jump_out() {
    let (dm, body, seat) = car_world();
    let mut game = Game::start_server(dm);
    let me = game.add_player("Ann");
    run(&mut game, 0.5);
    // Drop onto the seat: touching it sits you down.
    let above = game.world().part(seat).unwrap().position;
    game.world().player_mut(me).unwrap().body.position = Vec3::new(above.x, above.y + 4.0, above.z);
    run(&mut game, 1.0);
    assert_eq!(game.world().player(me).unwrap().seat, Some(seat), "sat down");
    let log: Vec<String> = game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect();
    assert!(log.is_empty(), "the car's script runs cleanly: {log:?}");

    // W: the seat says throttle 1, the script turns the motors, the car goes.
    let start = game.world().part(body).unwrap().position;
    game.set_input_for(me, PlayerInput { move_z: 1.0, ..Default::default() });
    run(&mut game, 0.2);
    assert!(matches!(game.world().get(seat).unwrap().attributes.get("throttle"), Some(brixo_core::Attribute::Num(n)) if *n == 1.0));
    run(&mut game, 3.0);
    let after = game.world().part(body).unwrap().position;
    assert!(flat(after, start) > 8.0, "the car drove: {start:?} -> {after:?}");
    assert!(after.z > start.z, "forwards is the way the seat faces (+Z): {start:?} -> {after:?}");
    let me_at = game.world().player(me).unwrap().body.position;
    let seat_at = game.world().part(seat).unwrap().position;
    assert!(flat(me_at, seat_at) < 0.5 && me_at.y > seat_at.y, "still sitting on it: {me_at:?} vs {seat_at:?}");

    // A: turning left (tank steering).
    let yaw0 = game.world().part(body).unwrap().rotation.y;
    game.set_input_for(me, PlayerInput { move_x: 1.0, ..Default::default() });
    run(&mut game, 2.0);
    let yaw1 = game.world().part(body).unwrap().rotation.y;
    let turned = (yaw1 - yaw0 + 540.0).rem_euclid(360.0) - 180.0;
    assert!(turned < -20.0, "A turns it left (yaw goes down, towards -X): {yaw0} -> {yaw1}");

    // Space: up and out, and not straight back in.
    game.set_input_for(me, PlayerInput { jump: true, ..Default::default() });
    run(&mut game, 0.3);
    game.set_input_for(me, PlayerInput::default());
    run(&mut game, 0.6);
    assert_eq!(game.world().player(me).unwrap().seat, None, "got up");
    assert!(matches!(game.world().get(seat).unwrap().attributes.get("throttle"), Some(brixo_core::Attribute::Num(n)) if *n == 0.0), "an empty seat has no throttle");
}

#[test]
fn scripts_seat_players_and_read_who_is_sitting() {
    let (mut dm, _body, _seat) = car_world();
    let root = dm.root();
    let s = dm.create(Class::Script, "Seater", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)
    seat = find(\"Seat\")
    print(seat.seat, seat.occupant)
    p.seat = seat
    print(seat.occupant.name, p.seat.name)
    wait(0.5)
    p.seat = nil
    print(seat.occupant)
end
"
    .into();
    let mut game = Game::start_server(dm);
    let _me = game.add_player("Ann");
    run(&mut game, 1.0);
    let log: Vec<String> = game.take_log().into_iter().map(|l| l.text).collect();
    for want in ["true nil", "Ann Seat", "nil"] {
        assert!(log.iter().any(|l| l == want), "{want} missing from {log:?}");
    }
}
