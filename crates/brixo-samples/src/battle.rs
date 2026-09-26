//! Spire Wars: a remake of the classic tower brickbattle. Four teams, four
//! towers of breakable bricks, six weapons, and rounds that end with the
//! whole map rebuilt. Shows off tools, explosions, collapsing buildings,
//! teams, GUI, scripts, custom sounds and the death animation.

use brixo_core::{Attribute, Class, Color, DataModel, InstanceId, Material, Shape, SoundProps, Vec3};

pub const TEAMS: [(&str, (u8, u8, u8), (f32, f32)); 4] = [
    ("Red", (196, 40, 28), (-75.0, -75.0)),
    ("Blue", (13, 105, 172), (75.0, -75.0)),
    ("Green", (40, 127, 71), (-75.0, 75.0)),
    ("Yellow", (245, 205, 48), (75.0, 75.0)),
];
/// Tower size: a 12x12 footprint, 16 rows of 2-stud bricks (32 studs tall).
const TOWER_ROWS: usize = 16;
/// The rebuild template sits this far below the map.
const TEMPLATE_DROP: f32 = 500.0;

const GAME: &str = r#"-- Spire Wars: teams, weapons, knockouts, rounds.
TEAMS = ["Red", "Blue", "Green", "Yellow"]
TOOLS = ["Sword", "Rocket Launcher", "Superball", "Slingshot", "Trowel", "Timebomb"]
round_length = find("Workspace").round_seconds
if round_length == nil then
    round_length = 240
end
round_start = time()
round_over = false
feed_time = -10
scores = {Red = 0, Blue = 0, Green = 0, Yellow = 0}

fn team_color(t)
    if t == "Red" then
        return {r = 196, g = 40, b = 28}
    elseif t == "Blue" then
        return {r = 13, g = 105, b = 172}
    elseif t == "Green" then
        return {r = 40, g = 127, b = 71}
    end
    return {r = 245, g = 205, b = 48}
end

-- The team with the fewest players.
fn smallest_team()
    best = TEAMS[1]
    best_count = 1000
    for t in TEAMS do
        n = 0
        for p in players() do
            if p.team == t then
                n += 1
            end
        end
        if n < best_count then
            best = t
            best_count = n
        end
    end
    return best
end

fn to_spawn(p)
    pad = find(p.team + " Spawn")
    if pad != nil then
        p.position = {x = pad.position.x, y = pad.position.y + 3.5, z = pad.position.z}
    end
end

fn say(text)
    feed = find("Feed")
    feed.text = text
    feed.visible = true
    feed_time = time()
end

on player_joined(p)
    p.team = smallest_team()
    p.shirt_color = team_color(p.team)
    p.kos = 0
    p.wipeouts = 0
    for name in TOOLS do
        tool = clone(find(name + " Template"))
        tool.name = name
        tool.parent = p
    end
    to_spawn(p)
    say(p.name + " joined " + p.team)
end

on died(p)
    p.wipeouts += 1
    killer = nil
    if p.last_hit_by != nil and p.last_hit_by != p.name then
        killer = find(p.last_hit_by)
    end
    if killer != nil and killer.team != p.team then
        killer.kos += 1
        scores[killer.team] = scores[killer.team] + 1
        say(killer.name + " knocked out " + p.name)
    else
        say(p.name + " wiped out")
    end
    p.last_hit_by = nil
end

-- Rebuilds the map from the hidden copy, and sends everyone home.
fn drop_in(obj)
    if obj.class == "part" or obj.class == "spawnlocation" then
        obj.position.y += 500
    end
    for c in obj.children do
        drop_in(c)
    end
end

fn rebuild()
    for c in find("Map").children do
        destroy(c)
    end
    for c in find("Map Template").children do
        copy = clone(c)
        drop_in(copy)
        copy.parent = find("Map")
    end
    for c in find("Projectiles").children do
        destroy(c)
    end
    for p in players() do
        to_spawn(p)
    end
end

fn winner()
    best = "Nobody"
    best_score = 0
    for t in TEAMS do
        if scores[t] > best_score then
            best = t
            best_score = scores[t]
        end
    end
    return best
