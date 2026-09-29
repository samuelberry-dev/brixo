//! Lets Rovik scripts see and change the game world.
//!
//! Parts reach scripts as objects. Their vector properties are "facets" of
//! the same object, so `self.position` is a live view of the real position:
//! `self.position.y += 1` moves the part, rather than changing a copy.

use parking_lot::Mutex as WorldMutex;
use std::sync::{Arc, Mutex};

use brixo_core::{CameraMode, Class, Color, DataModel, Face, InstanceId, PartProps, Shape, Vec3};
use rovik::value::format_number;
use rovik::{Host, ObjectRef, Value};

pub const FACET_SELF: u32 = 0;
pub const FACET_POSITION: u32 = 1;
pub const FACET_SIZE: u32 = 2;
pub const FACET_ROTATION: u32 = 3;
pub const FACET_COLOR: u32 = 4;
/// A player's four avatar colours; each works like a part's `color`.
pub const FACET_SKIN: u32 = 5;
pub const FACET_SHIRT: u32 = 6;
pub const FACET_PANTS: u32 = 7;
pub const FACET_SHOES: u32 = 8;
/// The `camera` object scripts get; its id is the player's.
pub const FACET_CAMERA: u32 = 9;
/// A GUI element's text and background colours.
pub const FACET_TEXT_COLOR: u32 = 10;
pub const FACET_BG_COLOR: u32 = 11;

/// What scripts can make with `create`.
const CREATABLE: [(&str, Class); 8] = [
    ("Part", Class::Part),
    ("Model", Class::Model),
    ("Folder", Class::Folder),
    ("TextLabel", Class::TextLabel),
    ("TextButton", Class::TextButton),
    ("Frame", Class::Frame),
    ("Tool", Class::Tool),
    ("SpawnLocation", Class::SpawnLocation),
];

fn is_color_facet(facet: u32) -> bool {
    facet == FACET_COLOR || (FACET_SKIN..=FACET_SHOES).contains(&facet) || facet == FACET_TEXT_COLOR || facet == FACET_BG_COLOR
}

/// Reads whichever colour a colour facet points at.
fn color_of(world: &DataModel, id: InstanceId, facet: u32) -> Option<Color> {
    if facet == FACET_COLOR {
        return world.body(id).map(|p| p.color);
    }
    if facet == FACET_TEXT_COLOR || facet == FACET_BG_COLOR {
        let g = world.gui(id)?;
        return Some(if facet == FACET_TEXT_COLOR { g.text_color } else { g.background_color });
    }
    let p = world.player(id)?;
    Some(match facet {
        FACET_SKIN => p.skin_color,
        FACET_SHIRT => p.shirt_color,
        FACET_PANTS => p.pants_color,
        _ => p.shoes_color,
    })
}

fn color_mut(world: &mut DataModel, id: InstanceId, facet: u32) -> Option<&mut Color> {
    if facet == FACET_COLOR {
        return world.body_mut(id).map(|p| &mut p.color);
    }
    if facet == FACET_TEXT_COLOR || facet == FACET_BG_COLOR {
        let g = world.gui_mut(id)?;
        return Some(if facet == FACET_TEXT_COLOR { &mut g.text_color } else { &mut g.background_color });
    }
    let p = world.player_mut(id)?;
    Some(match facet {
        FACET_SKIN => &mut p.skin_color,
        FACET_SHIRT => &mut p.shirt_color,
        FACET_PANTS => &mut p.pants_color,
        _ => &mut p.shoes_color,
    })
}

/// After a script changes a player's colour: skin colours the head, arms
/// and legs; a shirt (or pants) colour on someone wearing none puts a plain
/// one on, so team colours always show.
fn colored(world: &mut DataModel, id: InstanceId, facet: u32) {
    let Some(p) = world.player_mut(id) else { return };
    match facet {
        FACET_SKIN => {
            let c = p.skin_color;
            p.set_skin(c);
        }
        FACET_SHIRT if p.shirt == brixo_core::Shirt::None => p.shirt = brixo_core::Shirt::Tee,
        FACET_PANTS if p.pants == brixo_core::Pants::None => p.pants = brixo_core::Pants::Plain,
        _ => {}
    }
}

fn avatar_color_facet(name: &str) -> Option<u32> {
    match name {
        "skin_color" => Some(FACET_SKIN),
        "shirt_color" => Some(FACET_SHIRT),
        "pants_color" => Some(FACET_PANTS),
        "shoes_color" => Some(FACET_SHOES),
        _ => None,
    }
}

/// A handle on one facet of an object (a colour, the camera...).
pub fn facet_object(id: InstanceId, facet: u32) -> Value {
    Value::Object(ObjectRef { id: id.raw(), facet })
}

fn face_names() -> String {
    Face::ALL.iter().map(|f| f.name()).collect::<Vec<_>>().join(", ")
}

/// Smallest size a script can shrink a part to.
const MIN_SIZE: f32 = 0.05;

pub struct WorldHost {
    pub world: Arc<WorldMutex<DataModel>>,
    /// Seconds since the game started, for time().
    pub clock: Arc<Mutex<f64>>,
    /// Sounds and music scripts asked for, for the players to hear.
    pub sounds: Arc<Mutex<Vec<SoundEvent>>>,
    /// Lines for everyone's chat from the game itself ("Sam completed Win
    /// a Round!").
    pub notices: Arc<Mutex<Vec<String>>>,
    /// Explosions scripts set off, carried out by the game's next step.
    pub blasts: Arc<Mutex<Vec<Blast>>>,
    /// Saved player data, for save() and load().
    pub saves: Arc<Mutex<crate::saves::Saves>>,
    /// Each hinged part's angle as of the last physics step, for `hinge_angle`.
    pub hinge_angles: Arc<Mutex<std::collections::HashMap<InstanceId, f32>>>,
    /// Kart commands (boost, spin_out, place_kart) for the next step.
    pub kart_commands: Arc<Mutex<Vec<crate::physics::KartCommand>>>,
    /// Bots `add_bot` made, for the game to take on at its next step.
    pub new_bots: Arc<Mutex<Vec<InstanceId>>>,
}

/// Breakable parts: anchored parts with a custom field `breakable = true`.
/// They stay perfectly still (and cost nothing to simulate) until a blast
/// reaches them: then the ones in range come loose and fly, and any that
/// are no longer connected to solid ground, through the breakable parts they
/// touch, fall too. That's how a tower collapses when its base is blown out.
pub fn break_and_collapse(world: &mut DataModel, center: Vec3, radius: f32, power: f32) {
    let ids = world.walk();
    let mut loosened: Vec<InstanceId> = Vec::new();
    for &id in &ids {
        if !is_breakable(world, id) {
            continue;
        }
        let p = world.part_mut(id).unwrap();
        let away = glam::Vec3::new(p.position.x - center.x, p.position.y - center.y, p.position.z - center.z);
        let d = away.length();
        if d < radius {
            let kick = (away.normalize_or(glam::Vec3::Y) + glam::Vec3::Y * 0.35).normalize() * power * (1.0 - d / radius);
            p.anchored = false;
            p.velocity = Vec3::new(kick.x, kick.y, kick.z);
            loosened.push(id);
        }
    }
    if !loosened.is_empty() {
        collapse(world, &loosened);
    }
}

/// An anchored part with `breakable = true` (and not part of a tool).
fn is_breakable(world: &DataModel, id: InstanceId) -> bool {
    world.part(id).is_some_and(|p| p.anchored)
        && world.tool_of(id).is_none()
        && matches!(world.get(id).and_then(|i| i.attributes.get("breakable")), Some(brixo_core::Attribute::Bool(true)))
}

type Box3 = (glam::Vec3, glam::Vec3);

fn aabb(p: &brixo_core::PartProps) -> Box3 {
    let c = glam::Vec3::new(p.position.x, p.position.y, p.position.z);
    let h = glam::Vec3::new(p.size.x, p.size.y, p.size.z) / 2.0 + glam::Vec3::splat(0.06);
    (c - h, c + h)
}

fn touch(a: &Box3, b: &Box3) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

