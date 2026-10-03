//! How walking feels at different pings: a real server, a real connection
//! with pretend lag (brixo_server::lag), and the Player's prediction, run
//! for a scripted few seconds of walking, turning, jumping and stopping.
//!
//! Quick check (runs with the other tests): prediction responds at once
//! and stays smooth at a normal ping.
//! The full table (slower; prints numbers to compare before/after a change):
//!   cargo test -p brixo-client --test lag_feel -- --ignored --nocapture

use std::time::{Duration, Instant};

use brixo_client::Predictor;
use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::PlayerInput;
use brixo_server::lag::Lag;
use brixo_server::NetClient;

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(400.0, 1.0, 400.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    dm
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

#[derive(Debug)]
struct Feel {
    /// From pressing a key to your character moving on your screen.
    respond_ms: f32,
    /// From pressing a key to the server moving you (what others see).
    server_ms: f32,
    /// Frames where your character moved sideways further than walking
    /// can go: a visible correction.
    pops: usize,
    biggest_pop: f32,
    /// Off from the server once you've stopped and settled.
    settled_off: f32,
}

/// The input at `t` seconds into the script.
fn script(t: f32) -> PlayerInput {
    match t {
        t if t < 1.5 => PlayerInput { move_z: 1.0, ..Default::default() },
        t if t < 2.5 => PlayerInput { move_x: 1.0, ..Default::default() },
        t if t < 3.0 => PlayerInput { move_z: -1.0, jump: true, ..Default::default() },
        t if t < 3.5 => PlayerInput { move_z: -1.0, ..Default::default() },
        _ => PlayerInput::default(),
    }
}
const SCRIPT_SECONDS: f32 = 5.0;

fn measure(lag: Option<Lag>) -> Feel {
    let server = brixo_server::start(arena(), 0).unwrap();
    let mut net = NetClient::connect_lagged(&server.local_addr(), "Ann", lag).unwrap();
    let mut predictor = Predictor::default();
    let frame = Duration::from_secs_f32(1.0 / 60.0);

    // Join, and stand still until we've landed.
    let start = Instant::now();
    loop {
        net.poll();
        predictor.step(&net.world, net.me, PlayerInput::default(), None, 1.0 / 60.0);
        if net.me.is_some() && start.elapsed() > Duration::from_millis(1500) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "never joined");
        std::thread::sleep(frame);
    }
    let me = net.me.unwrap();
    let origin_pred = predictor.position().unwrap();
    let origin_server = net.world.player(me).unwrap().body.position;

    let mut feel = Feel { respond_ms: f32::NAN, server_ms: f32::NAN, pops: 0, biggest_pop: 0.0, settled_off: 0.0 };
    let began = Instant::now();
    let mut last = origin_pred;
    let mut last_t = Instant::now();
    loop {
        let t = began.elapsed().as_secs_f32();
        if t > SCRIPT_SECONDS {
            break;
        }
        let input = script(t);
        net.send_input(input);
        net.poll();
        let now = Instant::now();
        let dt = (now - last_t).as_secs_f32().min(0.1);
        last_t = now;
        predictor.step(&net.world, Some(me), input, None, dt);
        let here = predictor.position().unwrap_or(last);
        let server_at = net.world.player(me).map(|p| p.body.position).unwrap_or(origin_server);
        if feel.respond_ms.is_nan() && dist(here, origin_pred) > 0.05 {
            feel.respond_ms = t * 1000.0;
        }
        if feel.server_ms.is_nan() && dist(server_at, origin_server) > 0.05 {
            feel.server_ms = t * 1000.0;
        }
        // Walking covers about 16 studs a second; sideways steps well past
        // that are a correction you'd see. (Up and down is left out: a
        // fall is legitimately fast.)
        let step = dist(Vec3::new(here.x, 0.0, here.z), Vec3::new(last.x, 0.0, last.z));
        // (A frame can hold two physics steps when the timing lands that
        // way, so two steps' walking is allowed too.)
        let allowed = (20.0 * dt).max(2.0 * 16.0 / 60.0) + 0.05;
        if step > allowed {
            feel.pops += 1;
            if std::env::var_os("LAGFEEL_DEBUG").is_some() { eprintln!("pop t={t:.2} step={step:.2} dt={dt:.3} y={:.2}", here.y); }
            feel.biggest_pop = feel.biggest_pop.max(step);
        }
        last = here;
        std::thread::sleep(frame);
    }
    let server_at = net.world.player(me).unwrap().body.position;
    feel.settled_off = dist(last, server_at);
    feel
}

#[test]
fn prediction_responds_at_once_and_stays_smooth() {
    let feel = measure(Some(Lag { ping_ms: 150.0, jitter_ms: 10.0, stall_percent: 0.0 }));
    println!("{feel:?}");
    assert!(feel.respond_ms < 60.0, "you move within a few frames of pressing a key: {feel:?}");
    assert!(feel.server_ms > 60.0, "the server hears about it a ping later: {feel:?}");
    assert!(feel.pops <= 1, "no visible corrections walking at 150 ms: {feel:?}");
    assert!(feel.settled_off < 0.5, "once stopped, you're where the server says: {feel:?}");
}

#[test]
#[ignore]
fn the_table() {
    let cases: [(&str, Option<Lag>); 6] = [
        ("no lag", None),
        ("50 ms", Some(Lag { ping_ms: 50.0, jitter_ms: 0.0, stall_percent: 0.0 })),
        ("100 ms", Some(Lag { ping_ms: 100.0, jitter_ms: 0.0, stall_percent: 0.0 })),
        ("200 ms", Some(Lag { ping_ms: 200.0, jitter_ms: 0.0, stall_percent: 0.0 })),
        ("200 ms + jitter", Some(Lag { ping_ms: 200.0, jitter_ms: 40.0, stall_percent: 0.0 })),
        ("200 ms + 3% stalls", Some(Lag { ping_ms: 200.0, jitter_ms: 40.0, stall_percent: 3.0 })),
    ];
    println!("\n{:<20} {:>10} {:>10} {:>6} {:>10} {:>10}", "connection", "respond", "server", "pops", "worst pop", "settled");
    for (name, lag) in cases {
        let f = measure(lag);
        println!("{:<20} {:>8.0}ms {:>8.0}ms {:>6} {:>7.2} st {:>7.2} st", name, f.respond_ms, f.server_ms, f.pops, f.biggest_pop, f.settled_off);
    }
}
