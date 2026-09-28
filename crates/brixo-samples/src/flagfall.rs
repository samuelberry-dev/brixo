//! Flagfall: capture the flag. Red vs Blue, two castles facing each other
//! across a river, three ways across: the Bridge down the middle, the Ruins
//! on one flank (stepping stones and crumbling cover), the Tunnel on the
//! other (under the river, surfacing beside the enemy castle).
//!
//! Grab the enemy flag and carry it to your own flag stand. You can only
//! score while your own flag is home, so defending matters as much as
//! attacking. First to 3 captures, or the most after 8 minutes; tied at
//! the end means sudden death.
//!
//! The rules live in the Game script below (plain Rovik, open it in the
//! studio). The engine features they lean on: `carried_by` (a part rides
//! along with a player), `beacon` (a marker you can see through walls),
//! team spawns, breakable bricks, and the standard gear kit.

use brixo_core::{Attribute, Class, Color, DataModel, InstanceId, Material, Shape, SoundProps, Vec3};

use crate::{gears, synth};

pub const TEAMS: [(&str, (u8, u8, u8)); 2] = [("Red", (196, 40, 28)), ("Blue", (13, 105, 172))];

/// Which way each team's castle lies along X (Red west, Blue east).
fn side_of(team: &str) -> f32 {
    if team == "Red" { -1.0 } else { 1.0 }
}

/// The flag stand, this far from the middle.
pub const STAND_A: f32 = 120.0;
/// Where the flag pole's centre sits, above the top of its stand.
pub const POLE_UP: f32 = 3.5;
/// Where the cloth sits relative to the pole (up, and back).
const CLOTH_UP: f32 = 2.3;
const CLOTH_BACK: f32 = 1.7;
/// A carried flag rides on the carrier's back, this far up and behind.
const CARRY_UP: f32 = 1.2;
const CARRY_BACK: f32 = 0.9;
/// The ruins are rebuilt from a hidden copy this far below the map.
const TEMPLATE_DROP: f32 = 500.0;

const GAME: &str = r#"-- Flagfall: capture the flag.
-- Grab the enemy flag, bring it to your own stand while your flag is home.
TEAMS = ["Red", "Blue"]
GEAR = ["Sword", "Slingshot", "Rocket Launcher", "Superball", "Trowel", "Paintball Gun"]
-- Top right: everyone by team, with their captures, returns and knockouts.
leaderboard("captures", "returns", "kos")

-- Tunables. (Tests shorten the match by setting match_seconds on the
-- Workspace; caps_to_win works the same way.)
MATCH_LENGTH = find("Workspace").match_seconds
if MATCH_LENGTH == nil then
    MATCH_LENGTH = 480
end
CAPS_TO_WIN = find("Workspace").caps_to_win
if CAPS_TO_WIN == nil then
    CAPS_TO_WIN = 3
end
RETURN_TIME = 10
WALK_SPEED = 16
CARRY_SPEED = 11.5
WADE = 0.6

POLES = {Red = find("Red Flag Pole"), Blue = find("Blue Flag Pole")}
CLOTHS = {Red = find("Red Flag"), Blue = find("Blue Flag")}
STANDS = {Red = find("Red Flag Stand"), Blue = find("Blue Flag Stand")}
MUSIC_CALM = find("Theme")
MUSIC_INTENSE = find("Theme Intense")

score = {Red = 0, Blue = 0}
flag_state = {Red = "home", Blue = "home"}
flag_carrier = {Red = "", Blue = ""}
dropped_at = {Red = 0, Blue = 0}
match_start = time()
sudden_death = false
match_over = false
feed_time = -10
music_now = ""

fn other_team(team)
    if team == "Red" then
        return "Blue"
    end
    return "Red"
end

fn team_color(team)
    if team == "Red" then
        return {r = 196, g = 40, b = 28}
    elseif team == "Blue" then
        return {r = 13, g = 105, b = 172}
    end
    return {r = 60, g = 60, b = 66}
end

fn say(text, team)
    feed = find("Feed")
    feed.text = text
    feed.background_color = team_color(team)
    feed.visible = true
    feed_time = time()
end

fn smallest_team()
    red_n = 0
    blue_n = 0
    for someone in players() do
        if someone.team == "Red" then
            red_n += 1
        elseif someone.team == "Blue" then
            blue_n += 1
        end
    end
    if blue_n < red_n then
        return "Blue"
    end
    return "Red"
end

fn to_spawn(p)
    pad = find(p.team + " Spawn")
    p.position = {x = pad.position.x + random(-2, 2), y = pad.position.y + 3, z = pad.position.z + random(-2, 2)}
end

-- --- The flags ---------------------------------------------------------

-- Plants a flag upright with its pole centred at (x, y, z).
fn plant(team, x, y, z)
    pole = POLES[team]
    cloth = CLOTHS[team]
    pole.carried_by = nil
    cloth.carried_by = nil
    pole.position = {x = x, y = y, z = z}
    pole.rotation = {x = 0, y = 0, z = 0}
    cloth.position = {x = x, y = y + CLOTH_UP, z = z - CLOTH_BACK}
    cloth.rotation = {x = 0, y = 0, z = 0}
end

fn send_home(team)
    stand = STANDS[team]
    plant(team, stand.position.x, stand.position.y + stand.size.y / 2 + 0.1 + POLE_UP, stand.position.z)
    flag_state[team] = "home"
    flag_carrier[team] = ""
    for someone in players() do
        if someone.carrying == team then
            someone.carrying = nil
        end
    end
end

fn pick_up(team, p)
    flag_state[team] = "carried"
    flag_carrier[team] = p.name
    p.carrying = team
    POLES[team].carried_by = p.name
    CLOTHS[team].carried_by = p.name
    say(p.name + " has the " + team + " flag!", p.team)
    play_sound(find("Flag Taken"))
end