end

play_music(find("Theme"))
play_sound(find("Round Horn"))

every 0.5 seconds
    -- The clock.
    left = floor(round_length - (time() - round_start))
    if left < 0 then
        left = 0
    end
    secs = left % 60
    pad = ""
    if secs < 10 then
        pad = "0"
    end
    find("Clock").text = floor(left / 60) + ":" + pad + secs

    -- Team scores, and everyone's knockouts.
    find("Scores").text = "RED " + scores.Red + "    BLUE " + scores.Blue + "    GREEN " + scores.Green + "    YELLOW " + scores.Yellow
    board = "KNOCKOUTS"
    for p in players() do
        board = board + "\n" + p.name + " (" + p.team + ")   " + p.kos
    end
    find("Board").text = board
    if time() - feed_time > 4 then
        find("Feed").visible = false
    end

    -- The end of a round: a winner, a horn, and a fresh map.
    if left == 0 and not round_over then
        round_over = true
        w = winner()
        banner = find("Banner")
        if w == "Nobody" then
            banner.text = "Round over: a draw!"
        else
            banner.text = w + " team wins the round!"
            banner.background_color = team_color(w)
        end
        banner.visible = true
        play_sound(find("Round Horn"))
        play_sound("win")
        wait(5)
        rebuild()
        scores = {Red = 0, Blue = 0, Green = 0, Yellow = 0}
        for p in players() do
            p.kos = 0
        end
        banner.visible = false
        round_start = time()
        round_over = false
        play_sound(find("Round Horn"))
    end
end
"#;

/// Shared by every weapon: cooldowns, and who counts as an enemy.
const WEAPON_HELPERS: &str = r#"
fn ready(tool, seconds)
    if tool.ready_at != nil and time() < tool.ready_at then
        return false
    end
    tool.ready_at = time() + seconds
    return true
end

fn enemy(p, other)
    return other.class == "player" and other != p and other.health > 0 and other.team != p.team
end

fn hurt(p, other, amount)
    other.health -= amount
    other.last_hit_by = p.name
    play_sound("hit")
end
"#;

const SWORD: &str = r#"
-- Chop: hits enemies in reach, in front of you.
on activated(p)
    if not ready(self, 0.45) then
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
            hurt(p, other, 35)
        end
    end
end
"#;

const ROCKET_LAUNCHER: &str = r#"
-- A rocket that flies straight until it hits something.
on activated(p)
    if not ready(self, 1.6) then
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
    r.velocity = {x = f.x * 75, y = 0, z = f.z * 75}
    play_sound("whoosh")
end
"#;

const ROCKET: &str = r#"
-- Boom on impact; blows bricks out of whatever it hits.
armed_at = time()
gone = false
fn boom()
    if gone then
        return
    end
    gone = true
    hit = explode(self.position, 7, 110)
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
-- Throw a ball that bounces around, hurting enemies it hits.
on activated(p)
    if not ready(self, 1.8) then
        return
    end
    f = p.look
    b = clone(find("Superball Ball"))
    b.name = "Superball"
    b.owner = p.name
    b.color = {r = random(60, 255), g = random(60, 255), b = random(60, 255)}
    b.position = {x = p.position.x + f.x * 3, y = p.position.y + 1.5, z = p.position.z + f.z * 3}
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = f.x * 85, y = 14, z = f.z * 85}
    play_sound("pop")
end
"#;

const BOUNCING_BALL: &str = r#"
-- Hurts each enemy once per bounce-through; disappears after a while.
hit_at = {}
on touched(other)
    if self.parent.name == "Storage" or other.class != "player" then
        return
    end
    owner = find(self.owner)
    if owner == nil or not enemy(owner, other) then
        return
    end
    last = hit_at[other.name]
    if last == nil or time() - last > 0.6 then
        hit_at[other.name] = time()
        hurt(owner, other, self.damage)
    end
end
wait(self.lifetime)
if self.parent.name != "Storage" then
    destroy(self)
