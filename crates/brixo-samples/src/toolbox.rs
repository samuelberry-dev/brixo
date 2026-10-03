//! The toolbox's built-in items: ready-made things creators drop into
//! their games from Studio's Toolbox (karts, doors, pads, the gear kit...).
//!
//! Each is built here in code and stored the way Studio copies things (the
//! clipboard text of `DataModel::to_clipboard`), so inserting one is just a
//! paste. The website seeds its toolbox from these (with a picture each);
//! admins add more there. Every item is self-contained: its scripts find
//! what they need through `self`, never by a name elsewhere in the game,
//! so they work in any game, and more than one copy works.
//!
//! Items are built around the origin, standing on y = 0 (Studio moves them
//! to where you're looking), and face +Z.

use brixo_core::{Attribute, Class, Color, DataModel, Hinge, InstanceId, Material, Shape, Side, Vec3};

/// One toolbox item.
pub struct Item {
    /// Stays the same between releases: the website updates its copy by it.
    pub slug: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    /// What gets pasted (see `DataModel::to_clipboard`).
    pub content: String,
    /// Its picture in the Toolbox (a PNG).
    pub thumbnail: &'static [u8],
}

/// The categories, in the order the Toolbox lists them.
pub const CATEGORIES: [&str; 5] = ["Vehicles", "Building", "Obby", "Gameplay", "Weapons"];

type Rgb = (u8, u8, u8);

struct B {
    dm: DataModel,
}

impl B {
    fn part(&mut self, parent: InstanceId, name: &str, at: (f32, f32, f32), size: (f32, f32, f32), color: Rgb, material: Material) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = Vec3::new(at.0, at.1, at.2);
        p.size = Vec3::new(size.0, size.1, size.2);
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        p.anchored = true;
        id
    }

    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }

    fn model(&mut self, name: &str) -> InstanceId {
        let root = self.dm.root();
        self.dm.create(Class::Model, name, root).unwrap()
    }

    fn top(&mut self, class: Class, name: &str) -> InstanceId {
        let root = self.dm.root();
        self.dm.create(class, name, root).unwrap()
    }
}

/// Builds one item in a fresh world and gives back its clipboard text.
fn build(make: impl FnOnce(&mut B) -> InstanceId) -> String {
    let mut b = B { dm: DataModel::new() };
    let top = make(&mut b);
    b.dm.to_clipboard(&[top])
}

// --- the scripts ---------------------------------------------------------------

const KART: &str = r#"
-- Walk up to the kart and press F to drive it; F again to get out.
-- W/S drive, A/D steer, hold Space while turning to drift.
kart = self
seat = nil
for c in kart.children do
    if c.name == "Chassis" then
        seat = c
    end
end

-- A sign over it while nobody's driving.
sign = create("TextLabel", kart)
sign.name = "Kart Sign"
sign.text = "F to drive"
sign.attached_to = seat
sign.width = 0.07
sign.height = 0.03
sign.text_size = 14
sign.background = true
sign.background_color = {r = 27, g = 42, b = 53}