-- Drops a flag where its carrier fell. Off the edge of the world, it goes
-- straight home instead.
fn drop_flag(team, where)
    if where.y < -40 then
        send_home(team)
        say("The " + team + " flag fell off the world and went home", team)
        play_sound(find("Flag Returned"))
        return
    end
    plant(team, where.x, where.y + 1, where.z)
    flag_state[team] = "dropped"
    flag_carrier[team] = ""
    dropped_at[team] = time()
end

fn capture(p)
    taken = p.carrying
    p.carrying = nil
    p.captures += 1
    score[p.team] = score[p.team] + 1
    send_home(taken)
    say(p.name + " captured the " + taken + " flag!", p.team)
    play_sound(find("Capture"))
    fireworks(STANDS[p.team].position, team_color(p.team))
    if score[p.team] >= CAPS_TO_WIN or sudden_death then
        end_match(p.team)
    end
end

-- A burst of sparks over a flag stand.
fn fireworks(at, col)
    k = 0
    while k < 28 do
        spark = create("Part", find("Effects"))
        spark.name = "Spark"
        spark.shape = "ball"
        spark.size = {x = 0.9, y = 0.9, z = 0.9}
        spark.material = "neon"
        spark.can_collide = false
        if k % 3 == 0 then
            spark.color = {r = 255, g = 230, b = 120}
        else
            spark.color = col
        end
        spark.position = {x = at.x, y = at.y + 9, z = at.z}
        spark.anchored = false
        spark.velocity = {x = random(-28, 28), y = random(18, 42), z = random(-28, 28)}
        spark.dies_at = time() + 1.8
        k += 1
    end
end

-- What one player can do this moment: pick up, return or capture.
fn check_player(p)
    for team in TEAMS do
        if flag_state[team] != "carried" then
            fp = POLES[team].position
            fdx = fp.x - p.position.x
            fdz = fp.z - p.position.z
            fdy = fp.y - 1 - p.position.y
            if fdx * fdx + fdz * fdz < 16 and abs(fdy) < 4 then
                if team != p.team and p.carrying == nil then
                    pick_up(team, p)
                elseif team == p.team and flag_state[team] == "dropped" then
                    send_home(team)
                    p.returns += 1
                    say(p.name + " returned the " + team + " flag", team)
                    play_sound(find("Flag Returned"))
                end
            end
        end
    end
    if p.carrying != nil and flag_state[p.team] == "home" then
        sp = STANDS[p.team].position
        sdx = sp.x - p.position.x
        sdz = sp.z - p.position.z
        if sdx * sdx + sdz * sdz < 25 and abs(p.position.y - sp.y) < 6 then
            capture(p)
        end
    end
end

-- --- The match -----------------------------------------------------------

fn drop_in(obj)
    if obj.class == "part" then
        obj.position.y += 500
    end
    for c in obj.children do
        drop_in(c)
    end
end

fn reset_match()
    for c in find("Ruins").children do
        destroy(c)
    end
    for c in find("Ruins Template").children do
        copy = clone(c)
        drop_in(copy)
        copy.parent = find("Ruins")
    end
    for c in find("Projectiles").children do
        destroy(c)
    end
    for team in TEAMS do
        send_home(team)
        score[team] = 0
    end
    for someone in players() do
        someone.captures = 0
        someone.returns = 0
        someone.kos = 0
        someone.flag_time = 0
        someone.health = someone.max_health
        to_spawn(someone)
    end
    sudden_death = false
    match_start = time()
    find("Banner").visible = false
    match_over = false
    play_sound(find("Round Horn"))
    say("Capture the flag! First to " + CAPS_TO_WIN + " wins", "")
end

fn end_match(winner)
    match_over = true
    banner = find("Banner")
    if winner == "" then
        banner.text = "It's a draw!"
        banner.background_color = team_color("")
    else
        banner.text = winner + " team wins!"
        banner.background_color = team_color(winner)
        fireworks(STANDS[winner].position, team_color(winner))
    end
    banner.visible = true
    play_sound(find("Victory"))
    wait(8)
    reset_match()
end

-- --- Players -------------------------------------------------------------

on player_joined(p)
    p.team = smallest_team()
    p.shirt_color = team_color(p.team)
    if p.team == "Red" then
        p.pants_color = {r = 90, g = 24, b = 20}
    else
        p.pants_color = {r = 20, g = 44, b = 90}
    end
    p.captures = 0
    p.returns = 0
    p.kos = 0
    p.wipeouts = 0
    p.flag_time = 0
    for name in GEAR do
        tool = clone(find(name + " Template"))
        tool.name = name
        tool.parent = p
    end
    tip = clone(find("Tip Template"))
    tip.name = "Tip"
    tip.parent = p
    to_spawn(p)
    say(p.name + " joined " + p.team, p.team)
end

on died(p)
    p.wipeouts += 1
    if p.carrying != nil then
        lost = p.carrying
        p.carrying = nil
        if not match_over then
            drop_flag(lost, p.position)
            say(p.name + " dropped the " + lost + " flag!", lost)
        end
    end
    killer = nil
    if p.last_hit_by != nil and p.last_hit_by != p.name then
        killer = find(p.last_hit_by)
    end
    if killer != nil and killer.class == "player" and killer.team != p.team then
        killer.kos += 1
    end
    p.last_hit_by = nil
end

-- --- Every tenth of a second: the rules ----------------------------------

play_music(MUSIC_CALM)
music_now = "calm"
reset_match()

