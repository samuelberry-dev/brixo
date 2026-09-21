//! Lets Rovik scripts see and change the game world.
//!
//! Parts reach scripts as objects. Their vector properties are "facets" of
//! the same object, so `self.position` is a live view of the real position:
//! `self.position.y += 1` moves the part, rather than changing a copy.

use std::sync::{Arc, Mutex};

use brixo_core::{Class, Color, DataModel, InstanceId, PartProps, Vec3};
use rovik::value::format_number;
use rovik::{Host, ObjectRef, Value};

pub const FACET_SELF: u32 = 0;
pub const FACET_POSITION: u32 = 1;
pub const FACET_SIZE: u32 = 2;
pub const FACET_ROTATION: u32 = 3;
pub const FACET_COLOR: u32 = 4;

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
            Value::Object(o) if o.facet == FACET_COLOR => {
                let p = world.body(InstanceId::from_raw(o.id)).ok_or_else(gone)?;
                Ok(p.color)
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

            FACET_COLOR => {
                let c = world.body(id).ok_or_else(gone)?.color;
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

            FACET_COLOR => {
                let n = channel(&value, name)?;
                let p = world.body_mut(id).ok_or_else(gone)?;
                match name {
                    "r" => p.color.r = n,
                    "g" => p.color.g = n,
                    "b" => p.color.b = n,
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
            FACET_COLOR => match world.body(id) {
                Some(p) => format!("color({}, {}, {})", p.color.r, p.color.g, p.color.b),
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
            FACET_COLOR => "color".to_string(),
            _ => "vector".to_string(),
        }
    }

    fn function_names(&self) -> Vec<&'static str> {
        vec!["find", "destroy", "clone", "time"]
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
            other => Err(format!("unknown function '{other}'")),
        }
    }
}
