//! Watch a small game run in the terminal:
//!     cargo run -p brixo-runtime --example demo
//!
//! A block slides along, a coin waits to be touched, and a clock ticks.

use std::time::{Duration, Instant};

use brixo_core::{Class, DataModel, Vec3};
use brixo_runtime::Game;

fn main() {
    let mut dm = DataModel::new();
    let root = dm.root();

    let coin = dm.create(Class::Part, "Coin", root).unwrap();
    dm.part_mut(coin).unwrap().position = Vec3::new(6.0, 0.0, 0.0);
    let s = dm.create(Class::Script, "Pickup", coin).unwrap();
    dm.script_mut(s).unwrap().source = r#"
on touched(other)
    print(other.name + " grabbed the coin at " + round(time()) + "s!")
    destroy(self)
end
"#
    .to_string();

    let mover = dm.create(Class::Part, "Slider", root).unwrap();
    let s = dm.create(Class::Script, "Move", mover).unwrap();
    dm.script_mut(s).unwrap().source = r#"
print("Slider starting at " + self.position)
while self.position.x < 10 do
    wait(0.5)
    self.position.x += 1
    print("Slider at x = " + self.position.x)
end
print("Slider reached the end")
"#
    .to_string();

    let clock = dm.create(Class::Folder, "Clock", root).unwrap();
    let s = dm.create(Class::Script, "Tick", clock).unwrap();
    dm.script_mut(s).unwrap().source = r#"
seconds = 0
every 2 seconds
    seconds += 2
    print("-- " + seconds + " seconds --")
end
"#
    .to_string();

    let mut game = Game::start(dm);
    let started = Instant::now();
    let mut last = started;
    while started.elapsed() < Duration::from_secs(7) {
        std::thread::sleep(Duration::from_millis(16));
        let now = Instant::now();
        game.step((now - last).as_secs_f64());
        last = now;
        for line in game.take_log() {
            let mark = if line.is_error { "ERROR " } else { "" };
            println!("[{}] {mark}{}", line.source, line.text);
        }
    }
    println!("(stopped after 7 seconds)");
}