/// After some bricks were knocked loose: each group of breakable bricks
/// that was touching them is checked as a whole. A group still touching
/// solid ground (an anchored part that isn't breakable) stays up; a group
/// that isn't falls. Only groups next to the blast are looked at, so other
/// structures (like a hidden rebuild copy of the map) are never touched.
fn collapse(world: &mut DataModel, loosened: &[InstanceId]) {
    let mut bricks: Vec<(InstanceId, Box3)> = Vec::new();
    let mut ground: Vec<Box3> = Vec::new();
    for id in world.walk() {
        let Some(p) = world.part(id) else { continue };
        if !p.anchored || world.tool_of(id).is_some() {
            continue;
        }
        if is_breakable(world, id) { bricks.push((id, aabb(p))) } else { ground.push(aabb(p)) }
    }
    let gone: Vec<Box3> = loosened.iter().filter_map(|id| world.part(*id)).map(aabb).collect();
    // A grid of 4-stud cells, so each brick only checks its neighbours.
    let cell = |v: glam::Vec3| (v / 4.0).floor().as_ivec3();
    let mut grid: std::collections::HashMap<glam::IVec3, Vec<usize>> = Default::default();
    for (i, (_, b)) in bricks.iter().enumerate() {
        let (lo, hi) = (cell(b.0), cell(b.1));
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    grid.entry(glam::IVec3::new(x, y, z)).or_default().push(i);
                }
            }
        }
    }
    let neighbours = |i: usize| -> Vec<usize> {
        let b = bricks[i].1;
        let (lo, hi) = (cell(b.0), cell(b.1));
        let mut out = Vec::new();
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for &j in grid.get(&glam::IVec3::new(x, y, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        if j != i && touch(&b, &bricks[j].1) {
                            out.push(j);
                        }
                    }
                }
            }
        }
        out
    };
    let mut seen = vec![false; bricks.len()];
    let seeds: Vec<usize> = (0..bricks.len()).filter(|&i| gone.iter().any(|g| touch(&bricks[i].1, g))).collect();
    for seed in seeds {
        if seen[seed] {
            continue;
        }
        // Gather this whole group, noting whether any of it is on the ground.
        let mut group = vec![seed];
        seen[seed] = true;
        let mut k = 0;
        let mut grounded = false;
        while k < group.len() {
            let i = group[k];
            k += 1;
            grounded |= ground.iter().any(|g| touch(&bricks[i].1, g));
            for j in neighbours(i) {
                if !seen[j] {
                    seen[j] = true;
                    group.push(j);
                }
            }
        }
        if !grounded {
            for i in group {
                world.part_mut(bricks[i].0).unwrap().anchored = false;
            }
        }
    }
}

/// An explosion waiting to happen (see `explode`).
#[derive(Debug, Clone, Copy)]
pub struct Blast {
    pub center: Vec3,
    pub radius: f32,
    pub power: f32,
}

/// Something for players to hear.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundEvent {
    /// A sound effect, for everyone or just one player; `at` places it in
    /// the world (None: heard the same everywhere).
    Play { name: String, player: Option<InstanceId>, at: Option<brixo_core::SoundAt> },
    /// Start a music loop (None stops the music), for everyone or one player.
    Music { name: Option<String>, player: Option<InstanceId> },
}

/// What one player should hear.
#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    Sound(String),
    /// A sound from a place in the world (see `brixo_core::SoundAt`).
    SoundAt(String, brixo_core::SoundAt),
    Music(Option<String>),
}

impl SoundEvent {
    /// This event as `me` hears it (None if it's for someone else).
    pub fn for_player(self, me: Option<InstanceId>) -> Option<Cue> {
        match self {
            SoundEvent::Play { name, player, at } if player.is_none() || player == me => Some(match at {
                Some(at) => Cue::SoundAt(name, at),
                None => Cue::Sound(name),
            }),
            SoundEvent::Music { name, player } if player.is_none() || player == me => Some(Cue::Music(name)),
            _ => None,
        }
    }
}

/// The built-in fields each kind of object has, for "did you mean" on
/// typos. Only fields the class really has count: `pad.next` on a part is a
/// custom field, not a typo of a label's `text`.
fn fields_for(class: Class) -> Vec<&'static str> {
    let mut f = vec!["name", "class", "parent", "children"];
    match class {
        Class::Part | Class::SpawnLocation => f.extend([
            "position", "size", "rotation", "color", "anchored", "can_collide", "shape", "material",
            "transparency", "velocity", "floating", "bounce", "hinge", "hinge_at", "motor_speed", "swing_to", "hinge_angle",
        ]),
        Class::Model => f.extend(["driver", "speed", "steer", "drift", "boosting", "spinning", "locked", "top_speed"]),
        Class::Player => f.extend([
            "position", "size", "rotation", "velocity", "health", "max_health", "walk_speed", "jump_power", "face", "swinging", "look", "mouse",
            "skin_color", "shirt_color", "pants_color", "shoes_color", "shirt", "pants", "tshirt", "hats", "camera_mode", "equipped", "kart", "bot",
            "camera_part",
        ]),
        Class::TextLabel | Class::TextButton | Class::Frame => f.extend([
            "text", "text_size", "text_color", "background", "background_color", "visible", "x", "y", "width",
            "height", "attached_to",
        ]),
        Class::Sound => f.push("volume"),
        Class::Workspace => f.extend(brixo_core::Lighting::FIELDS),
        _ => {}
    }
    f
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// A real field of `class` that `name` is probably a typo of.
fn near_miss(class: Class, name: &str) -> Option<&'static str> {
    // Custom fields the engine itself reads aren't typos.
    if ["lane"].contains(&name) {
        return None;
    }
    fields_for(class)
        .into_iter()
        .filter(|f| *f != name)
        .find(|f| name.len() > 2 && edit_distance(name, f) <= if name.len() <= 4 { 1 } else { 2 })
}

fn attribute_value(a: &brixo_core::Attribute) -> Value {
    match a {
        brixo_core::Attribute::Num(n) => Value::Num(*n),
        brixo_core::Attribute::Str(t) => Value::str(t.as_str()),
        brixo_core::Attribute::Bool(b) => Value::Bool(*b),
    }
}

/// The script-side handle for an instance.
pub fn object(id: InstanceId) -> Value {
    Value::Object(ObjectRef {
        id: id.raw(),
        facet: FACET_SELF,
    })
}

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Workspace => "workspace",
        Class::Folder => "folder",
        Class::Part => "part",
        Class::Script => "script",
        Class::SpawnLocation => "spawnlocation",
        Class::Model => "model",
        Class::TextLabel => "textlabel",
        Class::TextButton => "textbutton",
        Class::Frame => "frame",
        Class::Tool => "tool",
        Class::Sound => "sound",
        Class::Player => "player",
    }
}

fn gone() -> String {
    "this object was destroyed, so it can't be used any more".to_string()
}

/// A part's velocity (conveyor speed, for anchored parts).
pub const FACET_VELOCITY: u32 = 12;

fn vec_facet(name: &str) -> Option<u32> {
    match name {
        "velocity" => Some(FACET_VELOCITY),
        "position" => Some(FACET_POSITION),
        "size" => Some(FACET_SIZE),
        "rotation" => Some(FACET_ROTATION),
        "color" => Some(FACET_COLOR),
        _ => None,
    }
}

fn read_vec(p: &PartProps, facet: u32) -> Vec3 {
    match facet {
        FACET_POSITION => p.position,
        FACET_VELOCITY => p.velocity,
        FACET_SIZE => p.size,
        _ => p.rotation,
    }
}

fn write_vec(p: &mut PartProps, facet: u32, v: Vec3) {
    match facet {
        FACET_POSITION => p.position = v,
        FACET_VELOCITY => p.velocity = v,
        FACET_SIZE => {
            p.size = Vec3::new(v.x.max(MIN_SIZE), v.y.max(MIN_SIZE), v.z.max(MIN_SIZE))
        }
        _ => p.rotation = v,
    }
}

