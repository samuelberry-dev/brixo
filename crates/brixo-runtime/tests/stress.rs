use brixo_core::{Class, DataModel};
use brixo_runtime::Game;
use std::time::Instant;

#[test]
fn two_hundred_scripts_waiting_at_once() {
    let mut dm = DataModel::new();
    let root = dm.root();
    for i in 0..200 {
        let p = dm.create(Class::Part, &format!("P{i}"), root).unwrap();
        let s = dm.create(Class::Script, "S", p).unwrap();
        dm.script_mut(s).unwrap().source =
            "n = 0\nwhile true do\n wait(0.1)\n n += 1\n self.position.y = n\nend".to_string();
    }
    let started = Instant::now();
    let mut game = Game::start(dm);
    for _ in 0..60 {
        game.step(1.0 / 60.0 * 6.0);
    }
    let elapsed = started.elapsed();
    assert_eq!(game.waiting_tasks(), 200);
    assert!(game.take_log().is_empty());
    eprintln!("200 scripts x 60 frames: {elapsed:?}");
    drop(game);
}