on key(p, k)
    if k != "f" or seat == nil then
        return
    end
    -- (Every kart's script hears the key: the first to act wins.)
    if p.kart_changed_at != nil and time() - p.kart_changed_at < 0.3 then
        return
    end
    if p.kart == kart then
        p.kart = nil
        p.kart_changed_at = time()
    elseif p.kart == nil and kart.driver == nil then
        dx = p.position.x - seat.position.x
        dz = p.position.z - seat.position.z
        if dx * dx + dz * dz < 81 then
            p.kart = kart
            p.kart_changed_at = time()
        end
    end
end

every 0.25 seconds
    sign.visible = kart.driver == nil
end
"#;

const SWING_DOOR: &str = r#"
-- Swings open away from whoever walks into it, and closes 3 seconds after.
-- Which way is "away" comes from the way the door faces, so it works
-- turned any way round.
a = self.rotation.y / 57.2958
front = {x = sin(a), z = cos(a)}
home = {x = self.position.x, z = self.position.z}
open_at = 0

on touched(other)
    if other.class == "player" then
        side = (other.position.x - home.x) * front.x + (other.position.z - home.z) * front.z
        if side < 0 then
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

const SLIDING_DOOR: &str = r#"
-- Slides up into the wall when someone walks into it, and back down
-- 3 seconds later.
closed_y = self.position.y
busy = false

on touched(other)
    if other.class != "player" or busy then
        return
    end
    busy = true
    play_sound_at("click", self)
    for i in 1..20 do
        self.position.y = closed_y + i * 0.4
        wait(0.03)
    end
    wait(3)
    for i in 1..20 do
        self.position.y = closed_y + 8 - i * 0.4
        wait(0.03)
    end
    self.position.y = closed_y
    busy = false
end
"#;

const JUMP_PAD: &str = r#"
-- Bounces whoever steps on it high into the air. Change `power` to jump
-- higher or lower.
power = 80

on touched(other)
    if other.class == "player" then
        other.velocity = {x = other.velocity.x, y = power, z = other.velocity.z}
        play_sound_at("jump", self)
    end
end
"#;

const SPEED_PAD: &str = r#"
-- Flings whoever steps on it forward, the way the pad faces (turn the pad
-- to aim it). `power` is how far.
power = 70

on touched(other)
    if other.class == "player" then
        a = self.rotation.y / 57.2958
        other.velocity = {x = sin(a) * power, y = 45, z = cos(a) * power}
        play_sound_at("whoosh", self)
    end
end
"#;

const BOOST_PAD: &str = r#"
-- Karts that drive over it get a burst of speed.
self.can_collide = false

on touched(other)
    if other.class == "player" and other.kart != nil then
        boost(other.kart, 1.2)
        play_sound("whoosh", other)
    end
end
"#;

const KILL_BRICK: &str = r#"
-- Touch it and you're out.
on touched(other)
    if other.class == "player" then
        other.health = 0
    end
end
"#;

const CHECKPOINT: &str = r#"
-- Touch it, and when you respawn you come back here instead of the start.
-- Checkpoints only count going forward: give each one a higher `stage`
-- than the one before (it's a custom field: change it in Properties).
if self.stage == nil then
    self.stage = 1
end

on touched(other)
    if other.class != "player" then
        return
    end
    if other.stage == nil or self.stage > other.stage then
        other.stage = self.stage
        other.checkpoint_x = self.position.x
        other.checkpoint_y = self.position.y
        other.checkpoint_z = self.position.z
        play_sound("win", other)
    end
end

on respawned(p)
    -- (Every checkpoint hears this; the one they got to sends them back.)
    if p.stage == self.stage and p.checkpoint_x != nil then
        p.position = {x = p.checkpoint_x, y = p.checkpoint_y + 4, z = p.checkpoint_z}
    end
end
"#;

const TELEPORTER: &str = r#"
-- Step on this pad and you're sent to the other one in the pair (move
-- them anywhere). A short wait after arriving stops you bouncing back.
target = nil
for c in self.parent.children do
    if c.class == "part" and c != self then
        target = c
    end
end

on touched(other)
    if other.class != "player" or target == nil then
        return
    end
    if other.teleported_at != nil and time() - other.teleported_at < 2 then
        return
    end
    other.teleported_at = time()
    other.position = {x = target.position.x, y = target.position.y + 4, z = target.position.z}
    play_sound("whoosh", other)
end
"#;

const COIN: &str = r#"
-- Touch it for a coin (players' `coins`). It comes back 10 seconds later.
-- Want coins on the leaderboard? Put leaderboard("coins") in a script.
taken = false

on touched(other)
    if other.class != "player" or taken then
        return
    end
    taken = true
    if other.coins == nil then
        other.coins = 0
    end
    other.coins += 1
    play_sound("coin", other)
    self.transparency = 1
    wait(10)
    self.transparency = 0
    taken = false
end

every 0.03 seconds
    self.rotation.y += 4
end
"#;

const MOVING_PLATFORM: &str = r#"
-- Glides back and forth, and players riding it go along. `distance` is
-- how far (along the way it faces), `seconds` one trip there and back.
distance = 20
seconds = 6
start = {x = self.position.x, y = self.position.y, z = self.position.z}
a = self.rotation.y / 57.2958

every 0.03 seconds
    t = (1 - cos(time() / seconds * 6.2832)) / 2
    self.position.x = start.x + sin(a) * distance * t
    self.position.z = start.z + cos(a) * distance * t
end
"#;

const GEAR_GIVER: &str = r#"
-- Everyone gets the whole gear kit (Sword, Slingshot, Rocket Launcher,
-- Superball, Trowel, Paintball Gun) when they join (they keep it when
-- they're knocked out). The gear itself waits in the Storage folder in here.
kit = nil
for c in self.children do
    if c.name == "Storage" then
        kit = c
    end
end

fn give(p)
    if kit == nil then
        return
    end
    for t in kit.children do
        if t.class == "tool" then
            g = clone(t)
            -- "Sword Template" is handed out as "Sword".
            g.name = t.gear_name
            g.parent = p
        end
    end
end

on player_joined(p)
    give(p)
end
"#;

/// The Car's driving: the seat says what its driver presses, and this turns
/// the wheels' motors (tank steering: the two sides run apart to turn).
const CAR: &str = r#"
-- Walk into the seat to drive: W/S go, A/D turn, Space gets out.
-- The seat's throttle and steer say what the driver presses; this
-- turns the wheels' motors to match.
seat = nil
left = []
right = []
for c in self.children do
    if c.name == "Seat" then
        seat = c
    elseif c.name == "Left Wheel" then
        push(left, c)
    elseif c.name == "Right Wheel" then
        push(right, c)
    end
end
-- Degrees a second at full throttle. (Wheels turned the other way round
-- drive backwards: flip the sign.)
speed = -300
-- How hard it turns: the two sides run this much apart.
turning = 2
every 0.05 seconds
    go = seat.throttle
    turn = seat.steer
    if go == nil then
        go = 0
    end
    if turn == nil then
        turn = 0
    end
    for w in left do
        w.motor_speed = (go - turn * turning) * speed
    end
    for w in right do
        w.motor_speed = (go + turn * turning) * speed
    end
end
"#;

// --- the items -------------------------------------------------------------------

fn car() -> String {
    build(|b| {
        let m = b.model("Car");
        let red: Rgb = (196, 40, 28);
        let dark: Rgb = (27, 42, 53);
        // The body first: the Model welds its other parts to it.
        let body = b.part(m, "Body", (0.0, 1.6, 0.0), (5.0, 1.0, 8.0), red, Material::Plastic);
        let hood = b.part(m, "Hood", (0.0, 2.4, 2.6), (5.0, 0.6, 2.8), red, Material::Plastic);
        let back = b.part(m, "Back", (0.0, 2.9, -3.4), (5.0, 1.6, 1.2), red, Material::Plastic);
        let seat = b.part(m, "Seat", (0.0, 2.6, -1.6), (2.0, 1.0, 2.0), dark, Material::Plastic);
        for id in [body, hood, back, seat] {
            b.dm.part_mut(id).unwrap().anchored = false;
        }
        b.dm.part_mut(seat).unwrap().seat = true;
        for (x, z) in [(-3.0, -2.5), (3.0, -2.5), (-3.0, 2.5), (3.0, 2.5)] {
            let name = if x < 0.0 { "Left Wheel" } else { "Right Wheel" };
            let w = b.part(m, name, (x, 1.5, z), (3.0, 1.0, 3.0), (40, 40, 44), Material::Plastic);
            let p = b.dm.part_mut(w).unwrap();
            p.shape = Shape::Cylinder;
            p.rotation = Vec3::new(0.0, 0.0, 90.0);
            p.hinge = Hinge::Y;
            p.anchored = false;
        }
        b.script(m, "Drive", CAR);
        m
    })
}

fn kart() -> String {
    build(|b| {
        let root = b.dm.root();
        let k = crate::speedway::build_kart(&mut b.dm, root, "Kart", (0.0, 1.1, 0.0), (196, 40, 28), (245, 205, 48));
        b.script(k, "Drive", KART);
        k
    })
}

fn swing_door() -> String {
    build(|b| {
        let m = b.model("Swinging Door");
        let wood: Rgb = (105, 64, 40);
        // The frame first: the Model welds its parts to its first part.
        b.part(m, "Frame Left", (-3.0, 4.0, 0.0), (1.0, 8.0, 1.0), wood, Material::Wood);
        b.part(m, "Frame Right", (3.0, 4.0, 0.0), (1.0, 8.0, 1.0), wood, Material::Wood);
        b.part(m, "Frame Top", (0.0, 8.5, 0.0), (7.0, 1.0, 1.0), wood, Material::Wood);
        let door = b.part(m, "Door", (0.0, 4.2, 0.0), (5.0, 7.0, 0.4), (200, 140, 70), Material::Wood);
        let d = b.dm.part_mut(door).unwrap();
        d.anchored = false;
        d.hinge = Hinge::Y;
        d.hinge_at = Side::Left;
        d.swing_to = Some(0.0);
        b.script(door, "Open", SWING_DOOR);
        m
    })
}

fn sliding_door() -> String {
    build(|b| {
        let m = b.model("Sliding Door");
        let grey: Rgb = (99, 95, 98);
        b.part(m, "Wall Left", (-4.5, 5.0, 0.0), (3.0, 10.0, 1.0), grey, Material::Concrete);
        b.part(m, "Wall Right", (4.5, 5.0, 0.0), (3.0, 10.0, 1.0), grey, Material::Concrete);
        b.part(m, "Wall Top", (0.0, 9.0, 0.0), (6.0, 2.0, 1.0), grey, Material::Concrete);
        let door = b.part(m, "Door", (0.0, 4.0, 0.0), (6.0, 8.0, 0.6), (13, 105, 172), Material::Metal);
        b.script(door, "Slide", SLIDING_DOOR);
        m
    })
}

fn pad(name: &str, color: Rgb, script_name: &str, script: &str) -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, name, (0.0, 0.5, 0.0), (6.0, 1.0, 6.0), color, Material::Neon);
        b.script(p, script_name, script);
        p
    })
}

