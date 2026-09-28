//! The Test Lab: every Brixo feature in one place, for trying things out
//! (admins only on the website). Around a plaza:
//!
//! - **North, doors and hinges:** a hinged door that opens away from you,
//!   saloon doors you push, a drawbridge on a lever, a trapdoor, a
//!   swinging sign and the old sliding door on a pressure plate.
//! - **East, the garage:** two cars with hinged, motorised wheels you drive
//!   with buttons floating over them, and a ramp.
//! - **West, the playground:** a spinner, a windmill, a merry-go-round and
//!   a swing.
//! - **South, the gear range:** everyone has the whole gear kit; a
//!   breakable tower with Explode and Rebuild buttons, and targets.
//! - **North-east, movement:** jump, launch and speed pads, teleporters, a
//!   conveyor, a moving platform, lava and a ball pit.
//! - **The plaza:** coins, team pads, and a sign.
//! - **On screen:** the leaderboard (coins, KOs, and visits, which are
//!   saved), and a **Lab controls** panel: time of day, fog, brightness,
//!   sky colour, a day cycle, and music.

use brixo_core::{Attribute, Class, Color, DataModel, Hinge, InstanceId, Material, Shape, Side, Vec3};

type Rgb = (u8, u8, u8);

struct B {
    dm: DataModel,
}

impl B {
    fn root(&self) -> InstanceId {
        self.dm.root()
    }

    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, parent: InstanceId, name: &str, at: (f32, f32, f32), size: (f32, f32, f32), color: Rgb, material: Material) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = Vec3::new(at.0, at.1, at.2);
        p.size = Vec3::new(size.0, size.1, size.2);
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        id
    }

    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }

    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.into(), value);
    }

    fn hinge(&mut self, id: InstanceId, hinge: Hinge, at: Side, motor: f32, swing_to: Option<f32>) {
        let p = self.dm.part_mut(id).unwrap();
        p.anchored = false;
        p.hinge = hinge;
        p.hinge_at = at;
        p.motor_speed = motor;
        p.swing_to = swing_to;
    }

    /// A label floating over a part.
    fn sign(&mut self, over: InstanceId, text: &str) {
        let parent = self.root();
        let id = self.dm.create(Class::TextLabel, &format!("{text} Sign"), parent).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.into();
        g.attached_to = Some(over);
        g.x = 0.0;
        g.y = 0.0;
        // Wide enough for its words.
        g.width = text.chars().count() as f32 * 0.0082 + 0.025;
        g.height = 0.045;
        g.text_size = 17.0;
        g.background = true;
        g.background_color = Color::new(13, 42, 74);
        g.text_color = Color::new(245, 205, 48);
    }

    /// A button, on screen (`attached` None) or floating over a part, with
    /// its `on clicked` script.
    #[allow(clippy::too_many_arguments)]
    fn button(&mut self, parent: InstanceId, text: &str, attached: Option<InstanceId>, at: (f32, f32), size: (f32, f32), color: Rgb, script: &str) -> InstanceId {
        let id = self.dm.create(Class::TextButton, text, parent).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.into();
        g.attached_to = attached;
        g.x = at.0;
        g.y = at.1;
        g.width = size.0;
        g.height = size.1;
        g.text_size = 14.0;
        g.background_color = Color::new(color.0, color.1, color.2);
        self.script(id, "Click", script);
        id
    }

    fn label(&mut self, parent: InstanceId, text: &str, at: (f32, f32), size: (f32, f32)) {
        let id = self.dm.create(Class::TextLabel, text, parent).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.into();
        g.x = at.0;
        g.y = at.1;
        g.width = size.0;
        g.height = size.1;
        g.text_size = 15.0;
        g.text_color = Color::new(245, 205, 48);
    }
}

const GRASS: Rgb = (75, 151, 75);
const STONE: Rgb = (163, 162, 165);
const WOOD: Rgb = (124, 86, 52);
const DARK: Rgb = (40, 44, 52);

/// The players' side of things: stats, the leaderboard, gear and controls
/// for everyone who joins, KOs, and the day cycle.
const LAB: &str = r#"
-- The Test Lab: stats, gear and controls for everyone.
leaderboard("coins", "kos", "visits")
world = find("Workspace")

