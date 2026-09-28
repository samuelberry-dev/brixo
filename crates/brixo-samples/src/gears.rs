//! Brixo's standard gear kit: Sword, Slingshot, Rocket Launcher, Superball,
//! Trowel and Paintball Gun, built once here so every game gets the same
//! tuned, well-modelled weapons instead of six copy-pasted scripts.
//!
//! Numbers are tuned to the original Roblox BrickBattle set (2005-2007),
//! which balanced its six gears as one system rather than picking each in
//! isolation: fast-cooldown/low-damage vs. slow-cooldown/high-damage, with
//! projectile speed as the other knob. Where our block-avatar scale differs
//! from Roblox's, the numbers are adapted rather than copied outright.
//!
//! Usage: call `install(&mut dm, storage)` once per game (in a Storage
//! folder, so the templates never show up for players — see BRIXO.md,
//! "Scripts run wherever they are"), then in `on player_joined`, clone each
//! `"<Name> Template"` the same way Spire Wars does.

use brixo_core::{Attribute, Class, Color, DataModel, InstanceId, Material, Shape, Vec3};

/// Every gear's name (also its Tool's name, so `find(name + " Template")`
/// works from a game's own scripts).
pub const NAMES: [&str; 6] = ["Sword", "Slingshot", "Rocket Launcher", "Superball", "Trowel", "Paintball Gun"];

/// Shared by every weapon script: cooldowns, and who counts as an enemy —
/// a team-mate never takes damage from another team-mate's gear. Games with
/// no teams (everyone's `team` is nil) treat everyone as an enemy.
const HELPERS: &str = r#"
fn ready(tool, seconds)
    if tool.ready_at != nil and time() < tool.ready_at then
        return false
    end
    tool.ready_at = time() + seconds
    return true
end

fn enemy(p, other)
    if other.class != "player" or other == p or other.health <= 0 then
        return false
    end
    return p.team == nil or other.team != p.team
end

fn hurt(p, other, amount)
    other.health -= amount
    other.last_hit_by = p.name
end

-- Where a shot starts (`ahead` studs in front of you, `up` above your
-- middle, `right` to your right: the gear in your right hand) and which
-- way it flies: toward where you clicked (p.mouse).
fn aim(p, ahead, up, right)
    f = p.look
    from = {x = p.position.x + f.x * ahead - f.z * right, y = p.position.y + up, z = p.position.z + f.z * ahead + f.x * right}
    t = p.mouse
    dx = t.x - from.x
    dy = t.y - from.y
    dz = t.z - from.z
    d = sqrt(dx * dx + dy * dy + dz * dz)
    if d < 1 then
        return {from = from, dir = {x = f.x, y = 0, z = f.z}}
    end
    return {from = from, dir = {x = dx / d, y = dy / d, z = dz / d}}
end

-- Is `other` part of the gear in `name`'s hand (or their backpack)? A shot
-- starts at the muzzle, so it can brush its own gun on the way out.
fn own_gear(name, other)
    holder = other.parent
    return holder != nil and holder.class == "tool" and holder.parent != nil and holder.parent.name == name
end

-- Turns a long part (a rocket: its length runs along its y) to point
-- along `d`, a direction of length 1.
fn point_along(part, d)
    part.rotation = {x = acos(d.y) * 57.2958, y = atan2(d.x, d.z) * 57.2958, z = 0}
end
"#;

const SWORD: &str = r#"
-- A single swing: everyone enemy in reach, in front of you. 0.4s cooldown,
-- moderate damage — the all-rounder every player starts with.
on activated(p)
    if not ready(self, 0.4) then
        return
    end
    play_sound_at("whoosh", p)
    f = p.look
    for other in players() do
        dx = other.position.x - p.position.x
        dz = other.position.z - p.position.z
        d = sqrt(dx * dx + dz * dz)
        ahead = (dx * f.x + dz * f.z) / max(d, 0.01)
        if enemy(p, other) and d < 7 and ahead > 0.3 and abs(other.position.y - p.position.y) < 4 then
            hurt(p, other, 22)
            play_sound_at("hit", other)
        end
    end
end
"#;