fn speed_pad() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Speed Pad", (0.0, 0.5, 0.0), (6.0, 1.0, 8.0), (255, 150, 30), Material::Neon);
        b.script(p, "Fling", SPEED_PAD);
        // A white chevron on it, pointing the way it throws you.
        for (dx, turn) in [(-0.9, 45.0), (0.9, -45.0)] {
            let bar = b.dm.create(Class::Part, "Arrow", p).unwrap();
            let a = b.dm.part_mut(bar).unwrap();
            a.position = Vec3::new(dx, 1.05, 1.0);
            a.size = Vec3::new(0.6, 0.1, 3.0);
            a.rotation = Vec3::new(0.0, turn, 0.0);
            a.color = Color::new(255, 255, 255);
            a.material = Material::Neon;
            a.can_collide = false;
            a.anchored = true;
        }
        p
    })
}

fn boost_pad() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Boost Pad", (0.0, 0.1, 0.0), (8.0, 0.2, 8.0), (255, 150, 30), Material::Neon);
        b.script(p, "Boost", BOOST_PAD);
        p
    })
}

fn checkpoint() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Checkpoint", (0.0, 0.5, 0.0), (6.0, 1.0, 6.0), (75, 151, 75), Material::Neon);
        b.dm.get_mut(p).unwrap().attributes.insert("stage".into(), Attribute::Num(1.0));
        b.script(p, "Checkpoint", CHECKPOINT);
        p
    })
}