fn number(value: &Value, what: &str) -> Result<f32, String> {
    match value {
        Value::Num(n) => Ok(*n as f32),
        other => Err(format!("{what} has to be a number, not a {}", other.type_name())),
    }
}

fn channel(value: &Value, what: &str) -> Result<u8, String> {
    Ok(number(value, what)?.round().clamp(0.0, 255.0) as u8)
}


impl WorldHost {
    /// A sound effect to play: one of Brixo's built-in names, or a Sound in
    /// the game ("#id").
    fn sound_name(&self, value: &Value, func: &str) -> Result<String, String> {
        let list = &brixo_core::SOUNDS;
        match value {
            Value::Str(n) if list.contains(&n.as_ref()) => Ok(n.to_string()),
            Value::Str(n) => Err(format!("there's no sound called '{n}'. Try one of: {}, or a Sound in your game: {func}(find(\"My Sound\"), ...)", list.join(", "))),
            Value::Object(o) if o.facet == FACET_SELF && self.world.lock().sound(InstanceId::from_raw(o.id)).is_some() => Ok(format!("#{}", o.id)),
            other => Err(format!("{func} needs a sound's name or a Sound, not a {}", other.type_name())),
        }
    }

    /// Reads a vector from `{x = 1, y = 2}` (missing axes keep `current`)
    /// or from another part's position/size/rotation.
    fn to_vec3(&self, world: &DataModel, value: &Value, current: Vec3) -> Result<Vec3, String> {
        match value {
            Value::Map(map) => {
                let map = map.read().unwrap().clone();
                let mut v = current;
                for (key, item) in &map {
                    match key.as_str() {
                        "x" => v.x = number(item, "x")?,
                        "y" => v.y = number(item, "y")?,
                        "z" => v.z = number(item, "z")?,
                        other => return Err(format!("a position only has x, y and z, not '{other}'")),
                    }
                }
                Ok(v)
            }
            Value::Object(o) if matches!(o.facet, FACET_POSITION | FACET_SIZE | FACET_ROTATION | FACET_VELOCITY) => {
                let p = world.body(InstanceId::from_raw(o.id)).ok_or_else(gone)?;
                Ok(read_vec(p, o.facet))
            }
            other => Err(format!(
                "expected something like {{x = 1, y = 2, z = 3}}, but got a {}",
                other.type_name()
            )),
        }
    }

    fn to_color(&self, world: &DataModel, value: &Value, current: Color) -> Result<Color, String> {
        match value {
            Value::Map(map) => {
                let map = map.read().unwrap().clone();
                let mut c = current;
                for (key, item) in &map {
                    match key.as_str() {
                        "r" => c.r = channel(item, "r")?,
                        "g" => c.g = channel(item, "g")?,
                        "b" => c.b = channel(item, "b")?,
                        other => return Err(format!("a color only has r, g and b, not '{other}'")),
                    }
                }
                Ok(c)
            }
            Value::Object(o) if is_color_facet(o.facet) => {
                color_of(world, InstanceId::from_raw(o.id), o.facet).ok_or_else(gone)
            }
            other => Err(format!(
                "expected something like {{r = 255, g = 0, b = 0}}, but got a {}",
                other.type_name()
            )),
        }
    }
}

