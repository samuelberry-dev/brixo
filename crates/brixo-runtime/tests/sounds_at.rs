//! Sounds from places in the world: play_sound_at, explosions, dying.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::{Cue, Game, SoundEvent};

const FRAME: f64 = 1.0 / 60.0;

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn script(dm: &mut DataModel, parent: InstanceId, src: &str) {
    let s = dm.create(Class::Script, "S", parent).unwrap();
    dm.script_mut(s).unwrap().source = src.to_string();
}

fn errors(game: &Game) -> Vec<String> {
    game.take_log().into_iter().filter(|l| l.is_error).map(|l| l.text).collect()
}

fn world() -> (DataModel, InstanceId) {
    let mut dm = DataModel::new();
    let root = dm.root();
    let floor = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(floor).unwrap();
    p.size = Vec3::new(200.0, 1.0, 200.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    let bell = dm.create(Class::Part, "Bell", root).unwrap();
    dm.part_mut(bell).unwrap().position = Vec3::new(30.0, 5.0, -10.0);
    (dm, bell)
}

#[test]
fn a_sound_can_come_from_a_part_or_a_spot() {
    let (mut dm, bell) = world();
    let root = dm.root();
    script(&mut dm, root, "wait(0.1)\nplay_sound_at(\"hit\", find(\"Bell\"))\nplay_sound_at(\"boom\", {x = 1, y = 2, z = 3})\nplay_sound(\"win\")");
    let mut game = Game::start(dm);
    run(&mut game, 0.3);
    assert!(errors(&game).is_empty());
    let sounds = game.take_sounds();
    let at = |n: &str| sounds.iter().find_map(|e| match e {
        SoundEvent::Play { name, at, .. } if name == n => Some(*at),
        _ => None,
    });
    let hit = at("hit").unwrap().unwrap();
    assert_eq!((hit.object, hit.position), (Some(bell), Vec3::new(30.0, 5.0, -10.0)), "follows the bell");
    let boom = at("boom").unwrap().unwrap();
    assert_eq!((boom.object, boom.position), (None, Vec3::new(1.0, 2.0, 3.0)));
    assert_eq!(at("win").unwrap(), None, "play_sound is still heard the same everywhere");
    // Everyone hears a placed sound, placed.
    let me = game.player_id();
    let cue = SoundEvent::Play { name: "hit".into(), player: None, at: Some(hit) }.for_player(me);
    assert_eq!(cue, Some(Cue::SoundAt("hit".into(), hit)));
    // The sound follows the part while it plays.
    game.world().part_mut(bell).unwrap().position = Vec3::new(0.0, 5.0, 0.0);
    assert_eq!(hit.now(&game.world()), Vec3::new(0.0, 5.0, 0.0));
}

#[test]
fn explosions_and_dying_are_heard_from_where_they_happen() {
    let (mut dm, _) = world();
    let root = dm.root();
    script(&mut dm, root, "wait(0.1)\nexplode({x = 50, y = 1, z = 50}, 4)");
    let mut game = Game::start(dm);
    run(&mut game, 0.3);
    let boom = game.take_sounds().into_iter().find_map(|e| match e {
        SoundEvent::Play { name, at, .. } if name == "boom" => at,
        _ => None,
    });
    assert_eq!(boom.map(|a| a.position), Some(Vec3::new(50.0, 1.0, 50.0)));
    let me = game.player_id().unwrap();
    game.world().player_mut(me).unwrap().health = 0.0;
    run(&mut game, 0.1);
    let death = game.take_sounds().into_iter().find_map(|e| match e {
        SoundEvent::Play { name, at, .. } if name == "death" => at,
        _ => None,
    });
    assert_eq!(death.and_then(|a| a.object), Some(me));
}

#[test]
fn a_part_isnt_a_listener() {
    // play_sound(sound, player) plays it for just that player; a part there
    // is a mistake, and the error says what to use.
    let (mut dm, _) = world();
    let root = dm.root();
    script(&mut dm, root, "play_sound(\"hit\", find(\"Bell\"))");
    let game = Game::start(dm);
    let e = errors(&game);
    assert!(e.iter().any(|l| l.contains("play_sound_at")), "{e:?}");
}
