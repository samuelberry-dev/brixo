//! On-screen GUI and tools, driven through a running game.

use brixo_core::{Class, DataModel, InstanceId, Vec3};
use brixo_runtime::Game;

const FRAME: f64 = 1.0 / 60.0;

fn arena() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let f = dm.create(Class::Part, "Floor", root).unwrap();
    let p = dm.part_mut(f).unwrap();
    p.size = Vec3::new(100.0, 1.0, 100.0);
    p.position = Vec3::new(0.0, -0.5, 0.0);
    dm
}

fn script(dm: &mut DataModel, parent: InstanceId, src: &str) {
    let s = dm.create(Class::Script, "Script", parent).unwrap();
    dm.script_mut(s).unwrap().source = src.to_string();
}

fn run(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds / FRAME).round() as usize {
        game.step(FRAME);
    }
}

fn texts(game: &Game) -> Vec<String> {
    game.take_log().into_iter().map(|l| l.text).collect()
}

#[test]
fn scripts_build_a_gui_for_each_player() {
    let mut dm = arena();
    let root = dm.root();
    script(&mut dm, root, r#"
on player_joined(p)
    label = create("TextLabel", p)
    label.text = "Coins: " + 0
    label.x = 0.02
    label.text_color = {r = 255, g = 220, b = 0}
    label.text_size = 28
end"#);
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    run(&mut game, 0.1);
    let world = game.world();
    let label = world.get(ann).unwrap().children[0];
    let g = world.gui(label).unwrap();
    assert_eq!(world.get(label).unwrap().class, Class::TextLabel);
    assert_eq!((g.text.as_str(), g.x, g.text_size), ("Coins: 0", 0.02, 28.0));
    assert_eq!((g.text_color.r, g.text_color.g, g.text_color.b), (255, 220, 0));
    assert!(brixo_runtime::gui_shown(&world, label));
}

#[test]
fn clicking_a_button_runs_its_script_for_that_player() {
    let mut dm = arena();
    let root = dm.root();
    let button = dm.create(Class::TextButton, "Buy", root).unwrap();
    script(&mut dm, button, "on clicked(p)\n print(p.name + \" clicked\")\n self.text = \"Thanks!\"\nend");
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    assert!(game.click(ann, button));
    run(&mut game, 0.05);
    assert_eq!(texts(&game), ["Ann clicked"]);
    assert_eq!(game.world().gui(button).unwrap().text, "Thanks!");
}

#[test]
fn you_cant_click_someone_elses_or_a_hidden_button() {
    let mut dm = with_personal_button();
    let root = dm.root();
    let hidden = dm.create(Class::TextButton, "Hidden", root).unwrap();
    dm.gui_mut(hidden).unwrap().visible = false;
    script(&mut dm, hidden, "on clicked(p)\n print(\"hidden clicked\")\nend");
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.1);
    let anns_button = game.world().get(ann).unwrap().children[0];
    assert!(!game.click(bob, anns_button), "Bob can't press Ann's button");
    assert!(game.click(ann, anns_button));
    assert!(!game.click(ann, hidden), "hidden buttons can't be clicked");
    run(&mut game, 0.05);
    assert_eq!(texts(&game), ["Ann pressed hers"]);
}

fn with_personal_button() -> DataModel {
    let mut dm = arena();
    let root = dm.root();
    // Scripts can't create scripts, so a personal button with behaviour is
    // made by cloning a hidden template that has its script inside.
    let template = dm.create(Class::TextButton, "Personal", root).unwrap();
    dm.gui_mut(template).unwrap().visible = false;
    script(&mut dm, template, "on clicked(p)\n print(p.name + \" pressed hers\")\nend");
    // Give each player a visible copy of the template.
    let giver = dm.create(Class::Folder, "Giver", root).unwrap();
    script(&mut dm, giver, "on player_joined(p)\n b = clone(find(\"Personal\"))\n b.visible = true\n b.parent = p\nend");
    dm
}

fn with_sword() -> DataModel {
    let mut dm = arena();
    let root = dm.root();
    let sword = dm.create(Class::Tool, "Sword", root).unwrap();
    let handle = dm.create(Class::Part, "Handle", sword).unwrap();
    {
        let p = dm.part_mut(handle).unwrap();
        p.position = Vec3::new(50.0, 1.0, 0.0);
        p.size = Vec3::new(0.4, 0.4, 1.0);
    }
    let blade = dm.create(Class::Part, "Blade", sword).unwrap();
    {
        let p = dm.part_mut(blade).unwrap();
        p.position = Vec3::new(50.0, 1.0, 2.5); // along +Z from the handle
        p.size = Vec3::new(0.3, 0.3, 4.0);
    }
    script(&mut dm, sword, "on activated(p)\n print(p.name + \" swung the \" + self.name)\nend");
    script(&mut dm, root, "on player_joined(p)\n t = clone(find(\"Sword\"))\n t.parent = p\nend");
    dm
}