fn teleporters() -> String {
    build(|b| {
        let m = b.top(Class::Folder, "Teleporter Pair");
        for (name, x, color) in [("Pad A", -8.0, (107, 50, 124)), ("Pad B", 8.0, (4, 175, 236))] {
            let p = b.part(m, name, (x, 0.5, 0.0), (5.0, 1.0, 5.0), color, Material::Neon);
            b.script(p, "Teleport", TELEPORTER);
        }
        m
    })
}

fn kill_brick() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Kill Brick", (0.0, 0.5, 0.0), (8.0, 1.0, 4.0), (255, 0, 0), Material::Neon);
        b.script(p, "Kill", KILL_BRICK);
        p
    })
}

fn coin() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Coin", (0.0, 2.5, 0.0), (2.0, 0.4, 2.0), (245, 205, 48), Material::Neon);
        let c = b.dm.part_mut(p).unwrap();
        c.shape = Shape::Cylinder;
        c.rotation = Vec3::new(90.0, 0.0, 0.0);
        c.can_collide = false;
        b.script(p, "Coin", COIN);
        p
    })
}

fn spinner() -> String {
    build(|b| {
        let m = b.model("Spinner");
        b.part(m, "Post", (0.0, 1.5, 0.0), (2.0, 3.0, 2.0), (27, 42, 53), Material::Metal);
        let bar = b.part(m, "Bar", (0.0, 3.5, 0.0), (16.0, 1.0, 1.0), (196, 40, 28), Material::Plastic);
        let p = b.dm.part_mut(bar).unwrap();
        p.anchored = false;
        p.hinge = Hinge::Y;
        p.hinge_at = Side::Bottom;
        p.motor_speed = 120.0;
        m
    })
}

fn moving_platform() -> String {
    build(|b| {
        let root = b.dm.root();
        let p = b.part(root, "Moving Platform", (0.0, 0.5, 0.0), (8.0, 1.0, 8.0), (218, 133, 65), Material::Wood);
        b.script(p, "Move", MOVING_PLATFORM);
        p
    })
}