end
"#;

const SLINGSHOT: &str = r#"
-- Quick pellets on an arc.
on activated(p)
    if not ready(self, 0.35) then
        return
    end
    f = p.look
    b = clone(find("Pellet Template"))
    b.name = "Pellet"
    b.owner = p.name
    b.position = {x = p.position.x + f.x * 3, y = p.position.y + 1.8, z = p.position.z + f.z * 3}
    b.anchored = false
    b.parent = find("Projectiles")
    b.velocity = {x = f.x * 95, y = 10, z = f.z * 95}
    play_sound("click")
end
"#;

const TROWEL: &str = r#"
-- Builds a wall of your team's bricks in front of you: cover, or steps up
-- somebody's tower. It crumbles away after a while.
on activated(p)
    if not ready(self, 3.5) then
        return
    end
    f = p.look
    side = {x = f.z, z = -f.x}
    cx = p.position.x + f.x * 6
    cz = p.position.z + f.z * 6
    base = p.position.y - 2.5
    wall = create("Folder", find("Projectiles"))
    wall.name = "Trowel Wall"
    for row in [0, 1, 2] do
        for col in [-1, 0, 1] do
            b = create("Part", wall)
            b.name = "Wall Brick"
            b.size = {x = 4, y = 2, z = 2}
            b.position = {x = cx + side.x * col * 4, y = base + 1 + row * 2, z = cz + side.z * col * 4}
            b.rotation = {x = 0, y = p.rotation.y, z = 0}
            b.color = p.shirt_color
            b.material = "brick"
            b.breakable = true
        end
    end
    play_sound("buy")
    wait(25)
    destroy(wall)
end
"#;

const TIMEBOMB: &str = r#"
-- Drops a bomb at your feet. Run.
on activated(p)
    if not ready(self, 3) then
        return
    end
    f = p.look
    b = clone(find("Ticking Bomb Template"))
    b.name = "Timebomb"
    b.owner = p.name
    b.position = {x = p.position.x + f.x * 2.5, y = p.position.y - 1.5, z = p.position.z + f.z * 2.5}
    b.anchored = false
    b.parent = find("Projectiles")
end
"#;

const TICKING: &str = r#"-- Blink, beep, boom. (The template in Storage just waits to be copied.)
if self.parent.name != "Storage" then
    for i in [1, 2, 3, 4, 5, 6] do
        self.color = {r = 255, g = 40, b = 40}
        play_sound("click")
        wait(0.25)
        self.color = {r = 30, g = 30, b = 34}
        wait(0.25)
    end
    hit = explode(self.position, 13, 150)
    for v in hit do
        v.last_hit_by = self.owner
    end
    destroy(self)
end
"#;

/// Spire Wars' theme song (loops as the background music).
const THEME_MP3: &[u8] = include_bytes!("../assets/strategy.mp3");