impl Host for WorldHost {
    fn get_field(&self, obj: ObjectRef, name: &str) -> Result<Value, String> {
        let world = self.world.lock();
        let id = InstanceId::from_raw(obj.id);
        let inst = world.get(id).ok_or_else(gone)?;

        match obj.facet {
            FACET_SELF => match name {
                "name" => Ok(Value::str(inst.name.as_str())),
                "class" => Ok(Value::str(class_name(inst.class))),
                // Lighting lives on the Workspace.
                "time_of_day" | "brightness" | "fog_start" | "fog_end" | "fog_color" | "sky_color" if inst.class == Class::Workspace => {
                    let l = brixo_core::Lighting::of(&world);
                    let color = |c: Color| Value::map([("r", c.r), ("g", c.g), ("b", c.b)].into_iter().map(|(k, v)| (k.to_string(), Value::Num(v as f64))).collect());
                    Ok(match name {
                        "time_of_day" => Value::Num(l.time_of_day as f64),
                        "brightness" => Value::Num(l.brightness as f64),
                        "fog_start" => Value::Num(l.fog_start as f64),
                        "fog_end" => Value::Num(l.fog_end as f64),
                        "fog_color" => color(l.fog_color),
                        _ => l.sky_color.map(color).unwrap_or(Value::Nil),
                    })
                }
                "parent" => Ok(inst.parent.map(object).unwrap_or(Value::Nil)),
                "children" => Ok(Value::list(inst.children.iter().map(|c| object(*c)).collect())),
                "camera_mode" => match world.player(id) {
                    Some(p) => Ok(Value::str(p.camera_mode.name())),
                    None => Err(format!("a {} doesn't have a camera_mode. Only players do", class_name(inst.class))),
                },
                "face" => match world.player(id) {
                    Some(p) => Ok(Value::str(p.face.name())),
                    None => Err(format!("a {} doesn't have a face. Only players do", class_name(inst.class))),
                },
                "hats" => match world.player(id) {
                    Some(p) => Ok(Value::list(p.hats.iter().flatten().map(|h| Value::str(h.name())).collect())),
                    None => Err(format!("a {} doesn't have hats. Only players do", class_name(inst.class))),
                },
                "shirt" | "pants" | "tshirt" => match world.player(id) {
                    Some(p) => Ok(match name {
                        "shirt" => Value::str(p.shirt.name()),
                        "pants" => Value::str(p.pants.name()),
                        _ => p.tshirt.map(|t| Value::str(t.name())).unwrap_or(Value::Nil),
                    }),
                    None => Err(format!("a {} doesn't have {name}. Only players do", class_name(inst.class))),
                },
                "skin_color" | "shirt_color" | "pants_color" | "shoes_color" => match world.player(id) {
                    Some(_) => Ok(facet_object(id, avatar_color_facet(name).unwrap())),
                    None => Err(format!("a {} doesn't have {name}. Only players do", class_name(inst.class))),
                },
                // The kart a player is driving (nil if none).
                "kart" if world.player(id).is_some() => {
                    Ok(world.player(id).unwrap().kart.filter(|k| world.get(*k).is_some()).map(object).unwrap_or(Value::Nil))
                }
                // The part a cutscene camera sits on (nil: the usual camera).
                "camera_part" if world.player(id).is_some() => Ok(brixo_core::camera_part(&world, id).map(object).unwrap_or(Value::Nil)),
                "bot" if world.player(id).is_some() => Ok(Value::Bool(matches!(inst.attributes.get("bot"), Some(brixo_core::Attribute::Bool(true))))),
                // Who's driving a kart (nil if nobody).
                "driver" if brixo_core::is_kart(&world, id) => Ok(world
                    .walk()
                    .into_iter()
                    .find(|p| world.player(*p).is_some_and(|pp| pp.kart == Some(id)))
                    .map(object)
                    .unwrap_or(Value::Nil)),
                "health" | "max_health" | "walk_speed" | "jump_power" => match world.player(id) {
                    Some(p) => Ok(Value::Num(match name {
                        "health" => p.health,
                        "max_health" => p.max_health,
                        "walk_speed" => p.walk_speed,
                        _ => p.jump_power,
                    } as f64)),
                    None => Err(format!("a {} doesn't have {name}. Only players do", class_name(inst.class))),
                },
                "x" | "y" | "width" | "height" | "text_size" | "text" | "background" | "visible" | "text_color"
                | "background_color"
                    if world.gui(id).is_some() =>
                {
                    let g = world.gui(id).unwrap();
                    Ok(match name {
                        "x" => Value::Num(g.x as f64),
                        "y" => Value::Num(g.y as f64),
                        "width" => Value::Num(g.width as f64),
                        "height" => Value::Num(g.height as f64),
                        "text_size" => Value::Num(g.text_size as f64),
                        "text" => Value::str(g.text.as_str()),
                        "background" => Value::Bool(g.background),
                        "visible" => Value::Bool(g.visible),
                        "text_color" => facet_object(id, FACET_TEXT_COLOR),
                        _ => facet_object(id, FACET_BG_COLOR),
                    })
                }
                "floating" if world.part(id).is_some() => Ok(Value::Bool(world.part(id).unwrap().floating)),
                "hinge" if world.part(id).is_some() => Ok(Value::str(world.part(id).unwrap().hinge.name())),
                "hinge_at" if world.part(id).is_some() => Ok(Value::str(world.part(id).unwrap().hinge_at.name())),
                "motor_speed" if world.part(id).is_some() => Ok(Value::Num(world.part(id).unwrap().motor_speed as f64)),
                "swing_to" if world.part(id).is_some() => Ok(world.part(id).unwrap().swing_to.map(|a| Value::Num(a as f64)).unwrap_or(Value::Nil)),
                // Read-only: how far it has turned (0 until it has a hinge).
                "hinge_angle" if world.part(id).is_some() => {
                    let a = self.hinge_angles.lock().unwrap().get(&id).copied().unwrap_or(0.0);
                    Ok(Value::Num(((a * 10.0).round() / 10.0) as f64))
                }
                "bounce" if world.part(id).is_some() => Ok(Value::Num(world.part(id).unwrap().bounce as f64)),
                "material" | "transparency" => match world.part(id) {
                    Some(p) if name == "material" => Ok(Value::str(p.material.name())),
                    Some(p) => Ok(Value::Num(p.transparency as f64)),
                    None => Err(format!("a {} doesn't have {name}. Only parts do", class_name(inst.class))),
                },
                "attached_to" if world.gui(id).is_some() => {
                    Ok(world.gui(id).unwrap().attached_to.filter(|a| world.get(*a).is_some()).map(object).unwrap_or(Value::Nil))
                }
                "volume" if world.sound(id).is_some() => Ok(Value::Num(world.sound(id).unwrap().volume as f64)),
                "swinging" if world.player(id).is_some() => Ok(Value::Bool(world.player(id).unwrap().swing > 0.0)),
                // Which way the player faces, flat: {x, y = 0, z}, length 1.
                "mouse" if world.player(id).is_some() => {
                    // Where they last clicked with a tool (read-only).
                    let m = world.player(id).unwrap().mouse;
                    let mut map = std::collections::BTreeMap::new();
                    map.insert("x".to_string(), Value::Num(m.x as f64));
                    map.insert("y".to_string(), Value::Num(m.y as f64));
                    map.insert("z".to_string(), Value::Num(m.z as f64));
                    Ok(Value::map(map))
                }
                "look" if world.player(id).is_some() => {
                    let yaw = world.player(id).unwrap().body.rotation.y.to_radians();
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("x".to_string(), Value::Num(yaw.sin() as f64));
                    m.insert("y".to_string(), Value::Num(0.0));
                    m.insert("z".to_string(), Value::Num(yaw.cos() as f64));
                    Ok(Value::map(m))
                }
                "equipped" => match world.player(id) {
                    Some(p) => Ok(p.equipped.filter(|t| world.get(*t).is_some()).map(object).unwrap_or(Value::Nil)),
                    None => Err(format!("a {} doesn't have equipped. Only players do", class_name(inst.class))),
                },
                "shape" => match world.part(id) {
                    Some(p) => Ok(Value::str(p.shape.name())),
                    None => Err(format!("a {} doesn't have a shape. Only parts do", class_name(inst.class))),
                },
                "anchored" | "can_collide" => match world.part(id) {
                    Some(p) => Ok(Value::Bool(if name == "anchored" { p.anchored } else { p.can_collide })),
                    None => Err(format!("a {} doesn't have {name}. Only parts do", class_name(inst.class))),
                },
                other => match vec_facet(other) {
                    Some(facet) if world.body(id).is_some() => Ok(Value::Object(ObjectRef {
                        id: obj.id,
                        facet,
                    })),
                    Some(_) => Err(format!(
                        "a {} doesn't have a {other}. Only parts do",
                        class_name(inst.class)
                    )),
                    // A custom field a script set earlier.
                    None if inst.attributes.contains_key(other) => Ok(attribute_value(&inst.attributes[other])),
                    None => match near_miss(inst.class, other) {
                        Some(real) => Err(format!("a {} doesn't have '{other}'. Did you mean '{real}'?", class_name(inst.class))),
                        // Custom fields that were never set read as nil, so
                        // scripts can check `if p.cash == nil then`.
                        None => Ok(Value::Nil),
                    },
                },
            },

            FACET_POSITION | FACET_SIZE | FACET_ROTATION | FACET_VELOCITY => {
                let v = read_vec(world.body(id).ok_or_else(gone)?, obj.facet);
                match name {
                    "x" => Ok(Value::Num(v.x as f64)),
                    "y" => Ok(Value::Num(v.y as f64)),
                    "z" => Ok(Value::Num(v.z as f64)),
                    other => Err(format!("this only has x, y and z, not '{other}'")),
                }
            }

            FACET_CAMERA => match name {
                "mode" => {
                    let p = world.player(id).ok_or_else(gone)?;
                    Ok(Value::str(p.camera_mode.name()))
                }
                other => Err(format!("the camera only has mode, not '{other}'")),
            },

            facet if is_color_facet(facet) => {
                let c = color_of(&world, id, facet).ok_or_else(gone)?;
                match name {
                    "r" => Ok(Value::Num(c.r as f64)),
                    "g" => Ok(Value::Num(c.g as f64)),
                    "b" => Ok(Value::Num(c.b as f64)),
                    other => Err(format!("a color only has r, g and b, not '{other}'")),
                }
            }

            _ => Err("unknown object".to_string()),
        }
    }

