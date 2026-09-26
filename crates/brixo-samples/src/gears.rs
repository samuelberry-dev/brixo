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
"#;

const SWORD: &str = r#"
-- A single swing: everyone enemy in reach, in front of you. 0.4s cooldown,
-- moderate damage — the all-rounder every player starts with.
on activated(p)
    if not ready(self, 0.4) then
        return
    end
    play_sound("whoosh")
    f = p.look
    for other in players() do
        dx = other.position.x - p.position.x
        dz = other.position.z - p.position.z
        d = sqrt(dx * dx + dz * dz)
        ahead = (dx * f.x + dz * f.z) / max(d, 0.01)
        if enemy(p, other) and d < 7 and ahead > 0.3 and abs(other.position.y - p.position.y) < 4 then
            hurt(p, other, 22)
            play_sound("hit")
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
    f = p.look
    b = clone(find("Pellet Template"))
    b.name = "Pellet"
    b.owner = p.name
    b.damage = 8
    -- (From chest height, so it meets people square on.)
    b.position = {x = p.position.x + f.x * 2.5, y = p.position.y + 0.8, z = p.position.z + f.z * 2.5}
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = f.x * 85, y = 4, z = f.z * 85}
    play_sound("twang")
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
    if gone or self.parent.name == "Storage" then
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
            play_sound("hit")
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
    f = p.look
    r = clone(find("Rocket Template"))
    r.name = "Rocket"
    r.owner = p.name
    r.position = {x = p.position.x + f.x * 4, y = p.position.y + 1.8, z = p.position.z + f.z * 4}
    r.rotation = {x = 0, y = p.rotation.y, z = 0}
    r.anchored = false
    r.parent = find("Projectiles")
    r.velocity = {x = f.x * 60, y = 0, z = f.z * 60}
    play_sound("whoosh")
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
    if gone or self.parent.name == "Storage" or time() - armed_at < 0.08 then
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
    f = p.look
    b = clone(find("Superball Ball"))
    b.name = "Superball"
    b.owner = p.name
    b.damage = 25
    b.color = {r = random(60, 255), g = random(60, 255), b = random(60, 255)}
    b.position = {x = p.position.x + f.x * 3, y = p.position.y + 1.5, z = p.position.z + f.z * 3}
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = f.x * 65, y = 8, z = f.z * 65}
    play_sound("pop")
end
"#;

const SUPERBALL_BALL: &str = r#"
hit_at = {}
on touched(other)
    if self.parent.name == "Storage" then
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
            play_sound("hit")
        end
        return
    end
    -- Bounced off something solid: halve its damage for next time, and bonk.
    self.damage = self.damage / 2
    play_sound("bonk")
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
    f = p.look
    side = {x = f.z, z = -f.x}
    cx = p.position.x + f.x * 6
    cz = p.position.z + f.z * 6
    base = p.position.y - 2.5
    wall = create("Folder", find("Projectiles"))
    wall.name = "Trowel Wall"
    r = random(0, 255)
    g = random(0, 255)
    bl = random(0, 255)
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
    play_sound("thud")
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
    f = p.look
    b = clone(find("Paintball Template"))
    b.name = "Paintball"
    b.owner = p.name
    b.color = p.shirt_color
    -- (From chest height, so it meets people square on.)
    b.position = {x = p.position.x + f.x * 2.5, y = p.position.y + 0.8, z = p.position.z + f.z * 2.5}
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = f.x * 130, y = 1, z = f.z * 130}
    play_sound("click")
end
"#;

const PAINTBALL: &str = r#"
-- `gone` guards against touching two things in the same physics step,
-- which would otherwise splat (and destroy) it twice.
gone = false
on touched(other)
    if gone or self.parent.name == "Storage" then
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
    play_sound("splat")
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
    /// renderer and runtime hold it by that convention — see
    /// `brixo_core::held_arm_angle`), its script, and whether it's held
    /// straight up (a sword) rather than out in front.
    fn tool(&mut self, storage: InstanceId, name: &str, grip_up: bool, parts: &[(&str, Vec3, Vec3, Rgb, Material, Shape)], script: &str) {
        let t = self.dm.create(Class::Tool, &format!("{name} Template"), storage).unwrap();
        for (pname, pos, size, color, material, shape) in parts {
            self.part(t, pname, Vec3::new(pos.x, pos.y - 300.0, pos.z), *size, *color, *material, *shape);
        }
        if grip_up {
            self.attr(t, "grip", Attribute::Str("up".into()));
        }
        self.script(t, "Use", script);
    }
}