on player_joined(p)
    p.coins = 0
    p.kos = 0
    -- Visits are saved between games.
    p.visits = (load(p, "visits") or 0) + 1
    save(p, "visits", p.visits)
    -- The whole gear kit.
    for name in ["Sword", "Slingshot", "Rocket Launcher", "Superball", "Trowel", "Paintball Gun"] do
        t = clone(find(name + " Template"))
        t.name = name
        t.parent = p
    end
    -- The Lab controls button, and its panel (folded away to start).
    toggle = clone(find("Lab Toggle Template"))
    toggle.name = "Lab Toggle"
    toggle.parent = p
    toggle.visible = true
    panel = clone(find("Lab Panel Template"))
    panel.name = "Lab Panel"
    panel.parent = p
end

-- A KO counts for whoever landed the last hit.
on died(p)
    if p.last_hit_by != nil then
        for other in players() do
            if other.name == p.last_hit_by and other.name != p.name then
                other.kos += 1
            end
        end
    end
    p.last_hit_by = nil
end

-- The day cycle (the panel's Day cycle button turns it on and off).
every 0.25 seconds
    if world.cycling == true then
        world.time_of_day = (world.time_of_day + 0.05) % 24
    end
end
"#;

const COIN: &str = r#"
-- A coin: +1, then it comes back after 5 seconds.
gone = false

on touched(other)
    if other.class == "player" and not gone and other.coins != nil then
        gone = true
        other.coins += 1
        play_sound("coin", other)
        self.transparency = 1
        wait(5)
        self.transparency = 0
        gone = false
    end
end

every 0.03 seconds
    self.rotation.y += 4
end
"#;

const TEAM_PAD: &str = r#"
-- Stand on it to join its team (the leaderboard groups teams; teammates'
-- gear doesn't hurt each other).
on touched(other)
    if other.class == "player" and other.team != self.team_name then
        other.team = self.team_name
        play_sound("click", other)
    end
end
"#;

const NO_TEAM_PAD: &str = r#"
on touched(other)
    if other.class == "player" and other.team != nil then
        other.team = nil
        play_sound("click", other)
    end
end
"#;

// --- doors ---------------------------------------------------------------

const SWING_DOOR: &str = r#"
-- Opens away from whoever walks into it, and closes 3 seconds after.
open_at = 0

on touched(other)
    if other.class == "player" then
        if other.position.z < self.position.z then
            self.swing_to = -100
        else
            self.swing_to = 100
        end
        open_at = time()
    end
end

every 0.5 seconds
    if self.swing_to != 0 and time() - open_at > 3 then
        self.swing_to = 0
    end
end
"#;

const LEVER: &str = r#"
-- Raises and lowers the drawbridge.
on clicked(p)
    bridge = find("Drawbridge")
    if bridge.swing_to == 0 then
        bridge.swing_to = -80
        self.text = "Lower bridge"
    else
        bridge.swing_to = 0
        self.text = "Raise bridge"
    end
    play_sound("click")
end
"#;

const TRAPDOOR: &str = r#"
-- Walk onto it: a moment later it drops open, then shuts again.
busy = false

on touched(other)
    if other.class == "player" and not busy then
        busy = true
        wait(0.4)
        self.swing_to = 85
        play_sound_at("thud", self)
        wait(3)
        self.swing_to = 0
        busy = false
    end
end
"#;

const WATER: &str = r#"
-- Fell in? Back to the start of the ramp.
on touched(other)
    if other.class == "player" then
        other.position = {x = -10, y = 3, z = 44}
        play_sound("splat", other)
    end
end
"#;

const SLIDE_PLATE: &str = r#"
-- The old way: a script slides an anchored door up and down.
door = find("Sliding Door")
closed_y = door.position.y
busy = false

on touched(other)
    if other.class != "player" or busy then
        return
    end
    busy = true
    self.color = {r = 60, g = 200, b = 80}
    play_sound_at("click", self)
    for i in 1..20 do
        door.position.y = closed_y + i * 0.4
        wait(0.03)
    end
    wait(3)
    for i in 1..20 do
        door.position.y = closed_y + (20 - i) * 0.4
        wait(0.03)
    end
    self.color = {r = 200, g = 60, b = 60}
    busy = false
end
"#;

// --- cars ----------------------------------------------------------------

/// A car's driving buttons: each sets the car's wheels' motors. The wheels
/// all turn around the car's width, so the same speed drives straight and
/// opposite speeds on each side turn it on the spot. A negative speed
/// rolls it forward (+Z).
fn drive(left: f32, right: f32, word: &str) -> String {
    format!(
        r#"
-- {word}: sets this car's wheel motors.
on clicked(p)
    for part in self.parent.children do
        if part.name == "Wheel L" then
            part.swing_to = nil
            part.motor_speed = {left}
        elseif part.name == "Wheel R" then
            part.swing_to = nil
            part.motor_speed = {right}
        end
    end
    play_sound("click", p)
end
"#
    )
}

/// A car's buttons only show while someone's near it, so the map isn't
/// covered in them.
const CAR_BUTTONS: &str = r#"
-- Show this car's buttons only while someone's close to it.
body = nil
for part in self.children do
    if part.name == "Body" then
        body = part
    end
end

every 0.3 seconds
    near = false
    for p in players() do
        dx = p.position.x - body.position.x
        dz = p.position.z - body.position.z
        if dx * dx + dz * dz < 25 * 25 then
            near = true
        end
    end
    for c in self.children do
        if c.class == "textbutton" then
            c.visible = near
        end
    end
end
"#;

/// Stop: the brakes. Each wheel holds still where it is.
const BRAKE: &str = r#"
-- Stop: brakes on (each wheel holds its angle).
on clicked(p)
    for part in self.parent.children do
        if part.name == "Wheel L" or part.name == "Wheel R" then
            part.motor_speed = 0
            part.swing_to = part.hinge_angle
        end
    end
    play_sound("click", p)
end
"#;

// --- the gear range --------------------------------------------------------

const EXPLODE: &str = r#"
on clicked(p)
    tower = find("Tower")
    if tower != nil then
        explode({x = tower.children[1].position.x, y = 3, z = tower.children[1].position.z}, 9)
    end
end
"#;

const REBUILD: &str = r#"
-- Clears what's left of the tower and builds a fresh one from Storage.
on clicked(p)
    old = find("Tower")
    while old != nil do
        destroy(old)
        old = find("Tower")
    end
    tower = clone(find("Tower Template"))
    tower.name = "Tower"
    tower.parent = find("Workspace")
    for brick in tower.children do
        brick.position.y += 300
    end
    play_sound("pop")
end
"#;

const TARGET: &str = r#"
-- A target: flashes and dings when something hits it.
on touched(other)
    if other.class != "player" then
        self.color = {r = 255, g = 255, b = 255}
        play_sound_at("hit", self)
        wait(0.2)
        self.color = {r = 196, g = 40, b = 28}
    end
end
"#;

// --- movement --------------------------------------------------------------

const JUMP_PAD: &str = r#"
on touched(other)
    if other.class == "player" then
        other.velocity = {x = 0, y = 80, z = 0}
        play_sound_at("jump", self)
    end
end
"#;

const LAUNCH_PAD: &str = r#"
on touched(other)
    if other.class == "player" then
        other.velocity = {x = 0, y = 60, z = 70}
        play_sound_at("whoosh", self)
    end
end
"#;

const SPEED_PAD: &str = r#"
on touched(other)
    if other.class == "player" and other.boosted != true then
        other.boosted = true
        other.walk_speed = 34
        play_sound("pop", other)
        wait(3)
        other.walk_speed = 16
        other.boosted = false
    end
end
"#;

const TELEPORTER: &str = r#"
on touched(other)
    if other.class != "player" then
        return
    end
    if other.teleported_at != nil and time() - other.teleported_at < 2 then
        return
    end
    target = find(self.destination)
    other.teleported_at = time()
    other.position = {x = target.position.x, y = target.position.y + 4, z = target.position.z}
    play_sound("whoosh", other)
end
"#;

const MOVING_PLATFORM: &str = r#"
start = self.position.x
every 0.03 seconds
    self.position.x = start + sin(time()) * 10
end
"#;

const LAVA: &str = r#"
on touched(other)
    if other.class == "player" then
        other.health = 0
    end
end
"#;

// --- the Lab controls panel ------------------------------------------------

const TOGGLE: &str = r#"
-- Shows and hides this player's Lab controls panel.
on clicked(p)
    for c in p.children do
        if c.name == "Lab Panel" then
            c.visible = not c.visible
        end
    end
end
"#;

fn set_world(what: &str) -> String {
    format!(
        r#"
on clicked(p)
    w = find("Workspace")
    {what}
    play_sound("click", p)
end
"#
    )
}

/// The Test Lab.
pub fn test_lab() -> DataModel {
    let mut b = B { dm: DataModel::new() };
    let root = b.root();
    use Material::*;

    // --- ground, plaza, spawn ---
    b.part(root, "Ground", (0.0, -1.0, 0.0), (320.0, 2.0, 320.0), GRASS, Grass);
    b.part(root, "Plaza", (0.0, 0.1, 0.0), (44.0, 0.2, 44.0), (170, 166, 160), Concrete);
    let spawn = b.dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    {
        let p = b.dm.part_mut(spawn).unwrap();
        p.position = Vec3::new(0.0, 0.6, -6.0);
        p.size = Vec3::new(8.0, 0.8, 8.0);
        p.color = Color::new(245, 205, 48);
        p.material = Metal;
    }
    let welcome = b.part(root, "Welcome Post", (0.0, 3.0, 4.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(welcome, "Brixo Test Lab");
    b.script(root, "Lab", LAB);
    let storage = b.dm.create(Class::Folder, "Storage", root).unwrap();
    crate::gears::install(&mut b.dm, storage);
    b.dm.create(Class::Folder, "Projectiles", root).unwrap();

    // Coins in a ring round the plaza.
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let c = b.part(root, "Coin", (a.cos() * 16.0, 2.5, a.sin() * 16.0 - 2.0), (2.0, 0.4, 2.0), (245, 205, 48), Neon);
        let p = b.dm.part_mut(c).unwrap();
        p.shape = Shape::Cylinder;
        p.rotation = Vec3::new(90.0, 0.0, 0.0);
        p.can_collide = false;
        b.script(c, "Coin", COIN);
    }
    // Team pads.
    for (i, (team, color)) in [("Red", (196, 40, 28)), ("Blue", (13, 105, 172))].into_iter().enumerate() {
        let pad = b.part(root, &format!("{team} Team Pad"), (-15.0 + i as f32 * 6.0, 0.3, 14.0), (4.0, 0.4, 4.0), color, Neon);
        b.attr(pad, "team_name", Attribute::Str(team.into()));
        b.script(pad, "Join", TEAM_PAD);
    }
    let none = b.part(root, "No Team Pad", (-3.0, 0.3, 14.0), (4.0, 0.4, 4.0), (200, 200, 200), Plastic);
    b.script(none, "Leave", NO_TEAM_PAD);
    let teams_post = b.part(root, "Teams Post", (-9.0, 2.0, 17.5), (0.6, 4.0, 0.6), WOOD, Wood);
    b.sign(teams_post, "Team pads");

    // --- north: doors and hinges ---
    let doors_post = b.part(root, "Doors Post", (0.0, 3.0, 36.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(doors_post, "Doors and hinges");
    // A hinged door in a frame that opens away from you.
    b.part(root, "Frame L", (-13.0, 4.0, 45.0), (1.0, 8.0, 1.0), WOOD, Wood);
    b.part(root, "Frame R", (-7.0, 4.0, 45.0), (1.0, 8.0, 1.0), WOOD, Wood);
    b.part(root, "Frame Top", (-10.0, 8.5, 45.0), (7.0, 1.0, 1.0), WOOD, Wood);
    let door = b.part(root, "Swing Door", (-10.0, 4.2, 45.0), (5.0, 7.0, 0.4), (200, 140, 70), Wood);
    b.hinge(door, Hinge::Y, Side::Left, 0.0, None);
    b.script(door, "Open", SWING_DOOR);
    // Saloon doors: two halves on free hinges, pushed open by walking through.
    b.part(root, "Saloon L", (4.0, 4.0, 45.0), (1.0, 8.0, 1.0), WOOD, Wood);
    b.part(root, "Saloon R", (12.0, 4.0, 45.0), (1.0, 8.0, 1.0), WOOD, Wood);
    for (x, side) in [(6.25, Side::Left), (9.75, Side::Right)] {
        let half = b.part(root, "Saloon Door", (x, 4.0, 45.0), (3.5, 3.5, 0.3), (160, 110, 60), Wood);
        b.hinge(half, Hinge::Y, side, 0.0, None);
    }
    // A drawbridge on a lever, between two high banks over water.
    let ramp_up = b.part(root, "Bank Ramp", (-10.0, 1.5, 50.0), (14.0, 3.0, 8.0), STONE, Concrete);
    b.dm.part_mut(ramp_up).unwrap().shape = Shape::Wedge;
    b.part(root, "Bank Near", (-10.0, 1.5, 55.5), (14.0, 3.0, 3.0), STONE, Concrete);
    // The far bank is half a stud lower: the lowered bridge rests on it.
    b.part(root, "Bank Far", (-10.0, 1.25, 68.75), (14.0, 2.5, 3.5), STONE, Concrete);
    let water = b.part(root, "Water", (-10.0, 0.15, 62.0), (14.0, 0.3, 10.0), (40, 110, 200), Neon);
    b.script(water, "Splash", WATER);
    let bridge = b.part(root, "Drawbridge", (-10.0, 2.75, 62.5), (6.0, 0.5, 11.0), WOOD, Wood);
    b.hinge(bridge, Hinge::X, Side::Back, 0.0, Some(0.0));
    let lever = b.part(root, "Lever Post", (-15.5, 4.5, 55.5), (0.6, 3.0, 0.6), DARK, Metal);
    b.button(root, "Raise bridge", Some(lever), (0.0, 0.0), (0.09, 0.04), (13, 105, 172), LEVER);
    // A trapdoor in a raised stage: walk onto it and it drops open.
    let stage_ramp = b.part(root, "Stage Ramp", (10.0, 1.5, 51.0), (6.0, 3.0, 8.0), STONE, Concrete);
    b.dm.part_mut(stage_ramp).unwrap().shape = Shape::Wedge;
    for (at, size) in [((10.0, 1.5, 57.0), (14.0, 3.0, 4.0)), ((10.0, 1.5, 67.0), (14.0, 3.0, 4.0)), ((5.0, 1.5, 62.0), (4.0, 3.0, 6.0)), ((15.0, 1.5, 62.0), (4.0, 3.0, 6.0))] {
        b.part(root, "Stage", at, size, (150, 140, 130), Brick);
    }
    let trap = b.part(root, "Trapdoor", (10.0, 2.75, 62.0), (6.0, 0.5, 6.0), (110, 80, 50), Wood);
    b.hinge(trap, Hinge::X, Side::Back, 0.0, Some(0.0));
    b.script(trap, "Spring", TRAPDOOR);
    // A sign swinging on a bar.
    b.part(root, "Sign Bar", (22.0, 7.25, 50.0), (4.0, 0.5, 0.5), DARK, Metal);
    b.part(root, "Sign Pole", (24.25, 3.75, 50.0), (0.5, 7.5, 0.5), DARK, Metal);
    let hanging = b.part(root, "Hanging Sign", (22.0, 5.0, 50.0), (3.0, 4.0, 0.2), (245, 205, 48), Wood);
    b.hinge(hanging, Hinge::X, Side::Top, 0.0, None);
    // The old sliding door, on a pressure plate.
    b.part(root, "Slide Frame L", (29.0, 4.0, 62.0), (1.0, 8.0, 1.0), STONE, Brick);
    b.part(root, "Slide Frame R", (35.0, 4.0, 62.0), (1.0, 8.0, 1.0), STONE, Brick);
    b.part(root, "Sliding Door", (32.0, 3.5, 62.0), (5.0, 7.0, 0.5), (90, 90, 100), Metal);
    let slide_plate = b.part(root, "Door Plate", (32.0, 0.3, 58.0), (4.0, 0.4, 4.0), (200, 60, 60), Neon);
    b.script(slide_plate, "Open", SLIDE_PLATE);

    // --- east: the garage ---
    b.part(root, "Garage Floor", (80.0, 0.1, 0.0), (60.0, 0.2, 70.0), (60, 62, 66), Concrete);
    let garage_post = b.part(root, "Garage Post", (48.0, 3.0, 0.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(garage_post, "Garage: drive the cars");
    let ramp = b.part(root, "Ramp", (80.0, 2.0, 26.0), (12.0, 4.0, 12.0), STONE, Concrete);
    b.dm.part_mut(ramp).unwrap().shape = Shape::Wedge;
    for (n, (x, color)) in [(70.0, (196, 40, 28)), (90.0, (13, 105, 172))].into_iter().enumerate() {
        let car = b.dm.create(Class::Model, &format!("Car {}", n + 1), root).unwrap();
        let body = b.part(car, "Body", (x, 2.2, -10.0), (4.0, 1.0, 8.0), color, Plastic);
        b.dm.part_mut(body).unwrap().anchored = false;
        let hood = b.part(car, "Seat", (x, 2.95, -11.0), (3.0, 0.5, 3.0), DARK, Plastic);
        b.dm.part_mut(hood).unwrap().anchored = false;
        for (dx, dz) in [(-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5), (2.5, 2.5)] {
            // The car's left is +X (it faces +Z).
            let name = if dx > 0.0 { "Wheel L" } else { "Wheel R" };
            let w = b.part(car, name, (x + dx, 1.7, -10.0 + dz), (3.0, 1.0, 3.0), (25, 25, 28), Plastic);
            let p = b.dm.part_mut(w).unwrap();
            p.shape = Shape::Cylinder;
            p.rotation = Vec3::new(0.0, 0.0, 90.0);
            b.hinge(w, Hinge::Y, Side::Middle, 0.0, None);
        }
        const S: f32 = 360.0;
        const TURN: f32 = 720.0;
        let blue = (13, 105, 172);
        b.button(car, "Forward", Some(body), (0.0, -0.05), (0.07, 0.04), (40, 127, 71), &drive(-S, -S, "Forward"));
        b.button(car, "Left", Some(body), (-0.075, 0.0), (0.07, 0.04), blue, &drive(TURN, -TURN, "Turn left"));
        b.button(car, "Stop", Some(body), (0.0, 0.0), (0.07, 0.04), (196, 40, 28), BRAKE);
        b.button(car, "Right", Some(body), (0.075, 0.0), (0.07, 0.04), blue, &drive(-TURN, TURN, "Turn right"));
        b.button(car, "Back", Some(body), (0.0, 0.05), (0.07, 0.04), blue, &drive(S, S, "Back"));
        b.script(car, "Buttons", CAR_BUTTONS);
    }

    // --- west: the playground ---
    let play_post = b.part(root, "Playground Post", (-48.0, 3.0, 0.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(play_post, "Playground: motors");
    // A spinner to jump over.
    b.part(root, "Spinner Hub", (-65.0, 1.0, -15.0), (1.5, 2.0, 1.5), DARK, Metal);
    let spinner = b.part(root, "Spinner", (-65.0, 2.5, -15.0), (16.0, 1.0, 1.0), (196, 40, 28), Neon);
    b.hinge(spinner, Hinge::Y, Side::Bottom, 90.0, None);
    // A merry-go-round to ride.
    b.part(root, "Carousel Hub", (-65.0, 0.5, 12.0), (2.0, 1.0, 2.0), DARK, Metal);
    let disc = b.part(root, "Carousel", (-65.0, 1.25, 12.0), (14.0, 0.5, 14.0), (245, 205, 48), Plastic);
    b.dm.part_mut(disc).unwrap().shape = Shape::Cylinder;
    b.hinge(disc, Hinge::Y, Side::Bottom, 30.0, None);
    // A windmill: a Model whose first part (the hub) is hinged, so the
    // sails welded to it turn with it.
    b.part(root, "Windmill Tower", (-88.0, 8.0, 0.0), (4.0, 16.0, 4.0), (230, 225, 210), Brick);
    let mill = b.dm.create(Class::Model, "Sails", root).unwrap();
    let hub = b.part(mill, "Sail Hub", (-88.0, 13.0, -2.5), (1.5, 1.5, 1.0), DARK, Wood);
    b.hinge(hub, Hinge::Z, Side::Front, 45.0, None);
    for (w, h) in [(14.0, 1.2), (1.2, 14.0)] {
        let sail = b.part(mill, "Sail", (-88.0, 13.0, -3.1), (w, h, 0.2), (240, 240, 240), Wood);
        b.dm.part_mut(sail).unwrap().anchored = false;
    }
    // A swing on a frame (push it).
    b.part(root, "Swing Bar", (-70.0, 9.25, 32.0), (6.0, 0.5, 0.5), WOOD, Wood);
    b.part(root, "Swing Leg L", (-73.25, 4.75, 32.0), (0.5, 9.5, 0.5), WOOD, Wood);
    b.part(root, "Swing Leg R", (-66.75, 4.75, 32.0), (0.5, 9.5, 0.5), WOOD, Wood);
    let swing = b.part(root, "Swing", (-70.0, 5.5, 32.0), (2.0, 7.0, 0.3), (196, 40, 28), Wood);
    b.hinge(swing, Hinge::X, Side::Top, 0.0, None);

    // --- south: the gear range ---
    let range_post = b.part(root, "Range Post", (0.0, 3.0, -36.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(range_post, "Gear range: you have every gear (1-6)");
    // The breakable tower, and its template in Storage (300 studs down).
    // A Folder, not a Model: Models weld their parts, and this has to break apart.
    let template = b.dm.create(Class::Folder, "Tower Template", storage).unwrap();
    for level in 0..8 {
        for (dx, dz) in [(-1.5, -1.5), (1.5, -1.5), (-1.5, 1.5), (1.5, 1.5)] {
            let y = 1.0 + level as f32 * 2.0;
            let color = if level % 2 == 0 { (196, 40, 28) } else { (218, 133, 65) };
            let brick = b.part(template, "Brick", (dx, y - 300.0, -70.0 + dz), (3.0, 2.0, 3.0), color, Brick);
            b.attr(brick, "breakable", Attribute::Bool(true));
        }
    }
    let tower = b.dm.clone_subtree(template).unwrap();
    b.dm.get_mut(tower).unwrap().name = "Tower".into();
    b.dm.reparent(tower, root);
    let bricks: Vec<InstanceId> = b.dm.get(tower).unwrap().children.clone();
    for id in bricks {
        b.dm.part_mut(id).unwrap().position.y += 300.0;
    }
    let tower_post = b.part(root, "Tower Buttons", (10.0, 1.5, -62.0), (0.6, 3.0, 0.6), DARK, Metal);
    b.button(root, "Explode", Some(tower_post), (-0.05, 0.0), (0.08, 0.04), (196, 40, 28), EXPLODE);
    b.button(root, "Rebuild", Some(tower_post), (0.05, 0.0), (0.08, 0.04), (40, 127, 71), REBUILD);
    // Targets, and walls to bank shots off.
    for (i, x) in [-20.0, -12.0, 12.0, 20.0].into_iter().enumerate() {
        let t = b.part(root, "Target", (x, 3.0 + (i % 2) as f32 * 2.0, -75.0), (4.0, 4.0, 0.5), (196, 40, 28), Plastic);
        b.script(t, "Ding", TARGET);
    }
    for x in [-30.0, 30.0] {
        b.part(root, "Bank Wall", (x, 3.0, -58.0), (3.0, 6.0, 12.0), STONE, Concrete);
    }

    // --- north-east: movement ---
    let move_post = b.part(root, "Movement Post", (45.0, 3.0, 45.0), (1.0, 6.0, 1.0), WOOD, Wood);
    b.sign(move_post, "Movement");
    let pads: [(&str, (f32, f32), Rgb, &str); 3] =
        [("Jump Pad", (55.0, 45.0), (60, 220, 255), JUMP_PAD), ("Launch Pad", (63.0, 45.0), (255, 120, 20), LAUNCH_PAD), ("Speed Pad", (71.0, 45.0), (255, 200, 0), SPEED_PAD)];
    for (name, (x, z), color, script) in pads {
        let pad = b.part(root, name, (x, 0.3, z), (4.0, 0.4, 4.0), color, Neon);
        b.script(pad, "Pad", script);
    }
    for (name, other, (x, z)) in [("Teleporter A", "Teleporter B", (55.0, 58.0)), ("Teleporter B", "Teleporter A", (95.0, 95.0))] {
        let pad = b.part(root, name, (x, 0.3, z), (4.0, 0.4, 4.0), (107, 50, 124), Neon);
        b.attr(pad, "destination", Attribute::Str(other.into()));
        b.script(pad, "Teleport", TELEPORTER);
    }
    let belt = b.part(root, "Conveyor", (70.0, 0.3, 60.0), (4.0, 0.4, 20.0), (40, 40, 44), Metal);
    b.dm.part_mut(belt).unwrap().velocity = Vec3::new(0.0, 0.0, 12.0);
    let platform = b.part(root, "Moving Platform", (80.0, 4.0, 75.0), (6.0, 1.0, 6.0), (13, 105, 172), Plastic);
    b.script(platform, "Move", MOVING_PLATFORM);
    let lava = b.part(root, "Lava", (60.0, 0.25, 75.0), (8.0, 0.3, 8.0), (255, 80, 20), Neon);
    b.script(lava, "Burn", LAVA);
    // A pit of bouncy balls.
    for (x, z, w, d) in [(90.0, 50.0, 12.0, 1.0), (90.0, 62.0, 12.0, 1.0), (84.5, 56.0, 1.0, 12.0), (95.5, 56.0, 1.0, 12.0)] {
        b.part(root, "Pit Wall", (x, 1.0, z), (w, 2.0, d), STONE, Concrete);
    }
    for i in 0..16 {
        let colors = [(196, 40, 28), (13, 105, 172), (245, 205, 48), (40, 127, 71)];
        let ball = b.part(root, "Ball", (86.5 + (i % 4) as f32 * 2.5, 2.0 + (i / 4) as f32 * 2.0, 52.5 + ((i / 4) % 4) as f32 * 2.5), (1.8, 1.8, 1.8), colors[i % 4], Plastic);
        let p = b.dm.part_mut(ball).unwrap();
        p.shape = Shape::Ball;
        p.anchored = false;
        p.bounce = 0.8;
    }

    // --- the Lab controls: a toggle button and its panel, in Storage,
    // cloned into each player (scripts come along) ---
    let toggle = b.button(storage, "Lab Toggle Template", None, (0.01, 0.215), (0.1, 0.045), (13, 105, 172), TOGGLE);
    {
        let g = b.dm.gui_mut(toggle).unwrap();
        g.text = "Lab controls".into();
        g.visible = false;
    }
    let panel = b.dm.create(Class::Frame, "Lab Panel Template", storage).unwrap();
    {
        let g = b.dm.gui_mut(panel).unwrap();
        g.x = 0.01;
        g.y = 0.27;
        g.width = 0.21;
        g.height = 0.52;
        g.background_color = Color::new(13, 42, 74);
        g.visible = false;
    }
    let (c1, c2, w, h) = (0.02, 0.115, 0.09, 0.042);
    let row = |i: usize| 0.32 + i as f32 * 0.052;
    b.label(panel, "Lighting", (0.02, 0.275), (0.19, 0.04));
    let blue = (13, 105, 172);
    let lighting: [(&str, &str); 11] = [
        ("Sunrise", "w.time_of_day = 6.3"),
        ("Noon", "w.time_of_day = 12"),
        ("Sunset", "w.time_of_day = 18.2"),
        ("Night", "w.time_of_day = 23"),
        ("Fog", "w.fog_start = 15\n    w.fog_end = 110\n    w.fog_color = {r = 185, g = 195, b = 205}"),
        ("Clear", "w.fog_end = 0"),
        ("Brighter", "w.brightness = min(w.brightness + 0.25, 3)"),
        ("Dimmer", "w.brightness = max(w.brightness - 0.25, 0)"),
        ("Alien sky", "w.sky_color = {r = 70, g = 190, b = 110}"),
        ("Blue sky", "w.sky_color = nil\n    w.brightness = 1"),
        ("Day cycle", "w.cycling = w.cycling != true\n    if w.cycling then\n        self.text = \"Day cycle: on\"\n    else\n        self.text = \"Day cycle\"\n    end"),
    ];
    for (i, (name, what)) in lighting.into_iter().enumerate() {
        let (x, width) = if i == 10 { (c1, 0.185) } else if i % 2 == 0 { (c1, w) } else { (c2, w) };
        b.button(panel, name, None, (x, row(i / 2)), (width, h), blue, &set_world(what));
    }
    b.label(panel, "Music and sound", (0.02, row(6) + 0.005), (0.19, 0.04));
    let music: [(&str, &str); 4] = [
        ("Sunny", "play_music(\"sunny\")"),
        ("Rush", "play_music(\"rush\")"),
        ("Stop music", "stop_music()"),
        ("Boom", "play_sound(\"boom\")"),
    ];
    for (i, (name, what)) in music.into_iter().enumerate() {
        let x = if i % 2 == 0 { c1 } else { c2 };
        b.button(panel, name, None, (x, row(7 + i / 2)), (w, h), (107, 50, 124), &set_world(what));
    }
    b.dm
}