const SLINGSHOT: &str = r#"
-- A fast, cheap pellet: low damage, a 0.2s cooldown, for close-range spam.
-- (Damage falls off each time the pellet bounces off something that isn't
-- a player — see PELLET below.)
on activated(p)
    if not ready(self, 0.2) then
        return
    end
    a = aim(p, 3.2, 1.9, 1.43)
    b = clone(find("Pellet Template"))
    b.name = "Pellet"
    b.owner = p.name
    b.damage = 8
    b.position = a.from
    b.anchored = false
    b.parent = find("Projectiles")
    -- A little lift, so it arcs onto what you clicked instead of dipping short.
    b.velocity = {x = a.dir.x * 90, y = a.dir.y * 90 + 3, z = a.dir.z * 90}
    play_sound_at("twang", p)
end
"#;

const PELLET: &str = r#"
-- Hits an enemy for its current damage, then keeps flying (halved). Bounces
-- off walls losing half its damage each time, so bank shots do less.
-- `gone` guards against touching two things in the same physics step,
-- which would otherwise call destroy(self) twice.
gone = false
fn expire()
    if gone then
        return
    end
    gone = true
    destroy(self)
end
on touched(other)
    if gone or self.parent.name == "Storage" or own_gear(self.owner, other) then
        return
    end
    owner = find(self.owner)
    if owner == nil then
        expire()
        return
    end
    if other.class == "player" then
        if other.name == self.owner then
            return
        end
        if enemy(owner, other) then
            hurt(owner, other, self.damage)
            play_sound_at("hit", other)
        end
        expire()
        return
    end
    -- A wall or the ground: keep going, but weaker, until it's spent.
    self.damage = self.damage / 2
    if self.damage < 1 then
        expire()
    end
end
wait(4)
if self.parent.name != "Storage" then
    expire()
end
"#;

const ROCKET_LAUNCHER: &str = r#"
-- A straight-flying rocket: 3 second reload, a big 8-stud, 55-damage blast
-- on impact. The slowest, heaviest gear in the kit.
on activated(p)
    if not ready(self, 3.0) then
        return
    end
    -- Out of the front of the tube, toward where you clicked.
    a = aim(p, 5.8, 1.7, 1.43)
    r = clone(find("Rocket Template"))
    r.name = "Rocket"
    r.owner = p.name
    r.position = a.from
    point_along(r, a.dir)
    r.anchored = false
    r.parent = find("Projectiles")
    r.velocity = {x = a.dir.x * 60, y = a.dir.y * 60, z = a.dir.z * 60}
    play_sound_at("whoosh", p)
end
"#;

const ROCKET: &str = r#"
-- Explodes on whatever it touches first (or after 5s, whichever's first).
armed_at = time()
gone = false
fn boom()
    if gone then
        return
    end
    gone = true
    hit = explode(self.position, 8, 90)
    for v in hit do
        v.last_hit_by = self.owner
    end
    destroy(self)
end
on touched(other)
    if gone or self.parent.name == "Storage" or time() - armed_at < 0.08 or own_gear(self.owner, other) then
        return
    end
    if other.class == "player" and other.name == self.owner then
        return
    end
    boom()
end
wait(5)
if not gone and self.parent.name != "Storage" then
    boom()
end
"#;

const SUPERBALL: &str = r#"
-- A heavy bouncy ball: 25 damage on a direct hit, a 1.8s cooldown. It keeps
-- bouncing (bounce = 0.92 on its part), losing half its damage and playing
-- a bonk each time it hits something that isn't a player, so a well-aimed
-- bank shot is still worth less than a direct hit.
on activated(p)
    if not ready(self, 1.8) then
        return
    end
    a = aim(p, 3.4, 1.0, 1.43)
    b = clone(find("Superball Ball"))
    b.name = "Superball"
    b.owner = p.name
    b.damage = 25
    b.color = {r = random(60, 255), g = random(60, 255), b = random(60, 255)}
    b.position = a.from
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = a.dir.x * 70, y = a.dir.y * 70 + 5, z = a.dir.z * 70}
    play_sound_at("pop", p)
end
"#;