/// A short brass-like fanfare, made here as a real WAV file and stored in
/// the game as a Sound: the same path an mp3 dropped on the studio takes.
fn round_horn_wav() -> Vec<u8> {
    const RATE: u32 = 22_050;
    let notes = [(0.0, 0.18, 262.0), (0.18, 0.18, 330.0), (0.36, 0.18, 392.0), (0.54, 0.7, 523.0)];
    let len = (1.3 * RATE as f32) as usize;
    let mut s = vec![0.0f32; len];
    for (start, dur, f) in notes {
        let (a, n) = ((start * RATE as f32) as usize, (dur * RATE as f32) as usize);
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let env = (i as f32 / 300.0).min(1.0) * (1.0 - i as f32 / n as f32).powf(0.7);
            // A few harmonics of a sawtooth: bright, like a horn.
            let v: f32 = (1..6).map(|h| (t * f * h as f32 * std::f32::consts::TAU).sin() / h as f32).sum();
            if a + i < len {
                s[a + i] += v * env * 0.28;
            }
        }
    }
    let mut out = Vec::with_capacity(44 + len * 2);
    let data = (len * 2) as u32;
    for chunk in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes(),
                  &RATE.to_le_bytes(), &(RATE * 2).to_le_bytes(), &2u16.to_le_bytes(), &16u16.to_le_bytes(), b"data", &data.to_le_bytes()] {
        out.extend_from_slice(chunk);
    }
    for v in s {
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

struct B {
    dm: DataModel,
}

type Rgb = (u8, u8, u8);

impl B {
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, parent: InstanceId, name: &str, pos: Vec3, size: Vec3, color: Rgb, material: Material, shape: Shape) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = pos;
        p.size = size;
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        p.shape = shape;
        id
    }
    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.into(), value);
    }
    fn brick(&mut self, parent: InstanceId, pos: Vec3, size: Vec3, color: Rgb) -> InstanceId {
        let id = self.part(parent, "Brick", pos, size, color, Material::Brick, Shape::Block);
        self.attr(id, "breakable", Attribute::Bool(true));
        id
    }
    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }
    fn label(&mut self, class: Class, name: &str, text: &str, rect: (f32, f32, f32, f32), size: f32) -> InstanceId {
        let root = self.dm.root();
        let id = self.dm.create(class, name, root).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.to_string();
        (g.x, g.y, g.width, g.height) = rect;
        g.text_size = size;
        g.background = true;
        id
    }

    /// A team tower: hollow walls of breakable bricks in a running bond,
    /// a floor on top, battlements, and the team's spawn pad.
    fn tower(&mut self, parent: InstanceId, team: &str, color: Rgb, (cx, cz): (f32, f32), dy: f32) {
        let t = self.dm.create(Class::Folder, &format!("{team} Tower"), parent).unwrap();
        let dark = ((color.0 as f32 * 0.8) as u8, (color.1 as f32 * 0.8) as u8, (color.2 as f32 * 0.8) as u8);
        for row in 0..TOWER_ROWS {
            let y = 1.02 + row as f32 * 2.02 + dy;
            let shift = if row % 2 == 0 { 0.0 } else { 2.0 };
            let shade = if row % 4 < 2 { color } else { dark };
            // Two walls along x (full width), two along z (between them).
            for wz in [cz - 5.0, cz + 5.0] {
                for k in 0..3 {
                    let x = cx - 4.0 + k as f32 * 4.0 + shift;
                    let w = if shift > 0.0 && k == 2 { 2.0 } else { 4.0 };
                    self.brick(t, Vec3::new(x - (4.0 - w) / 2.0, y, wz), Vec3::new(w, 2.0, 2.0), shade);
                }
                if shift > 0.0 {
                    self.brick(t, Vec3::new(cx - 5.0, y, wz), Vec3::new(2.0, 2.0, 2.0), shade);
                }
            }
            for wx in [cx - 5.0, cx + 5.0] {
                for k in 0..2 {
                    let z = cz - 2.0 + k as f32 * 4.0;
                    self.brick(t, Vec3::new(wx, y, z), Vec3::new(2.0, 2.0, 4.0), shade);
                }
            }
        }
        // The top: a floor, battlements at the corners, the spawn pad.
        let top = 1.02 + TOWER_ROWS as f32 * 2.02 + dy;
        for (fx, fz) in [(-4.0, -4.0), (0.0, -4.0), (4.0, -4.0), (-4.0, 0.0), (0.0, 0.0), (4.0, 0.0), (-4.0, 4.0), (0.0, 4.0), (4.0, 4.0)] {
            self.brick(t, Vec3::new(cx + fx, top - 0.5, cz + fz), Vec3::new(4.0, 1.0, 4.0), (99, 95, 98));
        }
        for (bx, bz) in [(-5.0, -5.0), (5.0, -5.0), (-5.0, 5.0), (5.0, 5.0)] {
            self.brick(t, Vec3::new(cx + bx, top + 1.0, cz + bz), Vec3::new(2.0, 2.0, 2.0), color);
        }
        let pad = self.dm.create(Class::SpawnLocation, &format!("{team} Spawn"), t).unwrap();
        {
            let p = self.dm.part_mut(pad).unwrap();
            p.position = Vec3::new(cx, top + 0.25, cz);
            p.size = Vec3::new(6.0, 0.5, 6.0);
            p.color = Color::new(color.0, color.1, color.2);
            p.material = Material::Neon;
        }
        self.attr(pad, "team", Attribute::Str(team.into()));
        self.attr(pad, "breakable", Attribute::Bool(true));
        // A flag. (Breakable too: it comes down with the tower rather than
        // holding it up.)
        let pole = self.part(t, "Pole", Vec3::new(cx + 4.0, top + 5.0, cz + 4.0), Vec3::new(0.4, 8.0, 0.4), (220, 220, 220), Material::Metal, Shape::Cylinder);
        let flag = self.part(t, "Flag", Vec3::new(cx + 4.0, top + 7.8, cz + 5.6), Vec3::new(0.2, 2.0, 3.0), color, Material::Plastic, Shape::Block);
        self.attr(pole, "breakable", Attribute::Bool(true));
        self.attr(flag, "breakable", Attribute::Bool(true));
    }

    /// Everything that gets rebuilt each round: the towers and the cover.
    fn map(&mut self, parent: InstanceId, dy: f32) {
        for (team, color, at) in TEAMS {
            self.tower(parent, team, color, at, dy);
        }
        let cover = self.dm.create(Class::Folder, "Cover", parent).unwrap();
        for (x, z, along_x) in [(0.0, -14.0, true), (0.0, 14.0, true), (-14.0, 0.0, false), (14.0, 0.0, false)] {
            for row in 0..2 {
                for k in -1..=1 {
                    let (px, pz, size) = if along_x {
                        (x + k as f32 * 4.0, z, Vec3::new(4.0, 2.0, 2.0))
                    } else {
                        (x, z + k as f32 * 4.0, Vec3::new(2.0, 2.0, 4.0))
                    };
                    self.brick(cover, Vec3::new(px, 1.02 + row as f32 * 2.02 + dy, pz), size, (163, 162, 165));
                }
            }
        }
    }
}

