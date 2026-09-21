//! Lets Rovik scripts see and change the game world.
//!
//! Parts reach scripts as objects. Their vector properties are "facets" of
//! the same object, so `self.position` is a live view of the real position:
//! `self.position.y += 1` moves the part, rather than changing a copy.

use std::sync::{Arc, Mutex};

use brixo_core::{CameraMode, Class, Color, DataModel, Face, InstanceId, PartProps, Vec3};
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

fn is_color_facet(facet: u32) -> bool {
    facet == FACET_COLOR || (FACET_SKIN..=FACET_SHOES).contains(&facet)
}

/// Reads whichever colour a colour facet points at.
fn color_of(world: &DataModel, id: InstanceId, facet: u32) -> Option<Color> {
    if facet == FACET_COLOR {
        return world.body(id).map(|p| p.color);
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
    let p = world.player_mut(id)?;
    Some(match facet {
        FACET_SKIN => &mut p.skin_color,
        FACET_SHIRT => &mut p.shirt_color,
        FACET_PANTS => &mut p.pants_color,
        _ => &mut p.shoes_color,
    })
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
    pub world: Arc<Mutex<DataModel>>,
    /// Seconds since the game started, for time().
    pub clock: Arc<Mutex<f64>>,
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
        Class::Player => "player",
    }
}

fn gone() -> String {
    "this object was destroyed, so it can't be used any more".to_string()
}

fn vec_facet(name: &str) -> Option<u32> {
    match name {
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
        FACET_SIZE => p.size,
        _ => p.rotation,
    }
}

fn write_vec(p: &mut PartProps, facet: u32, v: Vec3) {
    match facet {
        FACET_POSITION => p.position = v,
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

fn part_fields_hint() -> &'static str {
    "Parts have name, position, size, rotation, color, anchored, can_collide and parent"
}

impl WorldHost {
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
            Value::Object(o) if matches!(o.facet, FACET_POSITION | FACET_SIZE | FACET_ROTATION) => {
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
        let world = self.world.lock().unwrap();
        let id = InstanceId::from_raw(obj.id);
        let inst = world.get(id).ok_or_else(gone)?;

        match obj.facet {
            FACET_SELF => match name {
                "name" => Ok(Value::str(inst.name.as_str())),
                "class" => Ok(Value::str(class_name(inst.class))),
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
                "skin_color" | "shirt_color" | "pants_color" | "shoes_color" => match world.player(id) {
                    Some(_) => Ok(facet_object(id, avatar_color_facet(name).unwrap())),
                    None => Err(format!("a {} doesn't have {name}. Only players do", class_name(inst.class))),
                },
                "health" | "max_health" | "walk_speed" | "jump_power" => match world.player(id) {
                    Some(p) => Ok(Value::Num(match name {
                        "health" => p.health,
                        "max_health" => p.max_health,
                        "walk_speed" => p.walk_speed,
                        _ => p.jump_power,
                    } as f64)),
                    None => Err(format!("a {} doesn't have {name}. Only players do", class_name(inst.class))),
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
                    None if inst.class == Class::Part => {
                        Err(format!("a part doesn't have '{other}'. {}", part_fields_hint()))
                    }
                    None => Err(format!(
                        "a {} doesn't have '{other}'",
                        class_name(inst.class)
                    )),
                },
            },

            FACET_POSITION | FACET_SIZE | FACET_ROTATION => {
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
        let mut world = self.world.lock().unwrap();
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
                "skin_color" | "shirt_color" | "pants_color" | "shoes_color" => {
                    let facet = avatar_color_facet(name).unwrap();
                    let current = color_of(&world, id, facet)
                        .ok_or_else(|| format!("a {} doesn't have {name}. Only players do", class_name(class)))?;
                    let c = self.to_color(&world, &value, current)?;
                    *color_mut(&mut world, id, facet).unwrap() = c;
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
                    None => Err(format!(
                        "a {} doesn't have '{other}' to set",
                        class_name(class)
                    )),
                },
            },

            FACET_POSITION | FACET_SIZE | FACET_ROTATION => {
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
                Ok(())
            }

            _ => Err("unknown object".to_string()),
        }
    }

    fn describe(&self, obj: ObjectRef) -> String {
        let world = self.world.lock().unwrap();
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
                let world = self.world.lock().unwrap();
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
        vec!["find", "destroy", "clone", "time", "players"]
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
                let world = self.world.lock().unwrap();
                Ok(world.find_first(target).map(object).unwrap_or(Value::Nil))
            }
            "destroy" => {
                need(1)?;
                let id = instance_arg(&args[0])?;
                let mut world = self.world.lock().unwrap();
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
                let mut world = self.world.lock().unwrap();
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
            "players" => {
                need(0)?;
                let world = self.world.lock().unwrap();
                let players = world.walk().into_iter().filter(|id| world.player(*id).is_some()).map(object);
                Ok(Value::list(players.collect()))
            }
            other => Err(format!("unknown function '{other}'")),
        }
    }
}