fn gear_kit() -> String {
    build(|b| {
        let kit = b.top(Class::Folder, "Gear Kit");
        let storage = b.dm.create(Class::Folder, "Storage", kit).unwrap();
        // Where shots fly (the gear puts rockets and pellets in it).
        b.dm.create(Class::Folder, "Projectiles", kit).unwrap();
        crate::gears::install(&mut b.dm, storage);
        // Each tool knows the name it's handed out as.
        for name in crate::gears::NAMES {
            if let Some(t) = b.dm.get(storage).unwrap().children.iter().copied().find(|c| b.dm.get(*c).unwrap().name == format!("{name} Template")) {
                b.dm.get_mut(t).unwrap().attributes.insert("gear_name".into(), Attribute::Str(name.into()));
            }
        }
        b.script(kit, "Give Gear", GEAR_GIVER);
        kit
    })
}

macro_rules! thumb {
    ($slug:literal) => {
        include_bytes!(concat!("../assets/toolbox/", $slug, ".png"))
    };
}

/// Every built-in item.
pub fn items() -> Vec<Item> {
    vec![
        Item {
            slug: "car",
            name: "Car",
            category: "Vehicles",
            description: "A car built from parts: walk into the seat to drive (W/S, A/D, Space to get out). Its script turns the wheels' motors: change it to make it your own.",
            content: car(),
            thumbnail: thumb!("car"),
        },
        Item {
            slug: "kart",
            name: "Kart",
            category: "Vehicles",
            description: "A go-kart anyone can drive: walk up and press F. Drifts, boosts and jumps like Brickport Speedway's.",
            content: kart(),
            thumbnail: thumb!("kart"),
        },
        Item {
            slug: "boost-pad",
            name: "Boost Pad",
            category: "Vehicles",
            description: "Karts that drive over it get a burst of speed.",
            content: boost_pad(),
            thumbnail: thumb!("boost-pad"),
        },
        Item {
            slug: "swinging-door",
            name: "Swinging Door",
            category: "Building",
            description: "A door on hinges in its frame. Opens away from whoever walks into it, and closes behind them.",
            content: swing_door(),
            thumbnail: thumb!("swinging-door"),
        },
        Item {
            slug: "sliding-door",
            name: "Sliding Door",
            category: "Building",
            description: "Slides up into the wall when someone walks into it, and back down after 3 seconds.",
            content: sliding_door(),
            thumbnail: thumb!("sliding-door"),
        },
        Item {
            slug: "moving-platform",
            name: "Moving Platform",
            category: "Obby",
            description: "Glides back and forth the way it faces, carrying whoever stands on it.",
            content: moving_platform(),
            thumbnail: thumb!("moving-platform"),
        },
        Item {
            slug: "spinner",
            name: "Spinner",
            category: "Obby",
            description: "A bar spinning on a post, sweeping players off. No script: a hinge with a motor.",
            content: spinner(),
            thumbnail: thumb!("spinner"),
        },
        Item {
            slug: "kill-brick",
            name: "Kill Brick",
            category: "Obby",
            description: "The classic. Touch it and you're out.",
            content: kill_brick(),
            thumbnail: thumb!("kill-brick"),
        },
        Item {
            slug: "checkpoint",
            name: "Checkpoint",
            category: "Obby",
            description: "Respawn here instead of at the start. Give each checkpoint a higher stage (in Properties).",
            content: checkpoint(),
            thumbnail: thumb!("checkpoint"),
        },
        Item {
            slug: "jump-pad",
            name: "Jump Pad",
            category: "Gameplay",
            description: "Bounces whoever steps on it high into the air.",
            content: pad("Jump Pad", (4, 175, 236), "Bounce", JUMP_PAD),
            thumbnail: thumb!("jump-pad"),
        },
        Item {
            slug: "speed-pad",
            name: "Speed Pad",
            category: "Gameplay",
            description: "Flings you forward the way the arrow points. Turn it to aim.",
            content: speed_pad(),
            thumbnail: thumb!("speed-pad"),
        },
        Item {
            slug: "teleporters",
            name: "Teleporter Pair",
            category: "Gameplay",
            description: "Two pads: step on one, come out of the other. Move them anywhere.",
            content: teleporters(),
            thumbnail: thumb!("teleporters"),
        },
        Item {
            slug: "coin",
            name: "Coin",
            category: "Gameplay",
            description: "A spinning coin: touch it for a coin (the player's coins field). It comes back after 10 seconds.",
            content: coin(),
            thumbnail: thumb!("coin"),
        },
        Item {
            slug: "gear-kit",
            name: "Gear Kit",
            category: "Weapons",
            description: "Brixo's six weapons (Sword, Slingshot, Rocket Launcher, Superball, Trowel, Paintball Gun), given to everyone when they join. Teams don't hurt each other.",
            content: gear_kit(),
            thumbnail: thumb!("gear-kit"),
        },
    ]
}
