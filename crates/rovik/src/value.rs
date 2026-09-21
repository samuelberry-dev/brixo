//! Runtime values, the host interface, and variable scopes.
//!
//! Everything here is thread-safe (Arc + RwLock) so a game can run each
//! script task on its own thread. Only one task ever runs at a time, so the
//! locks never actually wait on each other.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use crate::ast::FnDef;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Builtin {
    Print,
    Len,
    Str,
    Num,
    Type,
    Push,
    Pop,
    Insert,
    Remove,
    Keys,
    Wait,
    Floor,
    Round,
    Abs,
    Min,
    Max,
    Sqrt,
    Random,
}

impl Builtin {
    pub const ALL: &'static [(&'static str, Builtin)] = &[
        ("print", Builtin::Print),
        ("len", Builtin::Len),
        ("str", Builtin::Str),
        ("num", Builtin::Num),
        ("type", Builtin::Type),
        ("push", Builtin::Push),
        ("pop", Builtin::Pop),
        ("insert", Builtin::Insert),
        ("remove", Builtin::Remove),
        ("keys", Builtin::Keys),
        ("wait", Builtin::Wait),
        ("floor", Builtin::Floor),
        ("round", Builtin::Round),
        ("abs", Builtin::Abs),
        ("min", Builtin::Min),
        ("max", Builtin::Max),
        ("sqrt", Builtin::Sqrt),
        ("random", Builtin::Random),
    ];

    pub fn lookup(name: &str) -> Option<Builtin> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, b)| *b == self)
            .map(|(n, _)| *n)
            .unwrap_or("?")
    }
}

/// A handle to something the host owns, like a part in the game world.
/// `facet` lets one object hand out sub-objects: the engine uses it so
/// `self.position` is a live view of the part's position, and
/// `self.position.y += 1` really moves the part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectRef {
    pub id: u64,
    pub facet: u32,
}

/// What a program embedding Rovik (like the Brixo engine) provides.
/// Errors are plain messages; the interpreter adds the line number.
pub trait Host: Send + Sync {
    fn get_field(&self, obj: ObjectRef, name: &str) -> Result<Value, String>;
    fn set_field(&self, obj: ObjectRef, name: &str, value: Value) -> Result<(), String>;
    /// How the object prints.
    fn describe(&self, obj: ObjectRef) -> String;
    /// What type() says for the object.
    fn type_name(&self, obj: ObjectRef) -> String;
    /// Names of extra global functions, like find and destroy.
    fn function_names(&self) -> Vec<&'static str>;
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String>;
}

pub struct Closure {
    pub def: Arc<FnDef>,
    pub env: Arc<Env>,
}

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Arc<str>),
    List(Arc<RwLock<Vec<Value>>>),
    /// Sorted by key so printing is predictable.
    Map(Arc<RwLock<BTreeMap<String, Value>>>),
    Func(Arc<Closure>),
    Builtin(Builtin),
    /// A function the host provides.
    HostFn(Arc<str>),
    /// Something the host owns.
    Object(ObjectRef),
}

impl Value {
    pub fn str(s: impl Into<Arc<str>>) -> Value {
        Value::Str(s.into())
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Arc::new(RwLock::new(items)))
    }

    pub fn map(entries: BTreeMap<String, Value>) -> Value {
        Value::Map(Arc::new(RwLock::new(entries)))
    }

    /// Only `nil` and `false` count as false.
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    /// The type name beginners see. Objects ask the host instead.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "text",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Func(_) | Value::Builtin(_) | Value::HostFn(_) => "function",
            Value::Object(_) => "object",
        }
    }

    /// Number of parameters, for functions written in Rovik.
    pub fn param_count(&self) -> Option<usize> {
        match self {
            Value::Func(c) => Some(c.def.params.len()),
            _ => None,
        }
    }

    /// Plain display without a host (objects show as <object>).
    pub fn display(&self) -> String {
        show(self, None, false, 0)
    }
}

/// Formats a value. `quoted` puts quotes around text (used inside lists).
pub fn show(value: &Value, host: Option<&dyn Host>, quoted: bool, depth: usize) -> String {
    if depth > 16 {
        return "...".to_string();
    }
    match value {
        Value::Nil => "nil".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => format_number(*n),
        Value::Str(s) if quoted => format!("\"{s}\""),
        Value::Str(s) => s.to_string(),
        Value::List(items) => {
            let items = items.read().unwrap().clone();
            let parts: Vec<String> = items
                .iter()
                .map(|v| show(v, host, true, depth + 1))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(entries) => {
            let entries = entries.read().unwrap().clone();
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{k} = {}", show(v, host, true, depth + 1)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Func(c) => match &c.def.name {
            Some(name) => format!("<fn {name}>"),
            None => "<fn>".to_string(),
        },
        Value::Builtin(b) => format!("<fn {}>", b.name()),
        Value::HostFn(name) => format!("<fn {name}>"),
        Value::Object(obj) => match host {
            Some(h) => h.describe(*obj),
            None => "<object>".to_string(),
        },
    }
}

/// Whole numbers print without a decimal point: 10, not 10.0.
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Deep equality: two lists with the same items are equal.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            if Arc::ptr_eq(x, y) {
                return true;
            }
            let x = x.read().unwrap().clone();
            let y = y.read().unwrap().clone();
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Value::Map(x), Value::Map(y)) => {
            if Arc::ptr_eq(x, y) {
                return true;
            }
            let x = x.read().unwrap().clone();
            let y = y.read().unwrap().clone();
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && values_equal(va, vb))
        }
        (Value::Func(x), Value::Func(y)) => Arc::ptr_eq(x, y),
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::HostFn(x), Value::HostFn(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        _ => false,
    }
}

/// A scope. The script has one at the top; each function call adds one
/// whose parent is the scope the function was defined in.
pub struct Env {
    vars: RwLock<HashMap<String, Value>>,
    parent: Option<Arc<Env>>,
}

impl Env {
    pub fn new_global() -> Arc<Env> {
        Arc::new(Env {
            vars: RwLock::new(HashMap::new()),
            parent: None,
        })
    }

    pub fn new_child(parent: Arc<Env>) -> Arc<Env> {
        Arc::new(Env {
            vars: RwLock::new(HashMap::new()),
            parent: Some(parent),
        })
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let found = self.vars.read().unwrap().get(name).cloned();
        match found {
            Some(v) => Some(v),
            None => self.parent.as_ref().and_then(|p| p.get(name)),
        }
    }

    /// Rovik's assignment rule: update the variable if it already exists in
    /// this scope or any outer one; otherwise create it here. At the top of a
    /// script "here" is the script scope; inside a function it's the function.
    pub fn assign(&self, name: &str, value: Value) {
        if !self.set_existing(name, &value) {
            self.vars.write().unwrap().insert(name.to_string(), value);
        }
    }

    fn set_existing(&self, name: &str, value: &Value) -> bool {
        {
            let mut vars = self.vars.write().unwrap();
            if let Some(slot) = vars.get_mut(name) {
                *slot = value.clone();
                return true;
            }
        }
        match &self.parent {
            Some(p) => p.set_existing(name, value),
            None => false,
        }
    }

    pub fn define(&self, name: &str, value: Value) {
        self.vars.write().unwrap().insert(name.to_string(), value);
    }

    /// Every name visible from here, for "did you mean" suggestions.
    pub fn visible_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.vars.read().unwrap().keys().cloned().collect();
        if let Some(p) = &self.parent {
            names.extend(p.visible_names());
        }
        names
    }
}