/// Adds all six weapon templates (and what they fire) to `storage`, a
/// Folder named "Storage". Call once per game.
pub fn install(dm: &mut DataModel, storage: InstanceId) {
    let mut b = Builder { dm };
    let v = Vec3::new;
    use Material::*;
    use Shape::*;

    // --- Sword: a crossguard, a wrapped grip, and a tapered double-edged
    // blade (two thin blocks meeting at a point would need a wedge; a
    // single blade with a wedge tip reads cleanly at this scale).
    b.tool(storage, "Sword", true, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.4, 0.4, 1.1), (78, 53, 36), Wood, Cylinder),
        ("Pommel", v(0.0, 0.0, -0.65), v(0.55, 0.55, 0.3), (140, 120, 40), Metal, Ball),
        ("Guard", v(0.0, 0.0, 0.65), v(1.6, 0.28, 0.32), (140, 120, 40), Metal, Block),
        ("Blade", v(0.0, 0.0, 2.7), v(0.22, 0.5, 3.6), (210, 214, 222), Metal, Block),
        ("Tip", v(0.0, 0.0, 4.75), v(0.22, 0.5, 0.9), (210, 214, 222), Metal, Wedge),
    ], SWORD);

    // --- Slingshot: a forked wooden body with a visible band.
    b.tool(storage, "Slingshot", false, &[
        ("Handle", v(0.0, -0.4, 0.0), v(0.35, 1.1, 0.35), (96, 64, 42), Wood, Cylinder),
        ("Fork L", v(-0.35, 0.55, 0.15), v(0.22, 0.9, 0.22), (96, 64, 42), Wood, Cylinder),
        ("Fork R", v(0.35, 0.55, 0.15), v(0.22, 0.9, 0.22), (96, 64, 42), Wood, Cylinder),
        ("Band", v(0.0, 1.0, 0.15), v(0.85, 0.08, 0.08), (196, 40, 28), Plastic, Block),
        ("Pouch", v(0.0, 0.65, 0.15), v(0.3, 0.15, 0.1), (60, 40, 30), Plastic, Block),
    ], SLINGSHOT);
    let pellet = b.part(storage, "Pellet Template", v(20.0, -300.0, 0.0), v(0.6, 0.6, 0.6), (196, 40, 28), Plastic, Ball);
    b.dm.part_mut(pellet).unwrap().bounce = 0.55;
    b.script(pellet, "Fly", PELLET);

    // --- Rocket Launcher: a shoulder tube with a handle, a sight, and a
    // muzzle that glows.
    b.tool(storage, "Rocket Launcher", false, &[
        ("Grip", v(0.0, -0.9, 0.0), v(0.4, 0.9, 0.5), (27, 42, 53), Metal, Block),
        ("Tube", v(0.0, -0.35, 0.6), v(0.95, 0.95, 3.6), (70, 90, 60), Metal, Cylinder),
        ("Sight", v(0.0, 0.25, 0.2), v(0.15, 0.35, 0.7), (20, 20, 24), Metal, Block),
        ("Muzzle", v(0.0, -0.35, 2.5), v(1.05, 1.05, 0.3), (255, 120, 20), Neon, Cylinder),
    ], ROCKET_LAUNCHER);
    let rocket = b.part(storage, "Rocket Template", v(24.0, -300.0, 0.0), v(0.7, 0.7, 2.2), (215, 215, 220), Metal, Cylinder);
    {
        let p = b.dm.part_mut(rocket).unwrap();
        p.floating = true;
    }
    b.script(rocket, "Fly", ROCKET);

    // --- Superball: a two-tone ball with a visible seam, so it reads as a
    // ball rather than a plain sphere.
    b.tool(storage, "Superball", false, &[
        ("Body", v(0.0, 0.0, 0.35), v(1.5, 1.5, 1.5), (107, 50, 124), Neon, Ball),
        ("Seam", v(0.0, 0.0, 0.35), v(1.55, 0.15, 1.55), (60, 25, 75), Plastic, Ball),
    ], SUPERBALL);
    let ball = b.part(storage, "Superball Ball", v(28.0, -300.0, 0.0), v(1.9, 1.9, 1.9), (107, 50, 124), Neon, Ball);
    {
        let p = b.dm.part_mut(ball).unwrap();
        p.bounce = 0.92;
    }
    b.script(ball, "Bounce", SUPERBALL_BALL);

    // --- Trowel: a proper spade shape, angled blade and a wooden shaft.
    b.tool(storage, "Trowel", false, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.3, 0.3, 1.0), (96, 64, 42), Wood, Cylinder),
        ("Ferrule", v(0.0, 0.0, 0.55), v(0.4, 0.4, 0.25), (140, 140, 145), Metal, Cylinder),
        ("Blade", v(0.0, -0.05, 1.15), v(1.0, 0.08, 1.3), (190, 195, 200), Metal, Wedge),
    ], TROWEL);

    // --- Paintball Gun: a hopper-fed marker with a barrel and a grip.
    b.tool(storage, "Paintball Gun", false, &[
        ("Body", v(0.0, 0.0, 0.0), v(0.5, 0.6, 1.3), (40, 40, 44), Metal, Block),
        ("Hopper", v(0.0, 0.55, -0.2), v(0.7, 0.7, 0.7), (20, 20, 24), Plastic, Cylinder),
        ("Barrel", v(0.0, 0.05, 1.1), v(0.25, 0.25, 1.4), (25, 25, 28), Metal, Cylinder),
        ("Grip", v(0.0, -0.55, -0.3), v(0.35, 0.7, 0.4), (25, 25, 28), Plastic, Block),
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