#[test]
fn players_get_equip_and_use_tools() {
    let mut game = Game::start_server(with_sword());
    let ann = game.add_player("Ann");
    run(&mut game, 0.5);
    let tools = game.backpack(ann);
    assert_eq!(tools.len(), 1, "the sword was given");
    assert!(!game.activate(ann), "nothing in hand yet");

    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    assert_eq!(game.world().player(ann).unwrap().equipped, Some(tools[0]));
    // The handle is in Ann's hand, and the blade still sticks out ahead.
    let (me, handle, blade) = {
        let w = game.world();
        let parts = w.parts_under(tools[0]);
        (w.player(ann).unwrap().body.position, w.part(parts[0]).unwrap().position, w.part(parts[1]).unwrap().position)
    };
    let d = ((handle.x - me.x).powi(2) + (handle.z - me.z).powi(2)).sqrt();
    assert!(d < 2.6 && (handle.y - me.y).abs() < 1.5, "handle at {handle:?}, player at {me:?}");
    assert!((blade.z - handle.z - 2.5).abs() < 0.05, "blade kept its offset: {blade:?} vs {handle:?}");

    assert!(game.activate(ann));
    run(&mut game, 0.05);
    assert_eq!(texts(&game), ["Ann swung the Sword"]);

    // Pressing the same key again puts it away.
    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    assert_eq!(game.world().player(ann).unwrap().equipped, None);
    let handle = {
        let w = game.world();
        w.part(w.parts_under(tools[0])[0]).unwrap().position
    };
    assert!(handle.y < -100.0, "back in the backpack, out of sight");
    assert!(!game.activate(ann));
}

#[test]
fn one_player_cant_fire_anothers_tool() {
    let mut game = Game::start_server(with_sword());
    let ann = game.add_player("Ann");
    let bob = game.add_player("Bob");
    run(&mut game, 0.3);
    game.equip(ann, Some(0));
    assert!(!game.activate(bob), "Bob has nothing in hand");
    run(&mut game, 0.05);
    assert!(texts(&game).is_empty());
}

#[test]
fn create_explains_what_it_can_make() {
    let mut dm = arena();
    let root = dm.root();
    script(&mut dm, root, "create(\"Player\", self)");
    let game = Game::start_server(dm);
    let log = game.take_log();
    assert!(log[0].is_error && log[0].text.contains("can't create a 'Player'") && log[0].text.contains("TextButton"), "{:?}", log[0]);
}

#[test]
fn swords_are_held_straight_up_and_chop_forward() {
    let mut dm = with_sword();
    let sword = dm.find_first("Sword").unwrap();
    dm.get_mut(sword).unwrap().attributes.insert("grip".into(), brixo_core::Attribute::Str("up".into()));
    let mut game = Game::start_server(dm);
    let ann = game.add_player("Ann");
    run(&mut game, 0.5);
    let tool = game.backpack(ann)[0];
    let parts = game.world().parts_under(tool);
    let (handle, blade) = (parts[0], parts[1]);
    let at = |game: &Game| {
        let w = game.world();
        (w.player(ann).unwrap().body.position, w.part(handle).unwrap().position, w.part(blade).unwrap().position)
    };

    // Equip twice (put away in between): the tool keeps its shape.
    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    game.equip(ann, Some(0));
    run(&mut game, 0.1);
    let (me, h, b) = at(&game);
    // In the raised right hand: above the shoulder, blade straight up.
    assert!(h.y > me.y + 1.8, "hand raised: handle {h:?}, player {me:?}");
    assert!((b.y - h.y - 2.5).abs() < 0.05 && (b.x - h.x).abs() < 0.05 && (b.z - h.z).abs() < 0.05, "blade straight up: {b:?} vs {h:?}");

    // Mid-swing, the blade has come forward, away from the player.
    game.activate(ann);
    run(&mut game, brixo_core::SWING_TIME as f64 / 2.0);
    let (me, h, b) = at(&game);
    assert!(b.z - h.z > 1.5 && b.z > me.z + 2.0, "chopped forward: blade {b:?}, handle {h:?}");
    // And back up when it's done.
    run(&mut game, 0.3);
    let (_, h, b) = at(&game);
    assert!((b.y - h.y - 2.5).abs() < 0.05, "back up");
}