every 0.1 seconds
    now = time()

    if not match_over then
        -- Carriers who left the game: their flag goes home.
        for team in TEAMS do
            if flag_state[team] == "carried" then
                still_here = false
                for someone in players() do
                    if someone.name == flag_carrier[team] and someone.carrying == team then
                        still_here = true
                    end
                end
                if not still_here then
                    send_home(team)
                end
            end
            if flag_state[team] == "dropped" and now - dropped_at[team] > RETURN_TIME then
                send_home(team)
                say("The " + team + " flag returned home", team)
                play_sound(find("Flag Returned"))
            end
        end

        for p in players() do
            if p.health > 0 and p.team != nil then
                check_player(p)
            end
        end
    end

    -- Time's up: the leader wins; a tie goes to sudden death.
    if not match_over and not sudden_death and now - match_start >= MATCH_LENGTH then
        if score.Red > score.Blue then
            end_match("Red")
        elseif score.Blue > score.Red then
            end_match("Blue")
        else
            sudden_death = true
            say("SUDDEN DEATH! Next capture wins", "")
            play_sound(find("Round Horn"))
        end
    end
end

-- A second clock for everything on screen, so the scores, flags and
-- music stay current even while the rules above are paused for the
-- end-of-match banner.
every 0.1 seconds
    now = time()

    -- Sparks burn out.
    for fx in find("Effects").children do
        if fx.dies_at != nil and now > fx.dies_at then
            destroy(fx)
        end
    end

    -- Healing: out of the fight for five seconds, you slowly heal.
    for p in players() do
        if p.health > 0 then
            if p.last_health != nil and p.health < p.last_health then
                p.hurt_at = now
            end
            if p.health < p.max_health and (p.hurt_at == nil or now - p.hurt_at > 5) then
                p.health = min(p.max_health, p.health + 0.8)
            end
        end
        p.last_health = p.health
    end

    -- Speed: carrying a flag slows you down; so does wading the river.
    for p in players() do
        if p.team == nil then
            continue
        end
        speed = WALK_SPEED
        if p.carrying != nil then
            speed = CARRY_SPEED
            p.flag_time += 0.1
        end
        if abs(p.position.x) < 10 and p.position.y < 0.5 and p.position.y > -6 then
            speed = speed * WADE
        end
        if p.walk_speed != speed then
            p.walk_speed = speed
        end
    end

    -- The clock.
    left = floor(MATCH_LENGTH - (now - match_start))
    if left < 0 then
        left = 0
    end
    if sudden_death then
        find("Clock").text = "SUDDEN DEATH"
        find("Clock").text_size = 15
    else
        secs = left % 60
        zero = ""
        if secs < 10 then
            zero = "0"
        end
        find("Clock").text = floor(left / 60) + ":" + zero + secs
        find("Clock").text_size = 26
    end

    -- The scoreboard, and each flag's status.
    find("Red Score").text = "RED  " + score.Red
    find("Blue Score").text = score.Blue + "  BLUE"
    for team in TEAMS do
        status = find(team + " Flag Status")
        cloth = CLOTHS[team]
        if flag_state[team] == "home" then
            status.text = team + " flag: home"
            cloth.beacon_text = team + " flag"
        elseif flag_state[team] == "carried" then
            status.text = team + " flag: TAKEN by " + flag_carrier[team]
            cloth.beacon_text = team + " flag: " + flag_carrier[team]
        else
            back_in = RETURN_TIME - floor(now - dropped_at[team])
            status.text = team + " flag: DROPPED (" + back_in + ")"
            cloth.beacon_text = team + " flag: dropped (" + back_in + ")"
        end
    end
    if now - feed_time > 4 then
        find("Feed").visible = false
    end

    -- Each player's own hint.
    for p in players() do
        for c in p.children do
            if c.name == "Tip" then
                if match_over or p.team == nil then
                    c.visible = false
                elseif p.carrying != nil and flag_state[p.team] != "home" then
                    c.text = "You have the flag! Your own flag must be home to score"
                    c.visible = true
                elseif p.carrying != nil then
                    c.text = "You have the flag! Run it back to your stand"
                    c.visible = true
                elseif flag_state[p.team] == "carried" then
                    c.text = "Your flag is taken! Stop " + flag_carrier[p.team]
                    c.visible = true
                elseif flag_state[p.team] == "dropped" then
                    c.text = "Your flag is down! Touch it to send it home"
                    c.visible = true
                else
                    c.visible = false
                end
            end
        end
    end

    -- Music: calm while both flags are home, intense while either is out.
    wanted = "calm"
    if sudden_death or flag_state.Red != "home" or flag_state.Blue != "home" then
        wanted = "intense"
    end
    if wanted != music_now then
        music_now = wanted
        if wanted == "calm" then
            play_music(MUSIC_CALM)
        else
            play_music(MUSIC_INTENSE)
        end
    end
end
"#;

type Rgb = (u8, u8, u8);

struct B {
    dm: DataModel,
}

/// A box from ranges: `a` runs away from the middle toward `side`'s castle
/// (so one description builds both mirrored castles), `y` up, `z` across.
#[derive(Clone, Copy)]
struct R {
    a: (f32, f32),
    y: (f32, f32),
    z: (f32, f32),
}

fn r(a: (f32, f32), y: (f32, f32), z: (f32, f32)) -> R {
    R { a, y, z }
}

fn shade(c: Rgb, f: f32) -> Rgb {
    ((c.0 as f32 * f).min(255.0) as u8, (c.1 as f32 * f).min(255.0) as u8, (c.2 as f32 * f).min(255.0) as u8)
}

