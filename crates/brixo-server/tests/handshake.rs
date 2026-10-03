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
    for _ in 0..100 {
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
    let welcome = serde_json::to_string(&brixo_server::protocol::ToClient::Welcome { you: 7, world }).unwrap();
    let update = serde_json::to_string(&brixo_server::protocol::ToClient::UpdateRequired { min: 2 }).unwrap();
    let (addr, _hello) = fake_server(vec![r#"{"SomethingNew":{"x":1}}"#.to_string(), welcome, update]);
    let mut c = NetClient::connect(&addr, "Ann").unwrap();
    wait_for(&mut c, |c| c.update_required);
    assert!(c.me.is_some(), "the Welcome after the unknown message still arrived");
    assert!(c.update_required);
}