const SUPERBALL_BALL: &str = r#"
hit_at = {}
on touched(other)
    if self.parent.name == "Storage" or own_gear(self.owner, other) then
        return
    end
    owner = find(self.owner)
    if owner == nil then
        return
    end
    if other.class == "player" then
        if other.name == self.owner then
            return
        end
        last = hit_at[other.name]
        if (last == nil or time() - last > 0.6) and enemy(owner, other) then
            hit_at[other.name] = time()
            hurt(owner, other, self.damage)
            play_sound_at("hit", other)
        end
        return
    end
    -- Bounced off something solid: halve its damage for next time, and bonk.
    self.damage = self.damage / 2
    play_sound_at("bonk", self)
end
wait(6)
if self.parent.name != "Storage" then
    destroy(self)
end
"#;

const TROWEL: &str = r#"
-- Builds a 3x3 wall of breakable bricks where you're looking: free cover,
-- or steps up something. 5 second cooldown; the wall crumbles after 24s on
-- its own, and a nearby explosion (a rocket) blows it apart instantly,
-- since it's ordinary breakable brick like anything else.
on activated(p)
    if not ready(self, 5.0) then
        return
    end
    -- Where you clicked, if it's in reach; otherwise 6 studs ahead.
    f = p.look
    side = {x = f.z, z = -f.x}
    feet = p.position.y - 2.5
    t = p.mouse
    dx = t.x - p.position.x
    dz = t.z - p.position.z
    reach = sqrt(dx * dx + dz * dz)
    if reach > 4 and reach < 30 and abs(t.y - feet) < 12 then
        cx = t.x
        cz = t.z
        base = t.y
    else
        cx = p.position.x + f.x * 6
        cz = p.position.z + f.z * 6
        base = feet
    end
    wall = create("Folder", find("Projectiles"))
    wall.name = "Trowel Wall"
    -- Your team's colour, or a random one when there are no teams.
    if p.team != nil then
        c = p.shirt_color
        r = c.r
        g = c.g
        bl = c.b
    else
        r = random(0, 255)
        g = random(0, 255)
        bl = random(0, 255)
    end
    for row in [0, 1, 2] do
        for col in [-1, 0, 1] do
            b = create("Part", wall)
            b.name = "Wall Brick"
            b.size = {x = 4, y = 2, z = 2}
            b.position = {x = cx + side.x * col * 4, y = base + 1 + row * 2, z = cz + side.z * col * 4}
            b.rotation = {x = 0, y = p.rotation.y, z = 0}
            b.color = {r = r, g = g, b = bl}
            b.material = "brick"
            b.breakable = true
        end
    end
    play_sound_at("thud", {x = cx, y = base, z = cz})
    wait(24)
    if self.parent.name != "Storage" then
        destroy(wall)
    end
end
"#;

const PAINTBALL_GUN: &str = r#"
-- The fastest, weakest gear: 2 damage, a 0.5s cooldown. Not for killing —
-- for area denial and marking a target (a splat, and a burst of your
-- colour that fades), especially useful for covering ground without
-- risking a real fight.
on activated(p)
    if not ready(self, 0.5) then
        return
    end
    a = aim(p, 3.4, 1.7, 1.43)
    b = clone(find("Paintball Template"))
    b.name = "Paintball"
    b.owner = p.name
    b.color = p.shirt_color
    b.position = a.from
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = a.dir.x * 130, y = a.dir.y * 130 + 1, z = a.dir.z * 130}
    play_sound_at("click", p)
end
"#;

const PAINTBALL: &str = r#"
-- `gone` guards against touching two things in the same physics step,
-- which would otherwise splat (and destroy) it twice.
gone = false
on touched(other)
    if gone or self.parent.name == "Storage" or own_gear(self.owner, other) then
        return
    end
    gone = true
    owner = find(self.owner)
    if owner != nil and other.class == "player" and other.name != self.owner and enemy(owner, other) then
        hurt(owner, other, 2)
    end
    -- A splat where it hit: your colour, shrinking away over half a second.
    splat = create("Part", find("Projectiles"))
    splat.name = "Splat"
    splat.shape = "ball"
    splat.size = {x = 1.4, y = 1.4, z = 1.4}
    splat.position = self.position
    splat.color = self.color
    splat.material = "neon"
    splat.can_collide = false
    play_sound_at("splat", splat)
    -- Hide the paintball rather than destroy it yet: destroying an object
    -- stops its script, and this script still has to fade the splat.
    self.transparency = 1
    self.anchored = true
    self.can_collide = false
    for i in [1, 2, 3, 4, 5, 6, 7, 8] do
        wait(0.06)
        splat.transparency = i / 8
    end
    destroy(splat)
    destroy(self)
