//! The version handshake: Players say which protocol they speak, and a
//! Player keeps going past messages it doesn't understand.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use brixo_server::protocol::{ToServer, PROTOCOL};
use brixo_server::NetClient;

/// A fake server: reads the Hello, then sends these lines.
fn fake_server(lines: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut hello = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut hello).unwrap();
        tx.send(hello).unwrap();
        for l in lines {
            stream.write_all(l.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    });
    (addr, rx)
}

fn wait_for(client: &mut NetClient, ok: impl Fn(&NetClient) -> bool) {
    for _ in 0..300 {
        client.poll();
        if ok(client) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn hello_says_which_protocol() {
    let (addr, hello) = fake_server(vec![]);
    let _c = NetClient::connect(&addr, "Ann").unwrap();
    let msg: ToServer = serde_json::from_str(&hello.recv().unwrap()).unwrap();
    assert!(matches!(msg, ToServer::Hello { protocol, .. } if protocol == PROTOCOL));
    // A Player from before the handshake still makes a valid Hello (0).
    let old: ToServer = serde_json::from_str(r#"{"Hello":{"name":"Ann","ticket":null}}"#).unwrap();
    assert!(matches!(old, ToServer::Hello { protocol: 0, .. }));
}

#[test]
fn unknown_messages_are_skipped_and_update_required_is_heard() {
    let world = brixo_core::DataModel::new().to_json().unwrap();
    let welcome = serde_json::to_string(&brixo_server::protocol::ToClient::Welcome { you: 7, world, steps: true }).unwrap();
    let update = serde_json::to_string(&brixo_server::protocol::ToClient::UpdateRequired { min: 2 }).unwrap();
    let (addr, _hello) = fake_server(vec![r#"{"SomethingNew":{"x":1}}"#.to_string(), welcome, update]);
    let mut c = NetClient::connect(&addr, "Ann").unwrap();
    wait_for(&mut c, |c| c.update_required);
    assert!(c.me.is_some(), "the Welcome after the unknown message still arrived");
    assert!(c.update_required);
}

#[test]
fn numbered_steps_are_applied_in_order_and_confirmed() {
    use brixo_core::{Class, DataModel, Vec3};
    use brixo_runtime::PlayerInput;
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let server = brixo_server::start(dm, 0).unwrap();
    let mut c = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut c, |c| c.me.is_some());
    assert!(c.server_steps, "this server takes numbered steps");
    // 60 steps walking +z (a second's worth), sent as a Player would:
    // a few at a time, as they happen.
    let walk = PlayerInput { move_z: 1.0, ..Default::default() };
    for batch in 0..6 {
        c.send_steps(1 + batch * 10, &[walk; 10]);
        std::thread::sleep(std::time::Duration::from_millis(160));
    }
    wait_for(&mut c, |c| c.you.is_some_and(|(ack, _)| ack >= 60));
    let (_, state) = c.you.unwrap();
    // A second of walking at 16 studs/s.
    assert!((state.position.z - 16.0).abs() < 1.5, "walked about 16 studs: {state:?}");
    // Then standing still.
    c.send_steps(61, &[PlayerInput::default(); 10]);
    wait_for(&mut c, |c| c.you.is_some_and(|(ack, _)| ack == 70));
    std::thread::sleep(std::time::Duration::from_millis(200));
    c.poll();
    let (_, stopped) = c.you.unwrap();
    // Steps it already has, sent again, are ignored: no walking again.
    c.send_steps(50, &[walk; 11]);
    std::thread::sleep(std::time::Duration::from_millis(300));
    c.poll();
    let (ack, now) = c.you.unwrap();
    assert_eq!(ack, 70);
    assert!((now.position.z - stopped.position.z).abs() < 0.05, "the repeats didn't walk again: {now:?} vs {stopped:?}");
}

#[test]
fn online_a_seated_players_keys_drive_the_seat() {
    use brixo_core::{Class, DataModel, Vec3};
    use brixo_runtime::PlayerInput;
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let seat = dm.create(Class::Part, "Seat", root).unwrap();
    let s = dm.part_mut(seat).unwrap();
    s.position = Vec3::new(10.0, 0.5, 10.0);
    s.size = Vec3::new(2.0, 1.0, 2.0);
    s.seat = true;
    let script = dm.create(Class::Script, "Seater", root).unwrap();
    dm.script_mut(script).unwrap().source = "on player_joined(p)\n    p.seat = find(\"Seat\")\nend\n".into();
    let server = brixo_server::start(dm, 0).unwrap();
    let mut c = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut c, |c| c.me.is_some_and(|me| c.world.player(me).is_some_and(|p| p.seat.is_some())));
    // Numbered steps of W: the seat hears throttle 1, everyone sees it.
    let w = PlayerInput { move_z: 1.0, ..Default::default() };
    for batch in 0..3 {
        c.send_steps(1 + batch * 10, &[w; 10]);
        std::thread::sleep(std::time::Duration::from_millis(160));
    }
    let seat_id = c.world.walk().into_iter().find(|i| c.world.get(*i).unwrap().name == "Seat").unwrap();
    wait_for(&mut c, |c| matches!(c.world.get(seat_id).unwrap().attributes.get("throttle"), Some(brixo_core::Attribute::Num(n)) if *n == 1.0));
    assert!(matches!(c.world.get(seat_id).unwrap().attributes.get("throttle"), Some(brixo_core::Attribute::Num(n)) if *n == 1.0));
}
