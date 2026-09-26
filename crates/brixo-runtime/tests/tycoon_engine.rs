//! The engine pieces a tycoon needs: conveyors, velocity, custom fields,
//! materials, transparency, floating labels and sounds.

use brixo_core::{Class, DataModel, InstanceId, Material, Vec3};
use brixo_runtime::{Game, SoundEvent};

const FRAME: f64 = 1.0 / 60.0;

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(100.0, 1.0, 100.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let s = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
    dm.part_mut(s).unwrap().position = Vec3::new(40.0, 0.5, 40.0);
    dm
}

fn part(dm: &mut DataModel, pos: Vec3, size: Vec3, anchored: bool) -> InstanceId {
    let root = dm.root();
    let id = dm.create(Class::Part, "P", root).unwrap();
    let p = dm.part_mut(id).unwrap();
    p.position = pos;
    p.size = size;
    p.anchored = anchored;
    id
}

fn script(dm: &mut DataModel, parent: InstanceId, src: &str) {
    let s = dm.create(Class::Script, "S", parent).unwrap();
    dm.script_mut(s).unwrap().source = src.to_string();
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

#[test]
fn conveyors_carry_loose_parts_along() {
    let mut dm = arena();
    let belt = part(&mut dm, Vec3::new(0.0, 0.5, 0.0), Vec3::new(4.0, 1.0, 30.0), true);
    dm.part_mut(belt).unwrap().velocity = Vec3::new(0.0, 0.0, 6.0);
    let ore = part(&mut dm, Vec3::new(0.0, 2.0, -12.0), Vec3::new(1.0, 1.0, 1.0), false);
    let mut game = Game::start(dm);
    run(&mut game, 2.0);
    let z = game.world().part(ore).unwrap().position.z;
    assert!(z > -3.0, "carried along the belt: z = {z}");
}

#[test]
fn scripts_launch_parts_by_setting_velocity() {
    let mut dm = arena();
    let ball = part(&mut dm, Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 1.0), false);
    script(&mut dm, ball, "wait(0.5)\nself.velocity = {x = 0, y = 60, z = 0}");
    let mut game = Game::start(dm);
    run(&mut game, 0.8);
    let p = *game.world().part(ball).unwrap();
    assert!(p.position.y > 8.0 && p.velocity.y > 5.0, "flying up: {p:?}");
}

#[test]
fn custom_fields_hold_money_and_typos_are_caught() {
    let mut dm = arena();
    let root = dm.root();
    script(&mut dm, root, r#"
on player_joined(p)
    print(p.cash == nil)
    p.cash = 100
    p.cash = p.cash + 25
    p.plot = "north"
    print(p.cash + " " + p.plot)
    p.jumppower = 50
end"#);
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    run(&mut game, 0.1);
    let log = game.take_log();
    assert_eq!(log[0].text, "true");
    assert_eq!(log[1].text, "125 north");
    assert!(log[2].is_error && log[2].text.contains("Did you mean 'jump_power'?"), "{:?}", log[2]);
    let w = game.world();
    assert_eq!(w.get(ann).unwrap().attributes["cash"], brixo_core::Attribute::Num(125.0));
}

#[test]
fn materials_transparency_labels_and_sounds_from_scripts() {
    let mut dm = arena();
    let pad = part(&mut dm, Vec3::new(0.0, 0.5, 0.0), Vec3::new(4.0, 1.0, 4.0), true);
    script(&mut dm, pad, r#"
self.material = "neon"
self.transparency = 0.5
tag = create("TextLabel", self.parent)
tag.text = "$50"
tag.attached_to = self
play_sound("buy")
play_music("sunny")
play_sound("kaboom")"#);
    let game = Game::start(dm);
    let log = game.take_log();
    assert!(log.last().unwrap().text.contains("no sound called 'kaboom'"), "{log:?}");
    let w = game.world();
    let p = w.part(pad).unwrap();
    assert_eq!((p.material, p.transparency), (Material::Neon, 0.5));
    let tag = w.find_first("TextLabel").unwrap();
    assert_eq!(w.gui(tag).unwrap().attached_to, Some(pad));
    drop(w);
    assert_eq!(
        game.take_sounds(),
        [
            SoundEvent::Play { name: "buy".into(), player: None },
            SoundEvent::Music { name: Some("sunny".into()), player: None },
        ]
    );
}

#[test]
fn scripts_play_the_games_own_sounds() {
    let mut dm = arena();
    let root = dm.root();
    let horn = dm.create(Class::Sound, "Horn", root).unwrap();
    *dm.sound_mut(horn).unwrap() = brixo_core::SoundProps::from_bytes("wav", b"not really audio, but data");
    script(&mut dm, root, "play_sound(find(\"Horn\"))\nfind(\"Horn\").volume = 0.25\nplay_music(find(\"Horn\"))\nplay_sound(find(\"Floor\"))");
    let game = Game::start(dm);
    let log = game.take_log();
    assert!(log.last().unwrap().is_error, "a part isn't a sound: {log:?}");
    let sounds = game.take_sounds();
    let id = format!("#{}", horn.raw());
    assert!(sounds.iter().any(|s| matches!(s, SoundEvent::Play { name, .. } if *name == id)), "{sounds:?}");
    assert!(sounds.iter().any(|s| matches!(s, SoundEvent::Music { name: Some(n), .. } if *n == id)));
    assert_eq!(game.world().sound(horn).unwrap().volume, 0.25);
}