/// A weapon template: its parts (pointing along +Z, the handle first),
/// grip, and script.
fn tool(b: &mut B, storage: InstanceId, name: &str, grip_up: bool, parts: &[(&str, Vec3, Vec3, Rgb, Material, Shape)], script: &str) {
    let t = b.dm.create(Class::Tool, &format!("{name} Template"), storage).unwrap();
    for (pname, pos, size, color, material, shape) in parts {
        let p = b.part(t, pname, Vec3::new(pos.x, pos.y - 300.0, pos.z), *size, *color, *material, *shape);
        b.dm.part_mut(p).unwrap().anchored = true;
    }
    if grip_up {
        b.attr(t, "grip", Attribute::Str("up".into()));
    }
    b.script(t, "Use", &format!("{WEAPON_HELPERS}{script}"));
}

pub fn spire_wars() -> DataModel {
    let mut b = B { dm: DataModel::new() };
    let root = b.dm.root();
    use Material::*;
    use Shape::*;

    // The ground (not breakable: it's what towers stand on), a plaza, trees.
    let ground = b.part(root, "Ground", Vec3::new(0.0, -1.0, 0.0), Vec3::new(260.0, 2.0, 260.0), (75, 151, 75), Grass, Block);
    let _ = ground;
    b.part(root, "Plaza", Vec3::new(0.0, 0.1, 0.0), Vec3::new(44.0, 0.2, 44.0), (170, 166, 160), Concrete, Block);
    for (x, z) in [(-40.0, 0.0), (40.0, 0.0), (0.0, -40.0), (0.0, 40.0)] {
        b.part(root, "Path", Vec3::new(x * 1.4, 0.08, z * 1.4), Vec3::new(if z == 0.0 { 70.0 } else { 6.0 }, 0.16, if z == 0.0 { 6.0 } else { 70.0 }), (160, 156, 150), Concrete, Block);
    }
    let decor = b.dm.create(Class::Folder, "Decor", root).unwrap();
    for (x, z, h) in [(-110.0, 0.0, 8.0), (110.0, 5.0, 7.0), (0.0, -110.0, 7.5), (5.0, 110.0, 8.0), (-40.0, -105.0, 6.5), (105.0, 40.0, 7.0), (-105.0, 45.0, 6.0), (45.0, -108.0, 7.0)] {
        let t = b.dm.create(Class::Model, "Tree", decor).unwrap();
        b.part(t, "Trunk", Vec3::new(x, h / 2.0, z), Vec3::new(1.6, h, 1.6), (105, 64, 40), Wood, Cylinder);
        b.part(t, "Leaves", Vec3::new(x, h + 1.5, z), Vec3::new(6.0, 6.0, 6.0), (75, 151, 75), Grass, Ball);
    }
    for (x, z) in [(-20.0, -20.0), (20.0, -20.0), (-20.0, 20.0), (20.0, 20.0)] {
        b.part(decor, "Lamp Post", Vec3::new(x, 3.0, z), Vec3::new(0.6, 6.0, 0.6), (27, 42, 53), Metal, Cylinder);
        b.part(decor, "Lamp", Vec3::new(x, 6.5, z), Vec3::new(1.4, 1.4, 1.4), (255, 230, 160), Neon, Ball);
    }

    // The map, and a hidden copy of it to rebuild from after each round.
    let map = b.dm.create(Class::Folder, "Map", root).unwrap();
    b.map(map, 0.0);
    let storage = b.dm.create(Class::Folder, "Storage", root).unwrap();
    let template = b.dm.create(Class::Folder, "Map Template", storage).unwrap();
    b.map(template, -TEMPLATE_DROP);
    b.dm.create(Class::Folder, "Projectiles", root).unwrap();

    // The weapons.
    let v = Vec3::new;
    tool(&mut b, storage, "Sword", true, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.35, 0.35, 1.2), (105, 64, 40), Wood, Block),
        ("Guard", v(0.0, 0.0, 0.7), v(1.4, 0.3, 0.3), (245, 205, 48), Metal, Block),
        ("Blade", v(0.0, 0.0, 2.8), v(0.2, 0.55, 3.8), (205, 210, 220), Metal, Block),
    ], SWORD);
    tool(&mut b, storage, "Rocket Launcher", false, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.4, 0.9, 0.5), (27, 42, 53), Metal, Block),
        ("Tube", v(0.0, 0.55, 0.6), v(0.9, 0.9, 3.4), (70, 90, 60), Metal, Cylinder),
        ("Tip", v(0.0, 0.55, 2.35), v(1.0, 1.0, 0.25), (255, 120, 20), Neon, Cylinder),
    ], ROCKET_LAUNCHER);
    tool(&mut b, storage, "Superball", false, &[
        ("Handle", v(0.0, 0.0, 0.3), v(1.6, 1.6, 1.6), (107, 50, 124), Neon, Ball),
    ], SUPERBALL);
    tool(&mut b, storage, "Slingshot", false, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.3, 1.2, 0.3), (105, 64, 40), Wood, Block),
        ("Fork L", v(-0.4, 0.8, 0.0), v(0.25, 0.8, 0.25), (105, 64, 40), Wood, Block),
        ("Fork R", v(0.4, 0.8, 0.0), v(0.25, 0.8, 0.25), (105, 64, 40), Wood, Block),
        ("Band", v(0.0, 1.1, 0.0), v(0.9, 0.1, 0.1), (196, 40, 28), Plastic, Block),
    ], SLINGSHOT);
    tool(&mut b, storage, "Trowel", false, &[
        ("Handle", v(0.0, 0.0, 0.0), v(0.3, 0.3, 1.0), (105, 64, 40), Wood, Block),
        ("Blade", v(0.0, -0.1, 1.2), v(1.1, 0.1, 1.4), (200, 205, 215), Metal, Block),
    ], TROWEL);
    tool(&mut b, storage, "Timebomb", false, &[
        ("Handle", v(0.0, 0.0, 0.3), v(1.1, 1.1, 1.1), (30, 30, 34), Metal, Ball),
        ("Fuse", v(0.0, 0.7, 0.3), v(0.2, 0.5, 0.2), (255, 60, 40), Neon, Cylinder),
    ], TIMEBOMB);

    // What the weapons make (cloned, so they bring their scripts).
    let rocket = b.part(storage, "Rocket Template", v(20.0, -300.0, 0.0), v(0.7, 0.7, 2.2), (220, 220, 220), Metal, Cylinder);
    b.dm.part_mut(rocket).unwrap().floating = true;
    b.dm.part_mut(rocket).unwrap().anchored = true;
    b.script(rocket, "Fly", ROCKET);
    let ball = b.part(storage, "Superball Ball", v(24.0, -300.0, 0.0), v(2.0, 2.0, 2.0), (107, 50, 124), Neon, Ball);
    {
        let p = b.dm.part_mut(ball).unwrap();
        p.bounce = 0.95;
        p.anchored = true;
    }
    b.attr(ball, "damage", Attribute::Num(25.0));
    b.attr(ball, "lifetime", Attribute::Num(6.0));
    b.script(ball, "Bounce", &format!("{WEAPON_HELPERS}{BOUNCING_BALL}"));
    let pellet = b.part(storage, "Pellet Template", v(28.0, -300.0, 0.0), v(0.7, 0.7, 0.7), (60, 60, 64), Metal, Ball);
    {
        let p = b.dm.part_mut(pellet).unwrap();
        p.bounce = 0.5;
        p.anchored = true;
    }
    b.attr(pellet, "damage", Attribute::Num(12.0));
    b.attr(pellet, "lifetime", Attribute::Num(3.0));
    b.script(pellet, "Hit", &format!("{WEAPON_HELPERS}{BOUNCING_BALL}"));
    let ticking = b.part(storage, "Ticking Bomb Template", v(32.0, -300.0, 0.0), v(1.6, 1.6, 1.6), (30, 30, 34), Metal, Ball);
    b.dm.part_mut(ticking).unwrap().anchored = true;
    b.script(ticking, "Tick", TICKING);

    // The theme song: an mp3 file stored in the game as a Sound.
    let theme = b.dm.create(Class::Sound, "Theme", root).unwrap();
    *b.dm.sound_mut(theme).unwrap() = SoundProps::from_bytes("mp3", THEME_MP3);
    b.dm.sound_mut(theme).unwrap().volume = 0.6;

    // A custom sound, made as a real WAV file.
    let horn = b.dm.create(Class::Sound, "Round Horn", root).unwrap();
    *b.dm.sound_mut(horn).unwrap() = SoundProps::from_bytes("wav", &round_horn_wav());

    // The GUI.
    let title = b.label(Class::TextLabel, "Title", "SPIRE WARS", (0.4, 0.012, 0.2, 0.055), 28.0);
    b.dm.gui_mut(title).unwrap().text_color = Color::new(245, 205, 48);
    b.label(Class::TextLabel, "Clock", "4:00", (0.61, 0.012, 0.07, 0.055), 24.0);
    b.label(Class::TextLabel, "Scores", "", (0.25, 0.075, 0.5, 0.045), 18.0);
    let board = b.label(Class::TextLabel, "Board", "KNOCKOUTS", (0.8, 0.14, 0.19, 0.26), 16.0);
    let _ = board;
    let feed = b.label(Class::TextLabel, "Feed", "", (0.3, 0.135, 0.4, 0.05), 20.0);
    {
        let g = b.dm.gui_mut(feed).unwrap();
        g.background_color = Color::new(120, 20, 20);
        g.visible = false;
    }
    let banner = b.label(Class::TextLabel, "Banner", "", (0.25, 0.35, 0.5, 0.1), 40.0);
    b.dm.gui_mut(banner).unwrap().visible = false;
    b.label(Class::TextLabel, "Help", "1 Sword   2 Rocket   3 Superball   4 Slingshot   5 Trowel   6 Timebomb", (0.2, 0.93, 0.6, 0.045), 16.0);

    b.script(root, "Game", GAME);
    b.dm
}