end
wait(3)
if not gone and self.parent.name != "Storage" then
    gone = true
    destroy(self)
end
"#;

struct Builder<'a> {
    dm: &'a mut DataModel,
}

type Rgb = (u8, u8, u8);

impl Builder<'_> {
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, parent: InstanceId, name: &str, pos: Vec3, size: Vec3, color: Rgb, material: Material, shape: Shape) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = pos;
        p.size = size;
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        p.shape = shape;
        p.anchored = true;
        id
    }
    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = format!("{HELPERS}{}", source.trim_start());
    }
    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.into(), value);
    }

    /// A weapon template: parts pointing along +Z with the handle first (the
    /// hand holds the first part: see `brixo_core::held_arm_angle`), its
    /// script, and whether it's held straight up (a sword) rather than out
    /// in front.
    fn tool(&mut self, storage: InstanceId, name: &str, grip_up: bool, parts: &[G], script: &str) {
        let t = self.dm.create(Class::Tool, &format!("{name} Template"), storage).unwrap();
        for g in parts {
            let id = self.part(t, g.name, Vec3::new(g.at.x, g.at.y - 300.0, g.at.z), g.size, g.color, g.material, g.shape);
            self.dm.part_mut(id).unwrap().rotation = g.turn;
        }
        if grip_up {
            self.attr(t, "grip", Attribute::Str("up".into()));
        }
        self.script(t, "Use", script);
    }
}

/// One piece of a gear's model.
#[derive(Clone, Copy)]
struct G {
    name: &'static str,
    at: Vec3,
    size: Vec3,
    /// Degrees, like any part's rotation.
    turn: Vec3,
    color: Rgb,
    material: Material,
    shape: Shape,
}

/// A plain (unturned) piece.
fn g(name: &'static str, at: Vec3, size: Vec3, color: Rgb, material: Material, shape: Shape) -> G {
    G { name, at, size, turn: Vec3::new(0.0, 0.0, 0.0), color, material, shape }
}

/// A cylinder lying along the gear (+Z): `across` wide, `length` long.
/// (Cylinders stand up along their y, so it's turned 90 degrees.)
fn rod(name: &'static str, at: Vec3, across: f32, length: f32, color: Rgb, material: Material) -> G {
    G { name, at, size: Vec3::new(across, length, across), turn: Vec3::new(90.0, 0.0, 0.0), color, material, shape: Shape::Cylinder }
}

/// A flat point: a square turned 45 degrees about y, so a corner points
/// forward (+Z). `width` is corner to corner.
fn point(name: &'static str, at: Vec3, width: f32, thick: f32, color: Rgb, material: Material) -> G {
    let side = width / std::f32::consts::SQRT_2;
    G { name, at, size: Vec3::new(side, thick, side), turn: Vec3::new(0.0, 45.0, 0.0), color, material, shape: Shape::Block }
}