    fn set_field(&self, obj: ObjectRef, name: &str, value: Value) -> Result<(), String> {
        let mut world = self.world.lock();
        let id = InstanceId::from_raw(obj.id);
        let class = world.get(id).ok_or_else(gone)?.class;

        match obj.facet {
            FACET_SELF => match name {
                "name" => match value {
                    Value::Str(s) => {
                        world.get_mut(id).unwrap().name = s.to_string();
                        Ok(())
                    }
                    other => Err(format!("name has to be text, not a {}", other.type_name())),
                },
                "class" => Err("class can't be changed".to_string()),
                "time_of_day" | "brightness" | "fog_start" | "fog_end" | "fog_color" | "sky_color" if class == Class::Workspace => {
                    let mut l = brixo_core::Lighting::of(&world);
                    let d = brixo_core::Lighting::default();
                    match name {
                        "time_of_day" => l.time_of_day = if matches!(value, Value::Nil) { d.time_of_day } else { number(&value, name)?.rem_euclid(24.0) },
                        "brightness" => l.brightness = if matches!(value, Value::Nil) { d.brightness } else { number(&value, name)?.clamp(0.0, 3.0) },
                        "fog_start" => l.fog_start = if matches!(value, Value::Nil) { 0.0 } else { number(&value, name)?.max(0.0) },
                        "fog_end" => l.fog_end = if matches!(value, Value::Nil) { 0.0 } else { number(&value, name)?.max(0.0) },
                        "fog_color" => l.fog_color = if matches!(value, Value::Nil) { d.fog_color } else { self.to_color(&world, &value, l.fog_color)? },
                        _ => {
                            l.sky_color = match value {
                                Value::Nil => None,
                                v => Some(self.to_color(&world, &v, l.sky_color.unwrap_or(Color::new(20, 85, 200)))?),
                            }
                        }
                    }
                    l.set(&mut world);
                    Ok(())
                }
                "mouse" | "look" | "swinging" | "bot" if world.player(id).is_some() => Err(format!("{name} can't be changed")),
                "driver" if brixo_core::is_kart(&world, id) => Err("a kart's driver can't be set: set the player's kart instead (player.kart = the_kart)".into()),
                // A cutscene: the player's camera sits on a part, looking
                // the way it faces (move the part to move the camera).
                "camera_part" if world.player(id).is_some() => match value {
                    Value::Nil => {
                        world.get_mut(id).unwrap().attributes.remove(brixo_core::CAMERA_PART);
                        Ok(())
                    }
                    Value::Object(o) if world.part(InstanceId::from_raw(o.id)).is_some() => {
                        world.get_mut(id).unwrap().attributes.insert(brixo_core::CAMERA_PART.into(), brixo_core::Attribute::Num(o.id as f64));
                        Ok(())
                    }
                    other => Err(format!("camera_part has to be a part (or nil for the usual camera), not a {}", other.type_name())),
                },
                // Sit a player in a kart to drive it, or (nil) get them out.
                "kart" if world.player(id).is_some() => match value {
                    Value::Nil => {
                        // (The engine lets them out beside the kart.)
                        world.player_mut(id).unwrap().kart = None;
                        Ok(())
                    }
                    Value::Object(o) => {
                        let mut kart = InstanceId::from_raw(o.id);
                        // A part of a kart counts as the kart.
                        if !brixo_core::is_kart(&world, kart) {
                            kart = brixo_core::kart_of(&world, kart).ok_or("that isn't a kart: a kart is a Model with the custom field kart = true")?;
                        }
                        let taken = world.walk().into_iter().any(|p| p != id && world.player(p).is_some_and(|pp| pp.kart == Some(kart)));
                        if taken {
                            return Err("someone's already driving that kart".into());
                        }
                        world.player_mut(id).unwrap().kart = Some(kart);
                        Ok(())
                    }
                    other => Err(format!("a player's kart has to be a kart (or nil to get out), not a {}", other.type_name())),
                },
                "x" | "y" | "width" | "height" | "text_size" | "text" | "background" | "visible" | "text_color"
                | "background_color"
                    if world.gui(id).is_some() =>
                {
                    let current = if name == "text_color" || name == "background_color" {
                        let facet = if name == "text_color" { FACET_TEXT_COLOR } else { FACET_BG_COLOR };
                        Some(self.to_color(&world, &value, color_of(&world, id, facet).unwrap())?)
                    } else {
                        None
                    };
                    let g = world.gui_mut(id).unwrap();
                    match name {
                        "x" => g.x = number(&value, name)?,
                        "y" => g.y = number(&value, name)?,
                        "width" => g.width = number(&value, name)?.max(0.0),
                        "height" => g.height = number(&value, name)?.max(0.0),
                        "text_size" => g.text_size = number(&value, name)?.clamp(4.0, 200.0),
                        "text" => {
                            // Numbers and true/false show as they'd print.
                            g.text = match &value {
                                Value::Str(t) => t.to_string(),
                                Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
                                Value::Num(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                other => return Err(format!("text has to be text or a number, not a {}", other.type_name())),
                            }
                        }
                        "background" | "visible" => {
                            let Value::Bool(flag) = value else {
                                return Err(format!("{name} has to be true or false, not a {}", value.type_name()));
                            };
                            if name == "background" { g.background = flag } else { g.visible = flag }
                        }
                        "text_color" => g.text_color = current.unwrap(),
                        _ => g.background_color = current.unwrap(),
                    }
                    Ok(())
                }
                "floating" if world.part(id).is_some() => {
                    let Value::Bool(f) = value else {
                        return Err(format!("floating has to be true or false, not a {}", value.type_name()));
                    };
                    world.part_mut(id).unwrap().floating = f;
                    Ok(())
                }
                "bounce" if world.part(id).is_some() => {
                    world.part_mut(id).unwrap().bounce = number(&value, name)?.clamp(0.0, 1.0);
                    Ok(())
                }
                "hinge" if world.part(id).is_some() => {
                    let names = brixo_core::Hinge::ALL.iter().map(|h| format!("\"{}\"", h.name())).collect::<Vec<_>>().join(", ");
                    let h = match &value {
                        Value::Str(t) => brixo_core::Hinge::from_name(&t.to_lowercase()),
                        Value::Nil | Value::Bool(false) => Some(brixo_core::Hinge::Off),
                        _ => None,
                    }
                    .ok_or_else(|| format!("a hinge turns a part around its \"y\" (height), \"x\" (width) or \"z\" (depth), or \"off\": one of {names}"))?;
                    let p = world.part_mut(id).unwrap();
                    p.hinge = h;
                    // A hinged part has to be loose to turn.
                    if h != brixo_core::Hinge::Off {
                        p.anchored = false;
                    }
                    Ok(())
                }
                "hinge_at" if world.part(id).is_some() => {
                    let names = brixo_core::Side::ALL.iter().map(|s| format!("\"{}\"", s.name())).collect::<Vec<_>>().join(", ");
                    let side = match &value {
                        Value::Str(t) => brixo_core::Side::from_name(&t.to_lowercase()),
                        _ => None,
                    }
                    .ok_or_else(|| format!("hinge_at is where on the part its hinge is: one of {names}"))?;
                    world.part_mut(id).unwrap().hinge_at = side;
                    Ok(())
                }
                "motor_speed" if world.part(id).is_some() => {
                    world.part_mut(id).unwrap().motor_speed = number(&value, name)?.clamp(-3600.0, 3600.0);
                    Ok(())
                }
                "swing_to" if world.part(id).is_some() => {
                    world.part_mut(id).unwrap().swing_to = match value {
                        Value::Nil => None,
                        v => Some(number(&v, name)?.clamp(-180.0, 180.0)),
                    };
                    Ok(())
                }
                "hinge_angle" if world.part(id).is_some() => {
                    Err("hinge_angle is how far the hinge has turned, so it can't be set: to turn it, set swing_to".into())
                }
                "volume" if world.sound(id).is_some() => {
                    world.sound_mut(id).unwrap().volume = number(&value, name)?.clamp(0.0, 1.0);
                    Ok(())
                }
                "material" => {
                    let names = brixo_core::Material::ALL.iter().map(|m| m.name()).collect::<Vec<_>>().join(", ");
                    let Value::Str(text) = &value else {
                        return Err(format!("material should be text, one of: {names}"));
                    };
                    let m = brixo_core::Material::from_name(text)
                        .ok_or_else(|| format!("there's no material called '{text}'. Try one of: {names}"))?;
                    world.part_mut(id).ok_or_else(|| format!("a {} doesn't have a material. Only parts do", class_name(class)))?.material = m;
                    Ok(())
                }
                "transparency" => {
                    let t = number(&value, name)?.clamp(0.0, 1.0);
                    world.part_mut(id).ok_or_else(|| format!("a {} doesn't have transparency. Only parts do", class_name(class)))?.transparency = t;
                    Ok(())
                }
                "attached_to" if world.gui(id).is_some() => {
                    let target = match value {
                        Value::Object(o) if o.facet == FACET_SELF => Some(InstanceId::from_raw(o.id)),
                        Value::Nil => None,
                        other => return Err(format!("attached_to has to be a part (or nil), not a {}", other.type_name())),
                    };
                    world.gui_mut(id).unwrap().attached_to = target;
                    Ok(())
                }
                "equipped" => Err("equipped changes when the player presses 1-9; move a Tool into the player to give it to them".to_string()),
                "shape" => {
                    let names = Shape::ALL.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
                    let Value::Str(text) = &value else {
                        return Err(format!("shape should be text, one of: {names}"));
                    };
                    let shape = Shape::from_name(text)
                        .ok_or_else(|| format!("there's no shape called '{text}'. Try one of: {names}"))?;
                    let p = world
                        .part_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have a shape. Only parts do", class_name(class)))?;
                    p.shape = shape;
                    Ok(())
                }
                "camera_mode" => {
                    let names = CameraMode::ALL.iter().map(|m| m.name()).collect::<Vec<_>>().join(", ");
                    let Value::Str(text) = &value else {
                        return Err(format!("camera_mode should be text, one of: {names}"));
                    };
                    let mode = CameraMode::from_name(text)
                        .ok_or_else(|| format!("there's no camera mode called '{text}'. Try one of: {names}"))?;
                    let p = world
                        .player_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have a camera_mode. Only players do", class_name(class)))?;
                    p.camera_mode = mode;
                    Ok(())
                }
                "face" => {
                    let Value::Str(text) = &value else {
                        return Err(format!("face should be text, one of: {}", face_names()));
                    };
                    let face = Face::from_name(text)
                        .ok_or_else(|| format!("there's no face called '{text}'. Try one of: {}", face_names()))?;
                    let p = world
                        .player_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have a face. Only players do", class_name(class)))?;
                    p.face = face;
                    Ok(())
                }
                // player.hats = ["crown", "cape"]: one per slot (head, face,
                // neck, back); [] takes them all off.
                "hats" => {
                    let Value::List(items) = &value else {
                        return Err("hats should be a list of names, like [\"crown\", \"cape\"]".into());
                    };
                    let mut wanted = Vec::new();
                    for v in items.read().unwrap().iter() {
                        let Value::Str(n) = v else { return Err("hats should be a list of names, like [\"crown\", \"cape\"]".into()) };
                        if brixo_core::Hat::from_name(n).is_none() {
                            let all: Vec<&str> = brixo_core::Hat::ALL.iter().map(|h| h.name()).collect();
                            return Err(format!("there's no hat called '{n}'. Try one of: {}", all.join(", ")));
                        }
                        wanted.push(n.to_string());
                    }
                    let p = world.player_mut(id).ok_or_else(|| format!("a {} doesn't have hats. Only players do", class_name(class)))?;
                    p.hats = brixo_core::Hat::list(&wanted);
                    Ok(())
                }
                "shirt" | "pants" | "tshirt" => {
                    fn names(all: impl Iterator<Item = &'static str>) -> String {
                        all.collect::<Vec<_>>().join(", ")
                    }
                    let p = world
                        .player_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have {name}. Only players do", class_name(class)))?;
                    match (name, &value) {
                        ("tshirt", Value::Nil) => p.tshirt = None,
                        ("shirt", Value::Str(t)) => {
                            p.shirt = brixo_core::Shirt::from_name(t).ok_or_else(|| {
                                format!("there's no shirt called '{t}'. Try one of: {}", names(brixo_core::Shirt::ALL.iter().map(|x| x.name())))
                            })?
                        }
                        ("pants", Value::Str(t)) => {
                            p.pants = brixo_core::Pants::from_name(t).ok_or_else(|| {
                                format!("there are no pants called '{t}'. Try one of: {}", names(brixo_core::Pants::ALL.iter().map(|x| x.name())))
                            })?
                        }
                        ("tshirt", Value::Str(t)) => {
                            p.tshirt = Some(brixo_core::TShirt::from_name(t).ok_or_else(|| {
                                format!("there's no t-shirt picture called '{t}'. Try one of: {}", names(brixo_core::TShirt::ALL.iter().map(|x| x.name())))
                            })?)
                        }
                        _ => return Err(format!("{name} should be text (the name of one){}", if name == "tshirt" { ", or nil for none" } else { "" })),
                    }
                    Ok(())
                }
                "skin_color" | "shirt_color" | "pants_color" | "shoes_color" => {
                    let facet = avatar_color_facet(name).unwrap();
                    let current = color_of(&world, id, facet)
                        .ok_or_else(|| format!("a {} doesn't have {name}. Only players do", class_name(class)))?;
                    let c = self.to_color(&world, &value, current)?;
                    *color_mut(&mut world, id, facet).unwrap() = c;
                    colored(&mut world, id, facet);
                    Ok(())
                }
                "health" | "max_health" | "walk_speed" | "jump_power" => {
                    let n = number(&value, name)?.max(0.0);
                    let p = world
                        .player_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have {name}. Only players do", class_name(class)))?;
                    match name {
                        // Health can't go above max_health; 0 means dead.
                        "health" => p.health = n.min(p.max_health),
                        "max_health" => {
                            p.max_health = n.max(1.0);
                            p.health = p.health.min(p.max_health);
                        }
                        "walk_speed" => p.walk_speed = n,
                        _ => p.jump_power = n,
                    }
                    Ok(())
                }
                "anchored" | "can_collide" => {
                    let Value::Bool(flag) = value else {
                        return Err(format!("{name} has to be true or false, not a {}", value.type_name()));
                    };
                    let p = world
                        .part_mut(id)
                        .ok_or_else(|| format!("a {} doesn't have {name}. Only parts do", class_name(class)))?;
                    if name == "anchored" {
                        p.anchored = flag;
                    } else {
                        p.can_collide = flag;
                    }
                    Ok(())
                }
                "children" => Err("children can't be set directly. Change a child's parent instead".to_string()),
                "parent" => match value {
                    Value::Object(p) if p.facet == FACET_SELF => {
                        let target = InstanceId::from_raw(p.id);
                        if world.get(target).is_none() {
                            return Err(gone());
                        }
                        if world.reparent(id, target) {
                            Ok(())
                        } else {
                            Err("can't move it there (it can't go inside itself or a script)".to_string())
                        }
                    }
                    Value::Nil => Err("to get rid of something, use destroy(it)".to_string()),
                    other => Err(format!("parent has to be an object, not a {}", other.type_name())),
                },
                "color" => {
                    let current = world.body(id).ok_or_else(|| format!("a {} doesn't have a color", class_name(class)))?.color;
                    let c = self.to_color(&world, &value, current)?;
                    world.body_mut(id).unwrap().color = c;
                    Ok(())
                }
                other => match vec_facet(other) {
                    Some(facet) => {
                        let p = world
                            .body(id)
                            .ok_or_else(|| format!("a {} doesn't have a {other}", class_name(class)))?;
                        let current = read_vec(p, facet);
                        let v = self.to_vec3(&world, &value, current)?;
                        write_vec(world.body_mut(id).unwrap(), facet, v);
                        Ok(())
                    }
                    None => {
                        if let Some(real) = near_miss(class, other) {
                            return Err(format!("a {} doesn't have '{other}'. Did you mean '{real}'?", class_name(class)));
                        }
                        // Anything else becomes a custom field: `p.cash = 100`.
                        let attr = match value {
                            Value::Num(n) => brixo_core::Attribute::Num(n),
                            Value::Str(t) => brixo_core::Attribute::Str(t.to_string()),
                            Value::Bool(b) => brixo_core::Attribute::Bool(b),
                            Value::Nil => {
                                world.get_mut(id).unwrap().attributes.remove(other);
                                return Ok(());
                            }
                            v => return Err(format!("custom fields can hold numbers, text or true/false, not a {}", v.type_name())),
                        };
                        world.get_mut(id).unwrap().attributes.insert(other.to_string(), attr);
                        Ok(())
                    }
                },
            },

            FACET_POSITION | FACET_SIZE | FACET_ROTATION | FACET_VELOCITY => {
                let n = number(&value, name)?;
                let p = world.body_mut(id).ok_or_else(gone)?;
                let mut v = read_vec(p, obj.facet);
                match name {
                    "x" => v.x = n,
                    "y" => v.y = n,
                    "z" => v.z = n,
                    other => return Err(format!("this only has x, y and z, not '{other}'")),
                }
                write_vec(p, obj.facet, v);
                Ok(())
            }

            FACET_CAMERA => match name {
                "mode" => {
                    let names = CameraMode::ALL.iter().map(|m| m.name()).collect::<Vec<_>>().join(", ");
                    let Value::Str(text) = &value else {
                        return Err(format!("camera.mode should be text, one of: {names}"));
                    };
                    let mode = CameraMode::from_name(text)
                        .ok_or_else(|| format!("there's no camera mode called '{text}'. Try one of: {names}"))?;
                    world.player_mut(id).ok_or_else(gone)?.camera_mode = mode;
                    Ok(())
                }
                other => Err(format!("the camera only has mode, not '{other}'")),
            },

            facet if is_color_facet(facet) => {
                let n = channel(&value, name)?;
                let c = color_mut(&mut world, id, facet).ok_or_else(gone)?;
                match name {
                    "r" => c.r = n,
                    "g" => c.g = n,
                    "b" => c.b = n,
                    other => return Err(format!("a color only has r, g and b, not '{other}'")),
                }
                colored(&mut world, id, facet);
                Ok(())
            }

            _ => Err("unknown object".to_string()),
        }
    }

    fn describe(&self, obj: ObjectRef) -> String {
        let world = self.world.lock();
        let id = InstanceId::from_raw(obj.id);
        let Some(inst) = world.get(id) else {
            return "<destroyed>".to_string();
        };
        match obj.facet {
            FACET_SELF => format!("{} \"{}\"", class_name(inst.class), inst.name),
            FACET_CAMERA => "camera".to_string(),
            facet if is_color_facet(facet) => match color_of(&world, id, facet) {
                Some(c) => format!("color({}, {}, {})", c.r, c.g, c.b),
                None => "<destroyed>".to_string(),
            },
            facet => match world.body(id) {
                Some(p) => {
                    let v = read_vec(p, facet);
                    format!(
                        "({}, {}, {})",
                        format_number(v.x as f64),
                        format_number(v.y as f64),
                        format_number(v.z as f64)
                    )
                }
                None => "<destroyed>".to_string(),
            },
        }
    }

    fn type_name(&self, obj: ObjectRef) -> String {
        match obj.facet {
            FACET_SELF => {
                let world = self.world.lock();
                world
                    .get(InstanceId::from_raw(obj.id))
                    .map(|i| class_name(i.class))
                    .unwrap_or("destroyed")
                    .to_string()
            }
            FACET_CAMERA => "camera".to_string(),
            facet if is_color_facet(facet) => "color".to_string(),
            _ => "vector".to_string(),
        }
    }

    fn function_names(&self) -> Vec<&'static str> {
        vec!["find", "destroy", "clone", "time", "players", "create", "play_sound", "play_sound_at", "play_music", "stop_music", "explode", "save", "load", "complete_challenge", "leaderboard", "boost", "spin_out", "place_kart", "add_bot"]
    }

    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        let need = |n: usize| -> Result<(), String> {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!("{name} needs {n} value(s) but got {}", args.len()))
            }
        };
        let instance_arg = |v: &Value| -> Result<InstanceId, String> {
            match v {
                Value::Object(o) if o.facet == FACET_SELF => Ok(InstanceId::from_raw(o.id)),
                other => Err(format!("{name} needs an object, like self, but got a {}", other.type_name())),
            }
        };

        match name {
            "find" => {
                need(1)?;
                let Value::Str(target) = &args[0] else {
                    return Err("find needs a name, like find(\"Coin\")".to_string());
                };
                let world = self.world.lock();
                Ok(world.find_first(target).map(object).unwrap_or(Value::Nil))
            }
            "destroy" => {
                need(1)?;
                let id = instance_arg(&args[0])?;
                let mut world = self.world.lock();
                if id == world.root() {
                    return Err("the workspace can't be destroyed".to_string());
                }
                // Destroying something already gone is harmless.
                world.remove(id);
                Ok(Value::Nil)
            }
            "clone" => {
                need(1)?;
                let id = instance_arg(&args[0])?;
                let mut world = self.world.lock();
                if world.get(id).is_none() {
                    return Err(gone());
                }
                world
                    .clone_subtree(id)
                    .map(object)
                    .ok_or_else(|| "the workspace can't be cloned".to_string())
            }
            "time" => {
                need(0)?;
                Ok(Value::Num(*self.clock.lock().unwrap()))
            }
            "create" => {
                need(2)?;
                let names = CREATABLE.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
                let Value::Str(wanted) = &args[0] else {
                    return Err(format!("create needs a class name first, like create(\"Part\", self). You can create: {names}"));
                };
                let class = CREATABLE
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(wanted))
                    .map(|(_, c)| *c)
                    .ok_or_else(|| format!("can't create a '{wanted}'. You can create: {names}"))?;
                let parent = instance_arg(&args[1])?;
                let mut world = self.world.lock();
                let name = CREATABLE.iter().find(|(_, c)| *c == class).unwrap().0;
                world
                    .create(class, name, parent)
                    .map(object)
                    .ok_or_else(|| "that can't hold other things (scripts can't have children)".to_string())
            }
            "explode" => {
                if !(2..=3).contains(&args.len()) {
                    return Err("explode needs a position and a radius, and optionally a power: explode(self.position, 12)".into());
                }
                let mut world = self.world.lock();
                let center = self.to_vec3(&world, &args[0], Vec3::ZERO)?;
                let radius = number(&args[1], "the radius")?;
                if !(0.5..=100.0).contains(&radius) {
                    return Err("an explosion's radius has to be between 0.5 and 100".into());
                }
                let power = match args.get(2) {
                    Some(v) => number(v, "the power")?.clamp(0.0, 400.0),
                    None => 70.0,
                };
                // Players caught in it are knocked out; they're returned so
                // scripts can say who did it.
                let mut caught = Vec::new();
                for id in world.walk() {
                    let Some(p) = world.player_mut(id) else { continue };
                    let d = p.body.position;
                    let dist = ((d.x - center.x).powi(2) + (d.y - center.y).powi(2) + (d.z - center.z).powi(2)).sqrt();
                    if dist < radius + 1.5 && p.dead == 0.0 && p.health > 0.0 {
                        p.health = 0.0;
                        caught.push(object(id));
                    }
                }
                break_and_collapse(&mut world, center, radius, power);
                self.blasts.lock().unwrap().push(Blast { center, radius, power });
                self.sounds.lock().unwrap().push(SoundEvent::Play {
                    name: "boom".into(),
                    player: None,
                    at: Some(brixo_core::SoundAt { object: None, position: center }),
                });
                Ok(Value::list(caught))
            }
            // play_sound_at("hit", other): heard from there by everyone,
            // quieter further away. `where` is a part or player (the sound
            // follows it), anything with parts in it, or a position.
            "play_sound_at" => {
                let usage = "play_sound_at needs a sound and where it comes from: play_sound_at(\"hit\", other) or play_sound_at(\"boom\", self.position)";
                need(2).map_err(|_| usage.to_string())?;
                let wanted = self.sound_name(&args[0], "play_sound_at")?;
                let world = self.world.lock();
                let at = match &args[1] {
                    Value::Object(o) if o.facet == FACET_SELF => {
                        let id = InstanceId::from_raw(o.id);
                        let position = brixo_core::object_position(&world, id)
                            .ok_or_else(|| format!("play_sound_at: {} isn't anywhere (it has no parts), so the sound can't come from it", world.get(id).map(|i| i.name.clone()).unwrap_or_default()))?;
                        brixo_core::SoundAt { object: Some(id), position }
                    }
                    other => brixo_core::SoundAt { object: None, position: self.to_vec3(&world, other, Vec3::ZERO).map_err(|e| format!("{usage} ({e})"))? },
                };
                drop(world);
                self.sounds.lock().unwrap().push(SoundEvent::Play { name: wanted, player: None, at: Some(at) });
                Ok(Value::Nil)
            }
            "play_sound" | "play_music" | "stop_music" => {
                let music = name != "play_sound";
                let (wanted, player_arg) = if name == "stop_music" {
                    (None, args.first())
                } else {
                    if args.is_empty() || args.len() > 2 {
                        return Err(format!("{name} needs a name, and optionally a player: {name}(\"{}\", player)", if music { "sunny" } else { "coin" }));
                    }
                    let list: &[&str] = if music { &brixo_core::MUSIC } else { &brixo_core::SOUNDS };
                    let wanted = match &args[0] {
                        // One of Brixo's built-in sounds or tracks...
                        Value::Str(n) if list.contains(&n.as_ref()) => n.to_string(),
                        Value::Str(n) => {
                            return Err(format!("there's no {} called '{n}'. Try one of: {}, or a Sound in your game: {name}(find(\"My Sound\"))", if music { "music" } else { "sound" }, list.join(", ")));
                        }
                        // ...or a Sound in the game (an audio file dropped on the studio).
                        Value::Object(o) if o.facet == FACET_SELF && self.world.lock().sound(InstanceId::from_raw(o.id)).is_some() => format!("#{}", o.id),
                        other => return Err(format!("{name} needs a sound's name or a Sound, not a {}", other.type_name())),
                    };
                    (Some(wanted), args.get(1))
                };
                let player = match player_arg {
                    Some(v) => Some(instance_arg(v)?),
                    None => None,
                };
                // The second value is who hears it: only a player makes sense.
                if let Some(id) = player {
                    if self.world.lock().player(id).is_none() {
                        return Err(format!("{name}(sound, player) plays it for just that player. To play a sound from a part, use play_sound_at(sound, part)"));
                    }
                }
                let event = if music {
                    SoundEvent::Music { name: wanted, player }
                } else {
                    SoundEvent::Play { name: wanted.unwrap(), player, at: None }
                };
                self.sounds.lock().unwrap().push(event);
                Ok(Value::Nil)
            }
            "save" | "load" => {
                let usage = if name == "save" { "save(player, \"coins\", player.coins)" } else { "load(player, \"coins\")" };
                need(if name == "save" { 3 } else { 2 }).map_err(|_| format!("{name} works like {usage}"))?;
                let player = instance_arg(&args[0])?;
                {
                    let world = self.world.lock();
                    if world.player(player).is_none() {
                        return Err(format!("{name} needs a player first: {usage}"));
                    }
                    // Bots have nothing saved and keep nothing.
                    if matches!(world.get(player).and_then(|i| i.attributes.get("bot")), Some(brixo_core::Attribute::Bool(true))) {
                        return Ok(Value::Nil);
                    }
                }
                let Value::Str(key) = &args[1] else {
                    return Err(format!("{name} needs a name for the value, in quotes: {usage}"));
                };
                let mut saves = self.saves.lock().unwrap();
                if name == "save" {
                    saves.save(player, key, &args[2])?;
                    Ok(Value::Nil)
                } else {
                    saves.load(player, key)
                }
            }
            // complete_challenge(player, "win_round"): gives them the
            // challenge's Brix (once, or once a day), says so in the chat,
            // and gives back how many Brix they got.
            "complete_challenge" => {
                let usage = "complete_challenge(player, \"win_round\")";
                need(2).map_err(|_| format!("complete_challenge works like {usage}"))?;
                let player = instance_arg(&args[0])?;
                let Value::Str(challenge) = &args[1] else {
                    return Err(format!("complete_challenge needs the challenge's name, in quotes: {usage}"));
                };
                let ok = !challenge.is_empty()
                    && challenge.chars().count() <= 40
                    && challenge.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ');
                if !ok {
                    return Err("a challenge's name is 1 to 40 letters, numbers, spaces or _".into());
                }
                let who = {
                    let world = self.world.lock();
                    if world.player(player).is_none() {
                        return Err(format!("complete_challenge needs a player first: {usage}"));
                    }
                    if matches!(world.get(player).and_then(|i| i.attributes.get("bot")), Some(brixo_core::Attribute::Bool(true))) {
                        return Ok(Value::Num(0.0));
                    }
                    world.get(player).map(|i| i.name.clone()).unwrap_or_default()
                };
                let done = self.saves.lock().unwrap().challenge(player, &challenge.to_lowercase());
                let Some(done) = done else { return Ok(Value::Num(0.0)) };
                let paid = if done.brix > 0 { format!(" (+{} Brix)", done.brix) } else { String::new() };
                self.notices.lock().unwrap().push(format!("{who} completed {}!{paid}", done.title));
                Ok(Value::Num(done.brix as f64))
            }
            // leaderboard("coins", "wins"): these player fields become the
            // leaderboard's columns (sorted by the first). leaderboard()
            // takes it away.
            "leaderboard" => {
                if args.len() > 5 {
                    return Err("a leaderboard can show up to 5 things: leaderboard(\"coins\", \"wins\")".into());
                }
                let mut columns = Vec::new();
                for a in args {
                    let Value::Str(field) = a else {
                        return Err(format!(
                            "leaderboard needs the names of player fields, in quotes: leaderboard(\"coins\") shows each player's p.coins (not a {})",
                            a.type_name()
                        ));
                    };
                    let ok = !field.is_empty()
                        && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !field.chars().next().unwrap().is_ascii_digit();
                    if !ok {
                        return Err(format!("'{field}' isn't a field name: use the name you give it on the player, like \"coins\" for p.coins"));
                    }
                    if field.chars().count() > 20 {
                        return Err(format!("'{field}' is too long for a leaderboard heading (20 letters at most)"));
                    }
                    columns.push(field.to_string());
                }
                let mut world = self.world.lock();
                let root = world.root();
                let attrs = &mut world.get_mut(root).unwrap().attributes;
                if columns.is_empty() {
                    attrs.remove(brixo_core::LEADERBOARD_FIELD);
                } else {
                    attrs.insert(brixo_core::LEADERBOARD_FIELD.into(), brixo_core::Attribute::Str(columns.join(",")));
                }
                Ok(Value::Nil)
            }
            // Karts: boost(kart, seconds), spin_out(kart),
            // place_kart(kart, position, facing).
            "boost" | "spin_out" | "place_kart" => {
                let usage = match name {
                    "boost" => "boost(kart, 1.5)",
                    "spin_out" => "spin_out(kart)",
                    _ => "place_kart(kart, {x = 0, y = 3, z = 0}, 90)",
                };
                let n = match name {
                    "boost" => 2,
                    "spin_out" => 1,
                    _ => 3,
                };
                need(n).map_err(|_| format!("{name} works like {usage}"))?;
                let world = self.world.lock();
                let mut kart = instance_arg(&args[0]).map_err(|_| format!("{name} needs a kart first: {usage}"))?;
                if !brixo_core::is_kart(&world, kart) {
                    kart = brixo_core::kart_of(&world, kart).ok_or_else(|| format!("{name} needs a kart (a Model with kart = true): {usage}"))?;
                }
                let command = match name {
                    "boost" => crate::physics::KartCommand::Boost(kart, number(&args[1], "seconds")?),
                    "spin_out" => crate::physics::KartCommand::SpinOut(kart),
                    _ => {
                        let at = self.to_vec3(&world, &args[1], Vec3::new(0.0, 0.0, 0.0))?;
                        crate::physics::KartCommand::Place(kart, glam::Vec3::new(at.x, at.y, at.z), number(&args[2], "facing")?)
                    }
                };
                drop(world);
                self.kart_commands.lock().unwrap().push(command);
                Ok(Value::Nil)
            }
            // add_bot(name): a computer player, to drive a kart (its
            // `kart` set like any player's). Bots follow the racing line.
            "add_bot" => {
                need(1).map_err(|_| "add_bot works like add_bot(\"Bolt\")".to_string())?;
                let Value::Str(wanted) = &args[0] else {
                    return Err("add_bot needs the bot's name, in quotes: add_bot(\"Bolt\")".into());
                };
                let mut world = self.world.lock();
                let taken = |w: &DataModel, n: &str| w.walk().into_iter().any(|p| w.player(p).is_some() && w.get(p).is_some_and(|i| i.name == n));
                let mut name = wanted.to_string();
                let mut k = 2;
                while taken(&world, &name) {
                    name = format!("{wanted}{k}");
                    k += 1;
                }
                let root = world.root();
                let spawn = world
                    .walk()
                    .into_iter()
                    .find(|i| world.get(*i).is_some_and(|x| x.class == Class::SpawnLocation))
                    .and_then(|i| world.part(i).copied());
                let id = world.create(Class::Player, &name, root).ok_or("couldn't make a bot")?;
                {
                    let p = world.player_mut(id).unwrap();
                    crate::game::random_colors(p, &mut crate::game::Rng::seeded());
                    if let Some(s) = spawn {
                        p.body.position = Vec3::new(s.position.x, s.position.y + s.size.y / 2.0 + 2.55, s.position.z);
                    }
                }
                world.get_mut(id).unwrap().attributes.insert("bot".into(), brixo_core::Attribute::Bool(true));
                drop(world);
                self.new_bots.lock().unwrap().push(id);
                Ok(object(id))
            }
            "players" => {
                need(0)?;
                let world = self.world.lock();
                let players = world.walk().into_iter().filter(|id| world.player(*id).is_some()).map(object);
                Ok(Value::list(players.collect()))
            }
            other => Err(format!("unknown function '{other}'")),
        }
    }
}
