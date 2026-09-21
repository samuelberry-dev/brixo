//! Runtime values and variable scopes.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

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
        Self::ALL.iter().find(|(_, b)| *b == self).map(|(n, _)| *n).unwrap_or("?")
    }
}

pub struct Closure {
    pub def: Rc<FnDef>,
    pub env: Rc<Env>,
}

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    /// Sorted by key so printing is predictable.
    Map(Rc<RefCell<BTreeMap<String, Value>>>),
    Func(Rc<Closure>),
    Builtin(Builtin),
}

impl Value {
    pub fn str(s: impl Into<Rc<str>>) -> Value {
        Value::Str(s.into())
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn map(entries: BTreeMap<String, Value>) -> Value {
        Value::Map(Rc::new(RefCell::new(entries)))
    }

    /// Only `nil` and `false` count as false.
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    /// The type name beginners see, in type() and in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "text",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Func(_) | Value::Builtin(_) => "function",
        }
    }

    /// How the value appears when printed or joined into text.
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            other => other.repr(),
        }
    }

    /// Like display, but text is quoted (used inside lists and maps).
    pub fn repr(&self) -> String {
        match self {
            Value::Nil => "nil".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Num(n) => format_number(*n),
            Value::Str(s) => format!("\"{s}\""),
            Value::List(items) => {
                let parts: Vec<String> = items.borrow().iter().map(|v| v.repr()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Map(entries) => {
                let parts: Vec<String> = entries
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", v.repr()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Func(c) => match &c.def.name {
                Some(name) => format!("<fn {name}>"),
                None => "<fn>".to_string(),
            },
            Value::Builtin(b) => format!("<fn {}>", b.name()),
        }
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
            if Rc::ptr_eq(x, y) {
                return true;
            }
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Value::Map(x), Value::Map(y)) => {
            if Rc::ptr_eq(x, y) {
                return true;
            }
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && values_equal(va, vb))
        }
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        _ => false,
    }
}

/// A scope. The script has one at the top; each function call adds one
/// whose parent is the scope the function was defined in.
pub struct Env {
    vars: RefCell<HashMap<String, Value>>,
    parent: Option<Rc<Env>>,
}

impl Env {
    pub fn new_global() -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            parent: None,
        })
    }

    pub fn new_child(parent: Rc<Env>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            parent: Some(parent),
        })
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(name) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.get(name))
    }

    /// Rovik's assignment rule: update the variable if it already exists in
    /// this scope or any outer one; otherwise create it here. At the top of a
    /// script "here" is the script scope; inside a function it's the function.
    pub fn assign(&self, name: &str, value: Value) {
        if !self.set_existing(name, &value) {
            self.vars.borrow_mut().insert(name.to_string(), value);
        }
    }

    fn set_existing(&self, name: &str, value: &Value) -> bool {
        if let Some(slot) = self.vars.borrow_mut().get_mut(name) {
            *slot = value.clone();
            return true;
        }
        match &self.parent {
            Some(p) => p.set_existing(name, value),
            None => false,
        }
    }

    pub fn define(&self, name: &str, value: Value) {
        self.vars.borrow_mut().insert(name.to_string(), value);
    }

    /// Every name visible from here, for "did you mean" suggestions.
    pub fn visible_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.vars.borrow().keys().cloned().collect();
        if let Some(p) = &self.parent {
            names.extend(p.visible_names());
        }
        names
    }
}