/// Adds all six weapon templates (and what they fire) to `storage`, a
/// Folder named "Storage". Call once per game.
pub fn install(dm: &mut DataModel, storage: InstanceId) {
    let mut b = Builder { dm };
    let v = Vec3::new;
    use Material::*;
    use Shape::*;
    const STEEL: Rgb = (205, 210, 218);
    const GOLD: Rgb = (212, 170, 50);
    const DARK: Rgb = (32, 34, 38);
    const WOOD: Rgb = (96, 64, 42);

    // --- Sword: held straight up. A wrapped grip, a gold crossguard and
    // pommel, and a broad steel blade with a darker fuller down the middle
    // and a proper point.
    b.tool(storage, "Sword", true, &[
        rod("Handle", v(0.0, 0.0, 0.0), 0.34, 1.1, (70, 45, 30), Wood),
        g("Pommel", v(0.0, 0.0, -0.66), v(0.46, 0.46, 0.46), GOLD, Metal, Ball),
        g("Guard", v(0.0, 0.0, 0.66), v(1.5, 0.26, 0.3), GOLD, Metal, Block),
        g("Blade", v(0.0, 0.0, 2.6), v(0.52, 0.12, 3.6), STEEL, Metal, Block),
        g("Fuller", v(0.0, 0.0, 2.45), v(0.12, 0.14, 3.0), (150, 156, 166), Metal, Block),
        point("Tip", v(0.0, 0.0, 4.4), 0.52, 0.12, STEEL, Metal),
    ], SWORD);

    // --- Slingshot: a forked wooden body with a red band and a pouch.
    b.tool(storage, "Slingshot", false, &[
        g("Handle", v(0.0, 0.0, 0.0), v(0.3, 1.0, 0.3), WOOD, Wood, Cylinder),
        g("Fork L", v(-0.32, 0.8, 0.0), v(0.2, 0.8, 0.2), WOOD, Wood, Cylinder),
        g("Fork R", v(0.32, 0.8, 0.0), v(0.2, 0.8, 0.2), WOOD, Wood, Cylinder),
        g("Yoke", v(0.0, 0.45, 0.0), v(0.84, 0.2, 0.22), WOOD, Wood, Block),
        g("Band", v(0.0, 1.12, 0.0), v(0.66, 0.07, 0.07), (196, 40, 28), Plastic, Block),
        g("Pouch", v(0.0, 1.12, -0.12), v(0.24, 0.16, 0.12), (60, 40, 30), Plastic, Block),
    ], SLINGSHOT);
    let pellet = b.part(storage, "Pellet Template", v(20.0, -300.0, 0.0), v(0.6, 0.6, 0.6), (196, 40, 28), Plastic, Ball);
    b.dm.part_mut(pellet).unwrap().bounce = 0.55;
    b.script(pellet, "Fly", PELLET);

    // --- Rocket Launcher: a long olive tube over a pistol grip, with dark
    // end caps, a yellow warning band, a sight on top and a glowing muzzle.
    const OLIVE: Rgb = (84, 104, 64);
    b.tool(storage, "Rocket Launcher", false, &[
        g("Grip", v(0.0, 0.0, 0.0), v(0.34, 0.8, 0.44), DARK, Plastic, Block),
        rod("Tube", v(0.0, 0.7, 1.0), 0.86, 4.2, OLIVE, Plastic),
        rod("Back Cap", v(0.0, 0.7, -1.15), 1.0, 0.3, DARK, Metal),
        rod("Front Cap", v(0.0, 0.7, 3.15), 1.0, 0.3, DARK, Metal),
        rod("Muzzle", v(0.0, 0.7, 3.31), 0.62, 0.04, (255, 120, 20), Neon),
        rod("Band", v(0.0, 0.7, 2.25), 0.9, 0.22, (245, 200, 40), Plastic),
        g("Sight", v(0.0, 1.24, 1.3), v(0.14, 0.3, 0.46), DARK, Metal, Block),
        g("Trigger Guard", v(0.0, 0.3, 0.3), v(0.12, 0.2, 0.36), DARK, Metal, Block),
    ], ROCKET_LAUNCHER);
    // The rocket: long along its y (scripts turn it to point where it flies).
    let rocket = b.part(storage, "Rocket Template", v(24.0, -300.0, 0.0), v(0.55, 2.2, 0.55), (215, 215, 220), Metal, Cylinder);
    {
        let p = b.dm.part_mut(rocket).unwrap();
        p.floating = true;
    }
    b.script(rocket, "Fly", ROCKET);

    // --- Superball: a two-tone ball with a visible seam, so it reads as a
    // ball rather than a plain sphere.
    b.tool(storage, "Superball", false, &[
        g("Body", v(0.0, 0.0, 0.35), v(1.5, 1.5, 1.5), (107, 50, 124), Neon, Ball),
        g("Seam", v(0.0, 0.0, 0.35), v(1.55, 0.15, 1.55), (60, 25, 75), Plastic, Ball),
    ], SUPERBALL);
    let ball = b.part(storage, "Superball Ball", v(28.0, -300.0, 0.0), v(1.9, 1.9, 1.9), (107, 50, 124), Neon, Ball);
    {
        let p = b.dm.part_mut(ball).unwrap();
        p.bounce = 0.92;
    }
    b.script(ball, "Bounce", SUPERBALL_BALL);

    // --- Trowel: a wooden handle, a steel collar, and a flat pointed blade.
    b.tool(storage, "Trowel", false, &[
        rod("Handle", v(0.0, 0.0, 0.0), 0.3, 1.0, WOOD, Wood),
        rod("Ferrule", v(0.0, 0.0, 0.6), 0.36, 0.22, (150, 150, 156), Metal),
        g("Neck", v(0.0, 0.0, 0.85), v(0.1, 0.1, 0.4), (150, 150, 156), Metal, Block),
        g("Blade", v(0.0, -0.05, 1.55), v(1.1, 0.12, 1.1), STEEL, Metal, Block),
        point("Tip", v(0.0, -0.05, 2.1), 1.1, 0.12, STEEL, Metal),
    ], TROWEL);

    // --- Paintball Gun: a pistol grip under a boxy body, a hopper on top
    // and a long barrel.
    b.tool(storage, "Paintball Gun", false, &[
        g("Grip", v(0.0, 0.0, 0.0), v(0.34, 0.8, 0.44), (25, 25, 28), Plastic, Block),
        g("Body", v(0.0, 0.62, 0.35), v(0.5, 0.55, 1.5), (40, 40, 44), Metal, Block),
        g("Hopper", v(0.0, 1.25, 0.15), v(0.7, 0.7, 0.7), (40, 150, 70), Plastic, Cylinder),
        rod("Barrel", v(0.0, 0.7, 1.9), 0.24, 1.6, (25, 25, 28), Metal),
        g("Trigger Guard", v(0.0, 0.28, 0.3), v(0.12, 0.2, 0.36), (25, 25, 28), Metal, Block),
    ], PAINTBALL_GUN);
    let paintball = b.part(storage, "Paintball Template", v(32.0, -300.0, 0.0), v(0.4, 0.4, 0.4), (255, 255, 255), Plastic, Ball);
    b.dm.part_mut(paintball).unwrap().bounce = 0.15;
    b.script(paintball, "Paint", PAINTBALL); // (not "Splat": that's the mark it leaves)
}

