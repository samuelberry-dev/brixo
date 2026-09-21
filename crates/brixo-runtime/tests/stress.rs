//! Performance checks. Run with --release for meaningful timings:
//!     cargo test -p brixo-runtime --release --test stress -- --nocapture

use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::Game;
use std::time::Instant;

#[test]
fn two_hundred_scripts_waiting_at_once() {
    let mut dm = DataModel::new();
    let root = dm.root();
    for i in 0..200 {
        let p = dm.create(Class::Part, &format!("P{i}"), root).unwrap();
        dm.part_mut(p).unwrap().position = Vec3::new((i % 20) as f32 * 3.0, 0.0, (i / 20) as f32 * 3.0);
        let s = dm.create(Class::Script, "S", p).unwrap();
        dm.script_mut(s).unwrap().source =
            "n = 0\nwhile true do\n wait(0.1)\n n += 1\n self.position.y = n\nend".to_string();
    }
    let started = Instant::now();
    let mut game = Game::start(dm);
    for _ in 0..60 {
        game.step(0.1);
    }
    assert_eq!(game.waiting_tasks(), 200);
    assert!(game.take_log().is_empty());
    eprintln!("200 scripts waking every frame, 60 frames: {:?}", started.elapsed());
}

#[test]
fn a_big_build_with_falling_crates() {
    // 400 anchored blocks (a floor of tiles and some walls) plus 150 loose
    // crates dropped onto them: a busy but realistic scene.
    let mut dm = DataModel::new();
    let root = dm.root();
    for x in 0..20 {
        for z in 0..20 {
            let t = dm.create(Class::Part, "Tile", root).unwrap();
            let p = dm.part_mut(t).unwrap();
            p.size = Vec3::new(4.0, 1.0, 4.0);
            p.position = Vec3::new(x as f32 * 4.0, -0.5, z as f32 * 4.0);
        }
    }
    for i in 0..150 {
        let c = dm.create(Class::Part, "Crate", root).unwrap();
        let p = dm.part_mut(c).unwrap();
        p.anchored = false;
        p.size = Vec3::new(2.0, 2.0, 2.0);
        p.position = Vec3::new(20.0 + (i % 10) as f32 * 2.5, 3.0 + (i / 10) as f32 * 2.5, 30.0);
    }
    let mut game = Game::start(dm);
    let started = Instant::now();
    let frames = 300;
    for _ in 0..frames {
        game.step(1.0 / 60.0);
    }
    let per_frame = started.elapsed() / frames;
    eprintln!("550 parts (150 falling), average frame: {per_frame:?}");
}
