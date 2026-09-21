//! Real sockets: a server and players connecting to it.

use std::time::{Duration, Instant};

use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::PlayerInput;
use brixo_server::NetClient;

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let s = dm.create(Class::Script, "Greeter", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n print(\"welcome \" + p.name)\nend".into();
    dm
}

/// Polls every client until `done` says yes (or fails after 5 seconds).
fn wait_for(clients: &mut [&mut NetClient], what: &str, done: impl Fn(&[&mut NetClient]) -> bool) {
    let start = Instant::now();
    loop {
        for c in clients.iter_mut() {
            c.poll();
        }
        if done(clients) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn player_count(c: &NetClient) -> usize {
    c.world.walk().into_iter().filter(|id| c.world.player(*id).is_some()).count()
}

#[test]
fn players_join_see_each_other_move_and_leave() {
    let server = brixo_server::start(arena(), 0).unwrap();
    let addr = server.local_addr();
    let mut ann = NetClient::connect(&addr, "Ann").unwrap();
    let mut ann2 = NetClient::connect(&addr, "Ann").unwrap();

    wait_for(&mut [&mut ann, &mut ann2], "both see two players", |cs| {
        cs.iter().all(|c| c.me.is_some() && player_count(c) == 2)
    });
    let (first, second) = (ann.me.unwrap(), ann2.me.unwrap());
    assert_ne!(first, second);
    assert_eq!(ann2.world.get(second).unwrap().name, "Ann2", "names are made unique");
    assert_eq!(server.player_count(), 2);

    // Ann walks; the other player sees her move.
    let start_z = ann2.world.player(first).unwrap().body.position.z;
    ann.send_input(PlayerInput { move_z: 1.0, ..Default::default() });
    wait_for(&mut [&mut ann, &mut ann2], "Ann to move", |cs| {
        cs[1].world.player(first).is_some_and(|p| p.body.position.z > start_z + 5.0)
    });
    // ...and she moved in her own view too; the second player didn't.
    assert!(ann.world.player(first).unwrap().body.position.z > start_z + 5.0);

    // The greeter script ran on the server, once per player.
    std::thread::sleep(Duration::from_millis(100));
    let log: Vec<String> = server.take_log().into_iter().map(|l| l.text).collect();
    for expected in ["welcome Ann", "welcome Ann2", "Ann joined", "Ann2 joined"] {
        assert!(log.contains(&expected.to_string()), "{expected} missing from {log:?}");
    }

    // The second player leaves: everyone else sees them go.
    drop(ann2);
    wait_for(&mut [&mut ann], "Ann2 to leave", |cs| player_count(cs[0]) == 1);
    assert!(ann.world.get(second).is_none());
}

#[test]
fn players_never_receive_script_code() {
    let server = brixo_server::start(arena(), 0).unwrap();
    let mut p = NetClient::connect(&server.local_addr(), "Snoop").unwrap();
    wait_for(&mut [&mut p], "welcome", |cs| cs[0].me.is_some());
    let greeter = p.world.find_first("Greeter").expect("the script exists");
    assert_eq!(p.world.script(greeter).unwrap().source, "");
}

#[test]
fn stopping_the_server_disconnects_players() {
    let server = brixo_server::start(arena(), 0).unwrap();
    let mut p = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut [&mut p], "welcome", |cs| cs[0].me.is_some());
    server.stop();
    wait_for(&mut [&mut p], "disconnect", |cs| !cs[0].connected);
}

#[test]
fn inputs_are_capped_at_walking_speed() {
    use std::io::Write;
    let server = brixo_server::start(arena(), 0).unwrap();
    let mut watcher = NetClient::connect(&server.local_addr(), "Watcher").unwrap();
    // A hacked client sends a huge move vector by hand.
    let mut cheat = std::net::TcpStream::connect(server.local_addr()).unwrap();
    writeln!(cheat, r#"{{"Hello":{{"name":"Cheater"}}}}"#).unwrap();
    wait_for(&mut [&mut watcher], "cheater joins", |cs| player_count(cs[0]) == 2);
    let cheater = watcher.world.find_first("Cheater").unwrap();
    let z0 = watcher.world.player(cheater).unwrap().body.position.z;
    writeln!(cheat, r#"{{"Input":{{"move_x":0.0,"move_z":50.0,"jump":false}}}}"#).unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    watcher.poll();
    let moved = watcher.world.player(cheater).unwrap().body.position.z - z0;
    assert!(moved > 8.0 && moved < 20.0, "moved {moved} studs in a second at walk speed 16");
}
