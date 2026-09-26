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

#[test]
fn players_click_buttons_and_use_tools_over_the_network() {
    let mut dm = arena();
    let root = dm.root();
    let button = dm.create(Class::TextButton, "Hello", root).unwrap();
    let s = dm.create(Class::Script, "S", button).unwrap();
    dm.script_mut(s).unwrap().source = "on clicked(p)\n self.text = \"Hi \" + p.name\nend".into();
    let tool = dm.create(Class::Tool, "Wand", root).unwrap();
    let handle = dm.create(Class::Part, "Handle", tool).unwrap();
    dm.part_mut(handle).unwrap().position = Vec3::new(30.0, 1.0, 0.0);
    let s = dm.create(Class::Script, "S", tool).unwrap();
    dm.script_mut(s).unwrap().source = "on activated(p)\n print(p.name + \" zapped\")\nend".into();
    let s = dm.create(Class::Script, "Give", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n w = clone(find(\"Wand\"))\n w.parent = p\nend".into();

    let server = brixo_server::start(dm, 0).unwrap();
    let mut ann = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut [&mut ann], "welcome", |cs| cs[0].me.is_some());

    ann.click(button);
    wait_for(&mut [&mut ann], "the button's new text", |cs| {
        cs[0].world.gui(button).is_some_and(|g| g.text == "Hi Ann")
    });

    ann.equip(0);
    let me = ann.me.unwrap();
    wait_for(&mut [&mut ann], "the wand in hand", |cs| cs[0].world.player(me).unwrap().equipped.is_some());
    ann.activate();
    std::thread::sleep(Duration::from_millis(150));
    let log: Vec<String> = server.take_log().into_iter().map(|l| l.text).collect();
    assert!(log.contains(&"Ann zapped".to_string()), "{log:?}");
}

#[test]
fn ticketed_servers_let_in_ticket_holders_as_their_account() {
    use std::sync::{Arc, Mutex};
    let look = brixo_runtime::Look {
        skin: brixo_core::Color::new(204, 142, 105),
        shirt: brixo_core::Color::new(13, 105, 172),
        pants: brixo_core::Color::new(27, 42, 53),
        shoes: brixo_core::Color::new(27, 27, 27),
        face: brixo_core::Face::Determined,
    };
    // One valid ticket, usable once (the website's rule).
    let valid = Arc::new(Mutex::new(Some("good-ticket".to_string())));
    let check: brixo_server::TicketCheck = Arc::new(move |t: &str| {
        let mut v = valid.lock().unwrap();
        if v.as_deref() == Some(t) {
            *v = None;
            Some(brixo_server::Identity { name: "Ann".into(), look })
        } else {
            None
        }
    });
    let server = brixo_server::start_with_tickets(arena(), 0, check).unwrap();
    let addr = server.local_addr();

    let mut ann = NetClient::connect_with_ticket(&addr, "good-ticket").unwrap();
    wait_for(&mut [&mut ann], "Ann joins", |cs| cs[0].me.is_some() && player_count(cs[0]) == 1);
    let me = ann.me.unwrap();
    assert_eq!(ann.world.get(me).unwrap().name, "Ann", "named by the ticket, not by the client");
    let p = ann.world.player(me).unwrap();
    assert_eq!(p.face, brixo_core::Face::Determined);
    assert_eq!((p.shirt_color.r, p.shirt_color.g, p.shirt_color.b), (13, 105, 172));

    // Reusing the ticket, a made-up one, or none at all: turned away.
    for ticket in [Some("good-ticket"), Some("made-up"), None] {
        let mut c = match ticket {
            Some(t) => NetClient::connect_with_ticket(&addr, t).unwrap(),
            None => NetClient::connect(&addr, "Sneaky").unwrap(),
        };
        wait_for(&mut [&mut c], "turned away", |cs| !cs[0].connected);
        assert!(c.me.is_none(), "{ticket:?} got in");
    }
    assert_eq!(server.player_count(), 1);
}

#[test]
fn sounds_reach_players_once_and_stay_out_of_world_updates() {
    let mut dm = arena();
    let root = dm.root();
    let horn = dm.create(Class::Sound, "Horn", root).unwrap();
    let bytes: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
    *dm.sound_mut(horn).unwrap() = brixo_core::SoundProps::from_bytes("wav", &bytes);
    let server = brixo_server::start(dm, 0).unwrap();
    let mut ann = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut [&mut ann], "the sound's audio", |cs| cs[0].assets.contains_key(&horn));
    assert_eq!(*ann.assets[&horn], bytes, "arrived intact");
    // The world copy has the Sound, but not its audio (that went once, separately).
    assert!(ann.world.sound(horn).unwrap().data.is_empty());
}

#[test]
fn storage_stays_on_the_server_and_changes_arrive_as_changes() {
    let mut dm = arena();
    let root = dm.root();
    let storage = dm.create(Class::Folder, "Storage", root).unwrap();
    let template = dm.create(Class::Part, "Secret Template", storage).unwrap();
    dm.part_mut(template).unwrap().position = Vec3::new(0.0, -300.0, 0.0);
    let still = dm.create(Class::Part, "Still", root).unwrap();
    dm.part_mut(still).unwrap().position = Vec3::new(20.0, 1.0, 0.0);
    // After joining: a custom field, and a copy of the template that comes
    // and goes.
    let s = dm.create(Class::Script, "S", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n p.team = \"Red\"\n wait(0.3)\n c = clone(find(\"Secret Template\"))\n c.name = \"Copy\"\n c.parent = find(\"Workspace\")\n wait(0.3)\n destroy(c)\nend".into();

    let server = brixo_server::start(dm, 0).unwrap();
    let mut ann = NetClient::connect(&server.local_addr(), "Ann").unwrap();
    wait_for(&mut [&mut ann], "joined", |cs| cs[0].me.is_some());
    assert!(ann.world.get(storage).is_none() && ann.world.get(template).is_none(), "Storage never reaches players");

    let me = ann.me.unwrap();
    wait_for(&mut [&mut ann], "the team field", |cs| {
        matches!(cs[0].world.get(me).unwrap().attributes.get("team"), Some(brixo_core::Attribute::Str(t)) if t == "Red")
    });
    wait_for(&mut [&mut ann], "the copy appearing", |cs| cs[0].world.find_first("Copy").is_some());
    wait_for(&mut [&mut ann], "the copy going away", |cs| cs[0].world.find_first("Copy").is_none());
    assert_eq!(ann.world.part(still).unwrap().position, Vec3::new(20.0, 1.0, 0.0), "untouched parts are still right");
}