/// A small deterministic hash, for "random" variety that's the same every build.
fn hash(n: i32) -> f32 {
    let mut x = (n as u32).wrapping_mul(2_654_435_761) ^ 0x9e37_79b9;
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    (x % 10_000) as f32 / 10_000.0
}

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

    /// A block filling `b` on `side` (-1 west, +1 east).
    fn boxr(&mut self, parent: InstanceId, name: &str, side: f32, b: R, color: Rgb, material: Material) -> InstanceId {
        let (x0, x1) = (side * b.a.0, side * b.a.1);
        let pos = Vec3::new((x0 + x1) / 2.0, (b.y.0 + b.y.1) / 2.0, (b.z.0 + b.z.1) / 2.0);
        let size = Vec3::new((x1 - x0).abs(), b.y.1 - b.y.0, b.z.1 - b.z.0);
        self.part(parent, name, pos, size, color, material, Shape::Block)
    }

    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.into(), value);
    }

    fn breakable(&mut self, id: InstanceId) {
        self.attr(id, "breakable", Attribute::Bool(true));
    }

    fn folder(&mut self, parent: InstanceId, name: &str) -> InstanceId {
        self.dm.create(Class::Folder, name, parent).unwrap()
    }

    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }

    fn sound(&mut self, parent: InstanceId, name: &str, wav: Vec<u8>, volume: f32) {
        let s = self.dm.create(Class::Sound, name, parent).unwrap();
        *self.dm.sound_mut(s).unwrap() = SoundProps::from_bytes("wav", &wav);
        self.dm.sound_mut(s).unwrap().volume = volume;
    }

    fn label(&mut self, parent: InstanceId, name: &str, text: &str, rect: (f32, f32, f32, f32), size: f32, bg: Rgb) -> InstanceId {
        let id = self.dm.create(Class::TextLabel, name, parent).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.to_string();
        (g.x, g.y, g.width, g.height) = rect;
        g.text_size = size;
        g.background = true;
        g.background_color = Color::new(bg.0, bg.1, bg.2);
        id
    }

    // --- The land ---------------------------------------------------------

    fn ground(&mut self, map: InstanceId) {
        use Material::*;
        let grass = (82, 148, 70);
        // Each side's ground is a thick slab with a hole where the tunnel's
        // stairs come up (a 60..95 from the middle, z 38..50).
        for side in [-1.0, 1.0] {
            for b in [
                r((10.0, 152.0), (-4.0, 0.0), (-97.0, 38.0)),
                r((10.0, 60.0), (-4.0, 0.0), (38.0, 50.0)),
                r((95.0, 152.0), (-4.0, 0.0), (38.0, 50.0)),
                r((10.0, 152.0), (-4.0, 0.0), (50.0, 97.0)),
            ] {
                self.boxr(map, "Ground", side, b, grass, Grass);
            }
            // The edge of the world: a stone wall all the way round.
            let stone = (128, 124, 118);
            self.boxr(map, "Boundary", side, r((150.0, 153.0), (0.0, 8.0), (-97.0, 97.0)), stone, Concrete);
            for z in [(-97.0, -94.0), (94.0, 97.0)] {
                self.boxr(map, "Boundary", side, r((0.0, 153.0), (-4.0, 8.0), z), stone, Concrete);
            }
        }

        // The river, north to south down the middle: a stony bed, and
        // water you wade through (slowly) rather than swim.
        let river = self.folder(map, "River");
        self.part(river, "Riverbed", Vec3::new(0.0, -3.5, 0.0), Vec3::new(20.0, 1.0, 188.0), (150, 138, 110), Concrete, Shape::Block);
        let water = self.part(river, "Water", Vec3::new(0.0, -1.8, 0.0), Vec3::new(20.0, 2.4, 188.0), (58, 138, 214), Plastic, Shape::Block);
        {
            let p = self.dm.part_mut(water).unwrap();
            p.transparency = 0.2;
            p.can_collide = false;
        }
        // Reeds and rocks along the banks.
        for i in 0..24 {
            let z = -90.0 + i as f32 * 7.7 + hash(i) * 3.0;
            if (-12.0..12.0).contains(&z) || (-52.0..-36.0).contains(&z) || (34.0..54.0).contains(&z) {
                continue;
            }
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let x = side * (10.6 + hash(i + 50) * 1.2);
            self.part(river, "Reeds", Vec3::new(x, 1.0, z), Vec3::new(0.6, 2.0, 1.6), (96, 128, 60), Grass, Shape::Block);
            if i % 3 == 0 {
                self.part(river, "Rock", Vec3::new(side * (6.0 + hash(i + 7) * 3.0), -2.6, z + 2.0), Vec3::new(2.4, 1.6, 2.0), (118, 114, 108), Concrete, Shape::Block);
            }
        }
    }

    /// The Bridge: the straight, exposed way across the middle.
    fn bridge(&mut self, map: InstanceId) {
        use Material::*;
        let f = self.folder(map, "Bridge");
        let stone = (170, 164, 150);
        let dark = (132, 126, 116);
        // The deck, and solid abutments on each bank.
        self.boxr(f, "Deck", 1.0, r((-24.0, 24.0), (3.0, 4.0), (-7.0, 7.0)), stone, Concrete);
        for side in [-1.0, 1.0] {
            self.boxr(f, "Abutment", side, r((10.0, 24.0), (0.0, 3.0), (-7.0, 7.0)), dark, Brick);
            // Steps down to the ground.
            for k in 0..3 {
                let a0 = 24.0 + k as f32 * 2.0;
                self.boxr(f, "Step", side, r((a0, a0 + 2.0), (0.0, 3.0 - k as f32), (-7.0, 7.0)), stone, Concrete);
            }
            // Piers in the river, leaving a way through underneath.
            for z in [(-7.0, -4.0), (4.0, 7.0)] {
                self.boxr(f, "Pier", side, r((4.5, 7.5), (-3.0, 3.0), z), dark, Brick);
            }
            // Lanterns on posts at each end.
            for z in [-7.5, 7.5] {
                self.part(f, "Lamp Post", Vec3::new(side * 23.0, 6.0, z), Vec3::new(0.8, 4.0, 0.8), (40, 40, 44), Metal, Shape::Cylinder);
                self.part(f, "Lamp", Vec3::new(side * 23.0, 8.4, z), Vec3::new(1.2, 1.2, 1.2), (255, 214, 140), Neon, Shape::Ball);
            }
        }
        // Low parapets: cover from the river, not from the other bank.
        for z in [(-8.0, -7.0), (7.0, 8.0)] {
            self.boxr(f, "Parapet", 1.0, r((-24.0, 24.0), (3.0, 5.5), z), dark, Brick);
        }
    }

    /// One bank's half of the Ruins: an old broken wall, an archway, a
    /// tumbled corner, rubble. All breakable, all rebuilt every match.
    fn ruins_half(&mut self, parent: InstanceId, side: f32, dy: f32) {
        let tones = [(150, 142, 128), (136, 129, 116), (164, 157, 142)];
        let mut n = if side < 0.0 { 0 } else { 1000 };
        let mut brick = |b: &mut B, a: f32, y: f32, z: f32, along_a: bool| {
            n += 1;
            let tone = tones[(hash(n) * 3.0) as usize % 3];
            let size = if along_a { (2.0, 1.0) } else { (1.0, 2.0) };
            let id = b.boxr(parent, "Ruin Brick", side, r((a - size.0, a + size.0), (y + dy, y + 2.0 + dy), (z - size.1, z + size.1)), tone, Material::Brick);
            b.breakable(id);
        };
        // The long wall, jagged along its top.
        // (A breach in the middle lines up with the stepping stones.)
        let heights = [4, 5, 3, 0, 5, 6, 3];
        for (k, h) in heights.iter().enumerate() {
            for row in 0..*h {
                brick(self, 18.0, row as f32 * 2.0, -58.0 + k as f32 * 4.0, false);
            }
        }
        // An archway: two columns and what's left of the lintel.
        for z in [-52.0, -42.0] {
            for row in 0..5 {
                let id = self.boxr(parent, "Ruin Column", side, r((29.0, 31.0), (row as f32 * 2.0 + dy, row as f32 * 2.0 + 2.0 + dy), (z - 1.0, z + 1.0)), (158, 150, 136), Material::Concrete);
                self.breakable(id);
            }
        }
        for z in [-50.0, -46.0] {
            brick(self, 30.0, 10.0, z, false);
        }
        // A tumbled corner.
        for (k, h) in [3, 2, 1].iter().enumerate() {
            for row in 0..*h {
                brick(self, 38.0 + k as f32 * 4.0, row as f32 * 2.0, -34.0, true);
            }
        }
        for (k, h) in [3, 1].iter().enumerate() {
            for row in 0..*h {
                brick(self, 44.0, row as f32 * 2.0, -30.0 + k as f32 * 4.0 + 2.0, false);
            }
        }
        // Rubble.
        for (a, z, along) in [(24.0, -36.0, true), (26.0, -62.0, false), (35.0, -58.0, true), (40.0, -39.0, false), (22.0, -48.0, true)] {
            brick(self, a, 0.0, z, along);
        }
    }

    fn ruins(&mut self, parent: InstanceId, dy: f32) {
        for side in [-1.0, 1.0] {
            self.ruins_half(parent, side, dy);
        }
    }

    /// The ford by the Ruins: stepping stones just above the water, and
    /// a fallen column on each bank (not breakable: permanent cover).
    fn ford(&mut self, map: InstanceId) {
        let f = self.folder(map, "Ford");
        for (k, x) in [-7.5f32, -2.5, 2.5, 7.5].iter().enumerate() {
            let z = -45.0 + if k % 2 == 0 { 1.0 } else { -1.0 };
            self.part(f, "Stepping Stone", Vec3::new(*x, -1.65, z), Vec3::new(3.6, 2.7, 4.2), (140, 136, 128), Material::Concrete, Shape::Block);
        }
        for side in [-1.0f32, 1.0] {
            let c = self.part(f, "Fallen Column", Vec3::new(side * 36.0, 1.0, -55.0), Vec3::new(2.0, 9.0, 2.0), (158, 150, 136), Material::Concrete, Shape::Cylinder);
            self.dm.part_mut(c).unwrap().rotation = Vec3::new(0.0, 20.0 * side, 90.0);
        }
    }

    /// The Tunnel: under the river from bank to bank, with stairs down at
    /// each end by the castles.
    fn tunnel(&mut self, map: InstanceId) {
        use Material::*;
        let f = self.folder(map, "Tunnel");
        let stone = (112, 106, 98);
        let dark = (84, 80, 74);
        // Floor and roof under the middle (and under the river).
        self.boxr(f, "Tunnel Floor", 1.0, r((-60.0, 60.0), (-15.0, -14.0), (37.0, 51.0)), dark, Concrete);
        self.boxr(f, "Tunnel Roof", 1.0, r((-60.0, 60.0), (-6.0, -4.0), (37.0, 51.0)), stone, Brick);
        // The side walls, the whole length, up to ground level.
        for z in [(36.0, 38.0), (50.0, 52.0)] {
            self.boxr(f, "Tunnel Wall", 1.0, r((-95.0, 95.0), (-15.0, -4.0), z), stone, Brick);
        }
        for side in [-1.0, 1.0] {
            // Stairs: 14 steps of one stud, up to the surface.
            for k in 1..=14 {
                let a0 = 60.0 + (k - 1) as f32 * 2.5;
                let top = -14.0 + k as f32;
                self.boxr(f, "Tunnel Step", side, r((a0, a0 + 2.5), (-15.0, top), (38.0, 50.0)), if k % 2 == 0 { stone } else { dark }, Concrete);
            }
            // Walls beside the stairs, rising a little above the ground.
            for z in [(36.0, 38.0), (50.0, 52.0)] {
                self.boxr(f, "Stair Wall", side, r((60.0, 95.0), (-4.0, 1.5), z), stone, Brick);
            }
            // A low wall where the tunnel roof ends, so nobody walks off
            // the edge into the stairwell by accident.
            self.boxr(f, "Stair Wall", side, r((59.0, 60.0), (-4.0, 1.5), (36.0, 52.0)), stone, Brick);
            // A gateway over the top of the stairs: two pillars and a
            // lintel, a landmark you can see from the castle.
            for z in [(35.5, 38.0), (50.0, 52.5)] {
                self.boxr(f, "Tunnel Gate", side, r((93.0, 96.0), (0.0, 8.0), z), dark, Brick);
            }
            self.boxr(f, "Tunnel Gate", side, r((93.0, 96.0), (8.0, 10.0), (35.5, 52.5)), dark, Brick);
            self.part(f, "Lamp", Vec3::new(side * 96.4, 7.0, 44.0), Vec3::new(1.0, 1.0, 1.0), (255, 190, 110), Neon, Shape::Ball);
        }
        // Lamps along the walls, so the tunnel is a place, not a black hole.
        for i in 0..11 {
            let x = -75.0 + i as f32 * 15.0;
            let y = if x.abs() < 60.0 { -8.5 } else { -14.0 + (x.abs() - 60.0) / 2.5 + 5.5 };
            for z in [38.2, 49.8] {
                self.part(f, "Tunnel Lamp", Vec3::new(x, y, z), Vec3::new(1.2, 0.8, 0.4), (255, 196, 120), Neon, Shape::Block);
            }
        }
    }

    // --- The castles --------------------------------------------------------

    fn castle(&mut self, map: InstanceId, team: &str, color: Rgb) {
        use Material::*;
        let s = side_of(team);
        let c = self.folder(map, &format!("{team} Castle"));
        // Each castle's stone leans toward its team's colour.
        let stone = if team == "Red" { (160, 146, 136) } else { (136, 146, 162) };
        let dark = shade(stone, 0.78);
        let floor = if team == "Red" { (150, 140, 132) } else { (132, 140, 152) };
        const H: f32 = 12.0;

        // The courtyard floor (through the gate, too), and a road out to
        // the bridge.
        self.boxr(c, "Courtyard", s, r((92.0, 142.0), (0.0, 0.2), (-26.0, 26.0)), floor, Concrete);
        self.boxr(c, "Road", s, r((32.0, 92.0), (0.0, 0.12), (-5.0, 5.0)), (160, 150, 130), Concrete);

        // The front wall, with the gate in the middle.
        for z in [(-30.0, -6.0), (6.0, 30.0)] {
            self.boxr(c, "Wall", s, r((98.0, 102.0), (0.0, H), z), stone, Brick);
        }
        self.boxr(c, "Gate Arch", s, r((98.0, 102.0), (9.0, H), (-6.0, 6.0)), dark, Brick);
        // Gate towers either side, sticking out toward the field.
        for z in [(-14.0, -6.0), (6.0, 14.0)] {
            self.boxr(c, "Gate Tower", s, r((92.0, 102.0), (0.0, H), z), dark, Brick);
        }
        // The back wall and the side walls, each with a small side door
        // (the north one faces the tunnel, the south one the ruins).
        self.boxr(c, "Wall", s, r((138.0, 142.0), (0.0, H), (-30.0, 30.0)), stone, Brick);
        for (z, sign) in [((26.0, 30.0), 1.0), ((-30.0, -26.0), -1.0)] {
            self.boxr(c, "Wall", s, r((102.0, 114.0), (0.0, H), z), stone, Brick);
            self.boxr(c, "Wall", s, r((120.0, 138.0), (0.0, H), z), stone, Brick);
            self.boxr(c, "Postern Arch", s, r((114.0, 120.0), (7.0, H), z), dark, Brick);
            let _ = sign;
        }

        // Battlements along every outer edge.
        let merlon = |b: &mut B, a: f32, z: f32| {
            b.boxr(c, "Battlement", s, r((a - 1.0, a + 1.0), (H, H + 2.0), (z - 1.0, z + 1.0)), stone, Brick);
        };
        for k in 0..8 {
            let z = -28.0 + k as f32 * 8.0;
            if !(-15.0..15.0).contains(&z) {
                merlon(self, 99.0, z);
            }
            merlon(self, 141.0, z);
        }
        for k in 0..6 {
            let a = 104.0 + k as f32 * 7.0;
            merlon(self, a, 29.0);
            merlon(self, a, -29.0);
        }
        for z in [-13.0, -7.0, 7.0, 13.0] {
            merlon(self, 93.0, z);
        }

        // Stairs up to the wall-walk, inside the front wall.
        for (sign, z0) in [(1.0, 8.0), (-1.0, -8.0)] {
            for k in 1..=12 {
                let za = z0 + sign * (k - 1) as f32 * 1.4;
                let zb = z0 + sign * k as f32 * 1.4;
                let (lo, hi) = if sign > 0.0 { (za, zb) } else { (zb, za) };
                self.boxr(c, "Wall Stairs", s, r((102.0, 106.0), (0.0, k as f32), (lo, hi)), if k % 2 == 0 { stone } else { dark }, Concrete);
            }
        }

        // The spawn house at the back: walls, a roof in team colour, and
        // the team's spawn pad inside.
        self.boxr(c, "Spawn House", s, r((126.0, 127.0), (0.0, 8.0), (-11.0, -4.0)), dark, Brick);
        self.boxr(c, "Spawn House", s, r((126.0, 127.0), (0.0, 8.0), (4.0, 11.0)), dark, Brick);
        self.boxr(c, "Spawn House", s, r((126.0, 127.0), (7.0, 8.0), (-4.0, 4.0)), dark, Brick);
        for z in [(-11.0, -10.0), (10.0, 11.0)] {
            self.boxr(c, "Spawn House", s, r((127.0, 138.0), (0.0, 8.0), z), dark, Brick);
        }
        self.boxr(c, "Spawn Roof", s, r((125.0, 138.0), (8.0, 9.0), (-12.0, 12.0)), shade(color, 0.85), Wood);
        let pad = self.dm.create(Class::SpawnLocation, &format!("{team} Spawn"), c).unwrap();
        {
            let p = self.dm.part_mut(pad).unwrap();
            p.position = Vec3::new(s * 132.0, 0.6, 0.0);
            p.size = Vec3::new(8.0, 0.8, 8.0);
            p.color = Color::new(color.0, color.1, color.2);
            p.material = Neon;
        }
        self.attr(pad, "team", Attribute::Str(team.into()));

        // The flag stand: a stone plinth with a glowing ring on top.
        let stand = self.boxr(c, &format!("{team} Flag Stand"), s, r((STAND_A - 3.0, STAND_A + 3.0), (0.0, 0.9), (-3.0, 3.0)), dark, Concrete);
        let _ = stand;
        self.boxr(c, "Stand Ring", s, r((STAND_A - 2.5, STAND_A + 2.5), (0.9, 1.0), (-2.5, 2.5)), color, Neon);
        for (da, dz) in [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
            self.part(c, "Stand Torch", Vec3::new(s * (STAND_A + da), 1.5, dz), Vec3::new(0.5, 3.0, 0.5), (60, 50, 40), Wood, Shape::Cylinder);
            self.part(c, "Torch Flame", Vec3::new(s * (STAND_A + da), 3.3, dz), Vec3::new(0.8, 0.8, 0.8), (255, 170, 60), Neon, Shape::Ball);
        }

        // Banners: big team-coloured drops on the back wall and the gate
        // towers, so you always know whose castle you're looking at.
        for z in [-18.0, 18.0] {
            self.boxr(c, "Castle Banner", s, r((137.6, 138.0), (3.0, 11.0), (z - 2.5, z + 2.5)), color, Plastic);
        }
        for z in [-10.0, 10.0] {
            self.boxr(c, "Castle Banner", s, r((91.6, 92.0), (4.0, 11.0), (z - 2.0, z + 2.0)), color, Plastic);
        }
        // Cover in the courtyard: crates and barrels.
        for (a, z) in [(108.0, -18.0), (110.0, 19.0), (130.0, -20.0), (112.0, -8.0), (112.0, 9.0)] {
            self.boxr(c, "Crate", s, r((a - 1.5, a + 1.5), (0.2, 3.2), (z - 1.5, z + 1.5)), (150, 110, 70), Wood);
        }
        for (a, z) in [(133.0, 18.0), (134.5, 20.5), (106.0, 22.0)] {
            self.part(c, "Barrel", Vec3::new(s * a, 1.6, z), Vec3::new(2.0, 2.8, 2.0), (120, 80, 50), Wood, Shape::Cylinder);
        }
        // Torches on the inside of the walls.
        for (a, z) in [(103.0, -24.0), (103.0, 24.0), (137.0, -24.0), (137.0, 24.0), (117.0, 25.0), (117.0, -25.0)] {
            self.part(c, "Wall Torch", Vec3::new(s * a, 7.0, z), Vec3::new(0.8, 0.8, 0.8), (255, 170, 60), Neon, Shape::Ball);
        }
    }

    /// The flag: a pole and a cloth, two separate parts the Game script
    /// moves together. Both ride on the carrier's back when taken.
    fn flag(&mut self, parent: InstanceId, team: &str, color: Rgb) {
        use Material::*;
        let s = side_of(team);
        let y = 1.0 + POLE_UP;
        let pole = self.part(parent, &format!("{team} Flag Pole"), Vec3::new(s * STAND_A, y, 0.0), Vec3::new(0.35, 7.0, 0.35), (230, 226, 214), Metal, Shape::Cylinder);
        let cloth = self.part(parent, &format!("{team} Flag"), Vec3::new(s * STAND_A, y + CLOTH_UP, -CLOTH_BACK), Vec3::new(0.2, 2.2, 3.2), color, Neon, Shape::Block);
        for (id, up, back) in [(pole, CARRY_UP, CARRY_BACK), (cloth, CARRY_UP + CLOTH_UP, CARRY_BACK + CLOTH_BACK)] {
            let p = self.dm.part_mut(id).unwrap();
            p.anchored = true;
            p.can_collide = false;
            self.attr(id, "carry_y", Attribute::Num(up as f64));
            self.attr(id, "carry_z", Attribute::Num(-back as f64));
        }
        self.attr(cloth, "beacon", Attribute::Bool(true));
        self.attr(cloth, "beacon_text", Attribute::Str(format!("{team} flag")));
    }

    /// Trees, rocks and the odd bush, placed so they're cover, not clutter.
    fn scenery(&mut self, map: InstanceId) {
        use Material::*;
        let f = self.folder(map, "Scenery");
        for side in [-1.0f32, 1.0] {
            // Tree clumps: the edges of the field and behind the castles.
            let spots = [
                (50.0, 70.0), (58.0, 80.0), (44.0, 84.0), (80.0, 72.0), (70.0, -75.0), (60.0, -84.0), (84.0, -70.0),
                (146.0, 60.0), (146.0, -60.0), (130.0, 80.0), (120.0, -84.0), (110.0, 70.0), (30.0, 22.0), (34.0, -20.0),
            ];
            for (i, (a, z)) in spots.iter().enumerate() {
                let k = i as i32 + if side < 0.0 { 0 } else { 100 };
                let h = 6.0 + hash(k) * 4.0;
                let (x, z) = (side * (a + hash(k + 3) * 4.0), z + hash(k + 5) * 4.0);
                let t = self.dm.create(Class::Model, "Tree", f).unwrap();
                self.part(t, "Trunk", Vec3::new(x, h / 2.0, z), Vec3::new(1.4, h, 1.4), (100, 70, 45), Wood, Shape::Cylinder);
                let leaf = if i % 3 == 0 { (60, 130, 58) } else { (74, 146, 64) };
                self.part(t, "Leaves", Vec3::new(x, h + 1.6, z), Vec3::new(6.5, 6.5, 6.5), leaf, Grass, Shape::Ball);
                self.part(t, "Leaves", Vec3::new(x + 1.4, h + 3.4, z - 0.8), Vec3::new(4.5, 4.5, 4.5), shade(leaf, 1.1), Grass, Shape::Ball);
            }
            // Boulders and low walls in the open field: somewhere to duck
            // behind between the bridge and the castle.
            for (i, (a, z, w, d, h)) in [
                (52.0, -8.0, 3.0, 6.0, 3.0), (66.0, 14.0, 6.0, 3.0, 2.6), (74.0, -22.0, 4.0, 4.0, 3.4),
                (46.0, 32.0, 5.0, 3.0, 2.4), (84.0, 30.0, 3.0, 5.0, 2.8), (40.0, -30.0, 3.0, 3.0, 2.0),
            ]
            .iter()
            .enumerate()
            {
                let rock = self.boxr(f, "Boulder", side, r((a - w / 2.0, a + w / 2.0), (0.0, *h), (z - d / 2.0, z + d / 2.0)), (126, 122, 116), Concrete);
                self.dm.part_mut(rock).unwrap().rotation = Vec3::new(0.0, hash(i as i32 + 40) * 40.0 - 20.0, 0.0);
            }
            for (a, z) in [(86.0, 58.0), (108.0, 54.0), (104.0, -56.0), (56.0, 58.0), (58.0, -62.0)] {
                self.part(f, "Bush", Vec3::new(side * a, 0.8, z), Vec3::new(3.4, 2.4, 3.4), (58, 120, 52), Grass, Shape::Ball);
            }
            // Lamp posts down the road.
            for a in [45.0, 70.0] {
                for z in [-6.5, 6.5] {
                    self.part(f, "Lamp Post", Vec3::new(side * a, 3.0, z), Vec3::new(0.6, 6.0, 0.6), (40, 40, 44), Metal, Shape::Cylinder);
                    self.part(f, "Lamp", Vec3::new(side * a, 6.4, z), Vec3::new(1.2, 1.2, 1.2), (255, 220, 160), Neon, Shape::Ball);
                }
            }
        }
    }
}