/// Clones every gear's Tool template into `player` (call from
/// `on player_joined`, the same way Spire Wars gives out weapons).
pub fn give_all(dm: &mut DataModel, player: InstanceId) {
    for name in NAMES {
        if let Some(template) = dm.find_first(&format!("{name} Template")) {
            if let Some(copy) = dm.clone_subtree(template) {
                dm.get_mut(copy).unwrap().name = name.to_string();
                dm.reparent(copy, player);
            }
        }
    }
}

/// A flat test map with a plaza, a couple of walls to bank shots off, and a
/// SpawnLocation: enough to see and use every gear. `brixo-samples spread
/// gear-range.brixo` writes this as a file to open in the studio.
pub fn gear_range() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let mut b = Builder { dm: &mut dm };
    let v = Vec3::new;
    use Material::*;
    use Shape::*;

    b.part(root, "Ground", v(0.0, -1.0, 0.0), v(120.0, 2.0, 120.0), (75, 151, 75), Grass, Block);
    b.part(root, "Plaza", v(0.0, 0.1, 0.0), v(50.0, 0.2, 50.0), (170, 166, 160), Concrete, Block);
    for (x, z) in [(-15.0, -20.0), (15.0, -20.0), (-15.0, 20.0), (15.0, 20.0)] {
        b.part(root, "Bank Wall", v(x, 3.0, z), v(3.0, 6.0, 10.0), (163, 162, 165), Concrete, Block);
    }
    let spawn = dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    {
        let p = dm.part_mut(spawn).unwrap();
        p.position = Vec3::new(0.0, 0.6, 0.0);
        p.size = Vec3::new(8.0, 0.8, 8.0);
        p.color = Color::new(245, 205, 48);
        p.material = Material::Metal;
    }
    let storage = dm.create(Class::Folder, "Storage", root).unwrap();
    install(&mut dm, storage);
    dm.create(Class::Folder, "Projectiles", root).unwrap();
    let script = r#"
on player_joined(p)
    give_all(p)
end
"#;
    let s = dm.create(Class::Script, "Give Gear", root).unwrap();
    dm.script_mut(s).unwrap().source = format!(
        "-- Everyone gets the whole gear kit on spawn.\nfn give_all(p)\n{}\nend\n{}",
        NAMES.iter().map(|n| format!("    t = clone(find(\"{n} Template\"))\n    t.name = \"{n}\"\n    t.parent = p")).collect::<Vec<_>>().join("\n"),
        script.trim_start()
    );
    dm
}