/// Flagfall, ready to play or to open in the studio.
pub fn flagfall() -> DataModel {
    let mut b = B { dm: DataModel::new() };
    let root = b.dm.root();

    let map = b.folder(root, "Map");
    b.ground(map);
    b.bridge(map);
    b.ford(map);
    b.tunnel(map);
    for (team, color) in TEAMS {
        b.castle(map, team, color);
    }
    b.scenery(map);
    let flags = b.folder(root, "Flags");
    for (team, color) in TEAMS {
        b.flag(flags, team, color);
    }
    let ruins = b.folder(root, "Ruins");
    b.ruins(ruins, 0.0);

    // Hidden things: the gear, and a copy of the ruins to rebuild from.
    let storage = b.folder(root, "Storage");
    gears::install(&mut b.dm, storage);
    let template = b.folder(storage, "Ruins Template");
    b.ruins(template, -TEMPLATE_DROP);
    // Each player's own hint, shown under the scores.
    let tip = b.label(storage, "Tip Template", "", (0.3, 0.185, 0.4, 0.045), 18.0, (24, 24, 30));
    {
        let g = b.dm.gui_mut(tip).unwrap();
        g.text_color = Color::new(255, 214, 90);
        g.visible = false;
    }
    b.folder(root, "Projectiles");
    b.folder(root, "Effects");

    // Music and sounds, all composed in code (see synth.rs).
    b.sound(root, "Theme", synth::flagfall_theme(false), 0.55);
    b.sound(root, "Theme Intense", synth::flagfall_theme(true), 0.55);
    b.sound(root, "Flag Taken", synth::flag_taken(), 0.9);
    b.sound(root, "Flag Returned", synth::flag_returned(), 0.8);
    b.sound(root, "Capture", synth::capture_fanfare(), 1.0);
    b.sound(root, "Victory", synth::victory(), 1.0);
    b.sound(root, "Round Horn", synth::horn(), 0.8);

    // The screen: score, clock, flag status, feed. (The leaderboard is
    // the built-in one: `leaderboard(...)` in the script.)
    let title = b.label(root, "Title", "FLAGFALL", (0.012, 0.945, 0.11, 0.045), 22.0, (24, 24, 30));
    b.dm.gui_mut(title).unwrap().text_color = Color::new(255, 214, 90);
    b.label(root, "Red Score", "RED  0", (0.355, 0.012, 0.1, 0.06), 28.0, (196, 40, 28));
    b.label(root, "Clock", "8:00", (0.455, 0.012, 0.09, 0.06), 26.0, (24, 24, 30));
    b.label(root, "Blue Score", "0  BLUE", (0.545, 0.012, 0.1, 0.06), 28.0, (13, 105, 172));
    b.label(root, "Red Flag Status", "Red flag: home", (0.285, 0.076, 0.215, 0.036), 15.0, (70, 20, 16));
    b.label(root, "Blue Flag Status", "Blue flag: home", (0.5, 0.076, 0.215, 0.036), 15.0, (10, 42, 80));
    let feed = b.label(root, "Feed", "", (0.3, 0.125, 0.4, 0.048), 19.0, (60, 60, 66));
    b.dm.gui_mut(feed).unwrap().visible = false;
    let banner = b.label(root, "Banner", "", (0.25, 0.33, 0.5, 0.11), 42.0, (60, 60, 66));
    b.dm.gui_mut(banner).unwrap().visible = false;
    b.label(root, "Help", "Take their flag to your stand. First to 3 wins.", (0.3, 0.955, 0.4, 0.035), 14.0, (24, 24, 30));

    // The flag's shape, shared with the script so both agree.
    let game = GAME.replacen(
        "TEAMS = [",
        &format!("POLE_UP = {POLE_UP}\nCLOTH_UP = {CLOTH_UP}\nCLOTH_BACK = {CLOTH_BACK}\nTEAMS = ["),
        1,
    );
    b.script(root, "Game", &game);
    b.dm
}
