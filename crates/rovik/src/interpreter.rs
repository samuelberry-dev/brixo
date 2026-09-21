//! Walks the syntax tree and runs it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ast::*;
use crate::error::{Result, RovikError};
use crate::value::{format_number, show, values_equal, Builtin, Closure, Env, Host, ObjectRef, Value};

/// Default number of statements a script may run before it has to pause
/// with wait(). Stops a `while true do` from freezing everything.
pub const STEP_LIMIT: u64 = 10_000_000;
/// How deep function calls may nest.
pub const CALL_DEPTH_LIMIT: usize = 200;

/// What a block of statements did when it finished.
enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// `on touched(player)`
    Event(String),
    /// `every 5 seconds`
    Every(f64),
}

/// An `on` or `every` block the script registered. In milestone 1 these are
/// recorded but not run; the engine will run them once Rovik is inside Brixo.
#[derive(Clone)]
pub struct Handler {
    pub trigger: Trigger,
    pub function: Value,
}

/// Called by print(). Lets a game send output to its own log.
pub type PrintHook = Arc<dyn Fn(&str) + Send + Sync>;
/// Called by wait(). Lets a game pause just this task; returning an error
/// stops the script (the game uses that when it's stopped).
pub type WaitHook = Arc<dyn Fn(f64) -> std::result::Result<(), String> + Send + Sync>;

pub struct Interpreter {
    globals: Arc<Env>,
    handlers: Arc<Mutex<Vec<Handler>>>,
    /// Everything print() produced, when no print hook is set.
    pub output: Vec<String>,
    /// Also write print() output to the terminal as it happens.
    pub echo: bool,
    /// Statements allowed between waits.
    pub step_limit: u64,
    pub host: Option<Arc<dyn Host>>,
    pub on_print: Option<PrintHook>,
    pub on_wait: Option<WaitHook>,
    steps: u64,
    depth: usize,
    rng: u64,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        Self {
            globals: Env::new_global(),
            handlers: Arc::new(Mutex::new(Vec::new())),
            output: Vec::new(),
            echo: false,
            step_limit: STEP_LIMIT,
            host: None,
            on_print: None,
            on_wait: None,
            steps: 0,
            depth: 0,
            rng: seed | 1,
        }
    }

    /// A second interpreter for the same script: it shares the script's
    /// variables, handlers and host, but keeps its own call depth and step
    /// count. A game runs each task (the script body, each event) on a fork.
    pub fn fork(&self) -> Interpreter {
        Interpreter {
            globals: self.globals.clone(),
            handlers: self.handlers.clone(),
            output: Vec::new(),
            echo: self.echo,
            step_limit: self.step_limit,
            host: self.host.clone(),
            on_print: self.on_print.clone(),
            on_wait: None,
            steps: 0,
            depth: 0,
            rng: self.rng.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Set a script-level variable before the script runs (like `self`).
    pub fn define_global(&self, name: &str, value: Value) {
        self.globals.define(name, value);
    }

    /// The `on` and `every` blocks registered so far.
    pub fn handlers(&self) -> Vec<Handler> {
        self.handlers.lock().unwrap().clone()
    }

    /// Formats a value the way print() does.
    pub fn display(&self, value: &Value) -> String {
        show(value, self.host.as_deref(), false, 0)
    }

    /// Lex, parse and run a whole script.
    pub fn run_source(&mut self, source: &str) -> Result<()> {
        let tokens = crate::lexer::lex(source)?;
        let program = crate::parser::parse(tokens)?;
        self.run(&program)
    }

    pub fn run(&mut self, program: &[Stmt]) -> Result<()> {
        self.steps = 0;
        let env = self.globals.clone();
        // `return` at the top level just ends the script.
        self.exec_block(program, &env)?;
        Ok(())
    }

    /// Read a script-level variable (used by tests and, later, the engine).
    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.get(name)
    }

    // --- statements ---

    fn exec_block(&mut self, stmts: &[Stmt], env: &Arc<Env>) -> Result<Flow> {
        for stmt in stmts {
            match self.exec(stmt, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn tick(&mut self, line: usize) -> Result<()> {
        self.steps += 1;
        if self.steps > self.step_limit {
            return Err(RovikError::new(
                line,
                "this script ran for too long without pausing. Is there a loop that never ends? Add wait() inside long loops",
            ));
        }
        Ok(())
    }

    fn exec(&mut self, stmt: &Stmt, env: &Arc<Env>) -> Result<Flow> {
        self.tick(stmt.line)?;
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                self.eval(expr, env)?;
            }

            StmtKind::Assign(target, value) => {
                let value = self.eval(value, env)?;
                self.assign(target, value, env)?;
            }

            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (cond, body) in branches {
                    if self.eval(cond, env)?.truthy() {
                        return self.exec_block(body, env);
                    }
                }
                if let Some(body) = otherwise {
                    return self.exec_block(body, env);
                }
            }

            StmtKind::While(cond, body) => {
                while self.eval(cond, env)?.truthy() {
                    self.tick(stmt.line)?;
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
            }

            StmtKind::ForIn(var, iterable, body) => {
                let items = match self.eval(iterable, env)? {
                    // Loop over a copy, so changing the list inside the loop is safe.
                    Value::List(list) => list.read().unwrap().clone(),
                    Value::Map(map) => map.read().unwrap().keys().map(|k| Value::str(k.as_str())).collect(),
                    Value::Str(s) => s.chars().map(|c| Value::str(c.to_string())).collect(),
                    other => {
                        return Err(RovikError::new(
                            stmt.line,
                            format!(
                                "a 'for' loop can go through a list, a map or some text, but this is a {}. For counting, use for i in 1..10 do",
                                other.type_name()
                            ),
                        ))
                    }
                };
                for item in items {
                    self.tick(stmt.line)?;
                    env.assign(var, item);
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
            }

            StmtKind::ForRange(var, from, to, body) => {
                let from = self.expect_number(from, env, "the start of a range")?;
                let to = self.expect_number(to, env, "the end of a range")?;
                let mut i = from;
                while i <= to {
                    self.tick(stmt.line)?;
                    env.assign(var, Value::Num(i));
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                    i += 1.0;
                }
            }

            StmtKind::Fn(def) => {
                let name = def.name.clone().unwrap_or_default();
                let closure = Value::Func(Arc::new(Closure {
                    def: def.clone(),
                    env: env.clone(),
                }));
                env.assign(&name, closure);
            }

            StmtKind::On { event, handler } => {
                self.handlers.lock().unwrap().push(Handler {
                    trigger: Trigger::Event(event.clone()),
                    function: Value::Func(Arc::new(Closure {
                        def: handler.clone(),
                        env: env.clone(),
                    })),
                });
            }

            StmtKind::Every { interval, handler } => {
                let seconds = self.expect_number(interval, env, "the time for 'every'")?;
                if seconds <= 0.0 {
                    return Err(RovikError::new(
                        stmt.line,
                        "'every' needs a time greater than 0 seconds",
                    ));
                }
                self.handlers.lock().unwrap().push(Handler {
                    trigger: Trigger::Every(seconds),
                    function: Value::Func(Arc::new(Closure {
                        def: handler.clone(),
                        env: env.clone(),
                    })),
                });
            }

            StmtKind::Return(value) => {
                let v = match value {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(v));
            }

            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
        }
        Ok(Flow::Normal)
    }

    fn assign(&mut self, target: &Expr, value: Value, env: &Arc<Env>) -> Result<()> {
        match &target.kind {
            ExprKind::Var(name) => {
                env.assign(name, value);
                Ok(())
            }

            ExprKind::Index(obj, index) => {
                let obj = self.eval(obj, env)?;
                let index = self.eval(index, env)?;
                match obj {
                    Value::List(list) => {
                        let len = list.read().unwrap().len();
                        let i = list_index(&index, len, target.line)?;
                        list.write().unwrap()[i] = value;
                        Ok(())
                    }
                    Value::Map(map) => {
                        let key = map_key(&index, target.line)?;
                        map.write().unwrap().insert(key, value);
                        Ok(())
                    }
                    other => Err(RovikError::new(
                        target.line,
                        format!("can't use [ ] to change a {}", other.type_name()),
                    )),
                }
            }

            ExprKind::Field(obj, name) => match self.eval(obj, env)? {
                Value::Map(map) => {
                    map.write().unwrap().insert(name.clone(), value);
                    Ok(())
                }
                Value::Object(o) => {
                    let host = self.require_host(target.line)?;
                    host.set_field(o, name, value)
                        .map_err(|m| RovikError::new(target.line, m))
                }
                other => Err(RovikError::new(
                    target.line,
                    format!("a {} doesn't have a '{name}' to set", other.type_name()),
                )),
            },

            _ => Err(RovikError::new(target.line, "can't assign to this")),
        }
    }

    // --- expressions ---

    fn eval(&mut self, expr: &Expr, env: &Arc<Env>) -> Result<Value> {
        let line = expr.line;
        match &expr.kind {
            ExprKind::Number(n) => Ok(Value::Num(*n)),
            ExprKind::Str(s) => Ok(Value::str(s.as_str())),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Nil => Ok(Value::Nil),

            ExprKind::Var(name) => self.lookup(name, env, line),

            ExprKind::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, env)?);
                }
                Ok(Value::list(values))
            }

            ExprKind::Map(entries) => {
                let mut map = BTreeMap::new();
                for (key, value) in entries {
                    map.insert(key.clone(), self.eval(value, env)?);
                }
                Ok(Value::map(map))
            }

            ExprKind::Unary(op, inner) => {
                let v = self.eval(inner, env)?;
                match op {
                    UnOp::Not => Ok(Value::Bool(!v.truthy())),
                    UnOp::Neg => match v {
                        Value::Num(n) => Ok(Value::Num(-n)),
                        other => Err(RovikError::new(
                            line,
                            format!("can't make a {} negative", other.type_name()),
                        )),
                    },
                }
            }

            // `and`/`or` stop early, and give back one of their values.
            ExprKind::And(a, b) => {
                let left = self.eval(a, env)?;
                if !left.truthy() {
                    return Ok(left);
                }
                self.eval(b, env)
            }
            ExprKind::Or(a, b) => {
                let left = self.eval(a, env)?;
                if left.truthy() {
                    return Ok(left);
                }
                self.eval(b, env)
            }

            ExprKind::Binary(op, a, b) => {
                let left = self.eval(a, env)?;
                let right = self.eval(b, env)?;
                self.binary(*op, left, right, line)
            }

            ExprKind::Call(callee, args) => {
                let function = self.eval(callee, env)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, env)?);
                }
                self.call(function, values, line)
            }

            ExprKind::Index(obj, index) => {
                let obj = self.eval(obj, env)?;
                let index = self.eval(index, env)?;
                match obj {
                    Value::List(list) => {
                        let list = list.read().unwrap();
                        let i = list_index(&index, list.len(), line)?;
                        Ok(list[i].clone())
                    }
                    Value::Map(map) => {
                        let key = map_key(&index, line)?;
                        Ok(map.read().unwrap().get(&key).cloned().unwrap_or(Value::Nil))
                    }
                    Value::Str(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        let i = list_index(&index, chars.len(), line)?;
                        Ok(Value::str(chars[i].to_string()))
                    }
                    other => Err(RovikError::new(
                        line,
                        format!("can't use [ ] on a {}", other.type_name()),
                    )),
                }
            }

            ExprKind::Field(obj, name) => match self.eval(obj, env)? {
                Value::Object(o) => self.host_get(o, name, line),
                // A missing field is nil, so `if player.shield then` works.
                Value::Map(map) => Ok(map.read().unwrap().get(name).cloned().unwrap_or(Value::Nil)),
                Value::Nil => Err(RovikError::new(
                    line,
                    format!("can't read '{name}' from nil. The value before the '.' doesn't exist"),
                )),
                other => Err(RovikError::new(
                    line,
                    format!("a {} doesn't have a '{name}'", other.type_name()),
                )),
            },

            ExprKind::Lambda(def) => Ok(Value::Func(Arc::new(Closure {
                def: def.clone(),
                env: env.clone(),
            }))),
        }
    }

    fn lookup(&self, name: &str, env: &Arc<Env>, line: usize) -> Result<Value> {
        if let Some(v) = env.get(name) {
            return Ok(v);
        }
        if let Some(b) = Builtin::lookup(name) {
            return Ok(Value::Builtin(b));
        }
        if let Some(host) = &self.host {
            if host.function_names().contains(&name) {
                return Ok(Value::HostFn(name.into()));
            }
        }

        // Reading a name that was never given a value is almost always a typo.
        let mut candidates = env.visible_names();
        candidates.extend(Builtin::ALL.iter().map(|(n, _)| n.to_string()));
        if let Some(host) = &self.host {
            candidates.extend(host.function_names().iter().map(|n| n.to_string()));
        }
        let suggestion = candidates
            .iter()
            .filter(|c| c.as_str() != name)
            .map(|c| (edit_distance(name, c), c))
            .filter(|(d, _)| *d <= 2)
            .min_by_key(|(d, _)| *d)
            .map(|(_, c)| format!(" Did you mean '{c}'?"))
            .unwrap_or_default();

        Err(RovikError::new(
            line,
            format!("'{name}' hasn't been given a value yet.{suggestion}"),
        ))
    }

    fn expect_number(&mut self, expr: &Expr, env: &Arc<Env>, what: &str) -> Result<f64> {
        match self.eval(expr, env)? {
            Value::Num(n) => Ok(n),
            other => Err(RovikError::new(
                expr.line,
                format!("{what} needs to be a number, but this is a {}", other.type_name()),
            )),
        }
    }

    // --- calls ---

    pub fn call(&mut self, function: Value, args: Vec<Value>, line: usize) -> Result<Value> {
        match function {
            Value::Func(closure) => {
                let def = &closure.def;
                let label = def.name.clone().unwrap_or_else(|| "this function".to_string());
                if args.len() != def.params.len() {
                    return Err(RovikError::new(
                        line,
                        format!(
                            "{label} needs {} but got {}",
                            count(def.params.len(), "value"),
                            args.len()
                        ),
                    ));
                }
                if self.depth >= CALL_DEPTH_LIMIT {
                    return Err(RovikError::new(
                        line,
                        "too many functions calling each other. Is a function calling itself forever?",
                    ));
                }

                let scope = Env::new_child(closure.env.clone());
                for (param, arg) in def.params.iter().zip(args) {
                    scope.define(param, arg);
                }

                self.depth += 1;
                let result = self.exec_block(&def.body, &scope);
                self.depth -= 1;

                match result? {
                    Flow::Return(v) => Ok(v),
                    _ => Ok(Value::Nil),
                }
            }
            Value::Builtin(b) => self.call_builtin(b, args, line),
            Value::HostFn(name) => {
                let host = self.require_host(line)?;
                host.call(&name, &args).map_err(|m| RovikError::new(line, m))
            }
            other => Err(RovikError::new(
                line,
                format!("a {} can't be called like a function", other.type_name()),
            )),
        }
    }

    fn call_builtin(&mut self, b: Builtin, args: Vec<Value>, line: usize) -> Result<Value> {
        let name = b.name();
        let need = |n: usize| -> Result<()> {
            if args.len() == n {
                Ok(())
            } else {
                Err(RovikError::new(
                    line,
                    format!("{name} needs {} but got {}", count(n, "value"), args.len()),
                ))
            }
        };
        let num = |v: &Value, what: &str| -> Result<f64> {
            match v {
                Value::Num(n) => Ok(*n),
                other => Err(RovikError::new(
                    line,
                    format!("{name} needs {what} to be a number, but got a {}", other.type_name()),
                )),
            }
        };

        match b {
            Builtin::Print => {
                let text: Vec<String> = args.iter().map(|v| self.display(v)).collect();
                let text = text.join(" ");
                match &self.on_print {
                    Some(hook) => hook(&text),
                    None => {
                        if self.echo {
                            println!("{text}");
                        }
                        self.output.push(text);
                    }
                }
                Ok(Value::Nil)
            }

            Builtin::Len => {
                need(1)?;
                match &args[0] {
                    Value::Str(s) => Ok(Value::Num(s.chars().count() as f64)),
                    Value::List(l) => Ok(Value::Num(l.read().unwrap().len() as f64)),
                    Value::Map(m) => Ok(Value::Num(m.read().unwrap().len() as f64)),
                    other => Err(RovikError::new(
                        line,
                        format!("len works on text, lists and maps, not a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Str => {
                need(1)?;
                Ok(Value::str(self.display(&args[0])))
            }

            Builtin::Num => {
                need(1)?;
                match &args[0] {
                    Value::Num(n) => Ok(Value::Num(*n)),
                    Value::Str(s) => Ok(s.trim().parse::<f64>().map(Value::Num).unwrap_or(Value::Nil)),
                    _ => Ok(Value::Nil),
                }
            }

            Builtin::Type => {
                need(1)?;
                Ok(Value::str(self.type_of(&args[0])))
            }

            Builtin::Push => {
                need(2)?;
                match &args[0] {
                    Value::List(l) => {
                        l.write().unwrap().push(args[1].clone());
                        Ok(Value::Nil)
                    }
                    other => Err(RovikError::new(
                        line,
                        format!("push needs a list first, but got a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Pop => {
                need(1)?;
                match &args[0] {
                    Value::List(l) => Ok(l.write().unwrap().pop().unwrap_or(Value::Nil)),
                    other => Err(RovikError::new(
                        line,
                        format!("pop needs a list, but got a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Insert => {
                need(3)?;
                match &args[0] {
                    Value::List(l) => {
                        let len = l.read().unwrap().len();
                        // Inserting at len+1 is allowed: it adds to the end.
                        let i = list_index(&args[1], len + 1, line)?;
                        l.write().unwrap().insert(i, args[2].clone());
                        Ok(Value::Nil)
                    }
                    other => Err(RovikError::new(
                        line,
                        format!("insert needs a list first, but got a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Remove => {
                need(2)?;
                match &args[0] {
                    Value::List(l) => {
                        let len = l.read().unwrap().len();
                        let i = list_index(&args[1], len, line)?;
                        Ok(l.write().unwrap().remove(i))
                    }
                    Value::Map(m) => {
                        let key = map_key(&args[1], line)?;
                        Ok(m.write().unwrap().remove(&key).unwrap_or(Value::Nil))
                    }
                    other => Err(RovikError::new(
                        line,
                        format!("remove needs a list or map, but got a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Keys => {
                need(1)?;
                match &args[0] {
                    Value::Map(m) => Ok(Value::list(
                        m.read().unwrap().keys().map(|k| Value::str(k.as_str())).collect(),
                    )),
                    other => Err(RovikError::new(
                        line,
                        format!("keys needs a map, but got a {}", other.type_name()),
                    )),
                }
            }

            Builtin::Wait => {
                need(1)?;
                let seconds = num(&args[0], "the time")?;
                if seconds < 0.0 {
                    return Err(RovikError::new(line, "wait can't take a negative time"));
                }
                match &self.on_wait {
                    // Inside a game: pause just this task.
                    Some(hook) => hook(seconds).map_err(|m| RovikError::new(line, m))?,
                    // On its own (the command line): pause the program.
                    None => std::thread::sleep(Duration::from_secs_f64(seconds)),
                }
                self.steps = 0;
                Ok(Value::Nil)
            }

            Builtin::Floor => {
                need(1)?;
                Ok(Value::Num(num(&args[0], "its value")?.floor()))
            }
            Builtin::Round => {
                need(1)?;
                Ok(Value::Num(num(&args[0], "its value")?.round()))
            }
            Builtin::Abs => {
                need(1)?;
                Ok(Value::Num(num(&args[0], "its value")?.abs()))
            }
            Builtin::Sqrt => {
                need(1)?;
                let n = num(&args[0], "its value")?;
                if n < 0.0 {
                    return Err(RovikError::new(line, "can't take the square root of a negative number"));
                }
                Ok(Value::Num(n.sqrt()))
            }

            Builtin::Min | Builtin::Max => {
                if args.is_empty() {
                    return Err(RovikError::new(line, format!("{name} needs at least one number")));
                }
                let mut best = num(&args[0], "every value")?;
                for v in &args[1..] {
                    let n = num(v, "every value")?;
                    best = if b == Builtin::Min { best.min(n) } else { best.max(n) };
                }
                Ok(Value::Num(best))
            }

            Builtin::Random => match args.len() {
                // random() gives a decimal from 0 up to (not including) 1.
                0 => Ok(Value::Num(self.next_random())),
                // random(a, b) gives a whole number from a to b, including both.
                2 => {
                    let lo = num(&args[0], "the lowest value")?.ceil();
                    let hi = num(&args[1], "the highest value")?.floor();
                    if hi < lo {
                        return Err(RovikError::new(line, "random's first number must not be bigger than its second"));
                    }
                    let span = hi - lo + 1.0;
                    Ok(Value::Num(lo + (self.next_random() * span).floor()))
                }
                n => Err(RovikError::new(
                    line,
                    format!("random needs either no values or 2 values, but got {n}"),
                )),
            },
        }
    }

    fn require_host(&self, line: usize) -> Result<Arc<dyn Host>> {
        self.host.clone().ok_or_else(|| {
            RovikError::new(line, "objects only work when the script runs inside Brixo")
        })
    }

    fn host_get(&self, obj: ObjectRef, name: &str, line: usize) -> Result<Value> {
        let host = self.require_host(line)?;
        host.get_field(obj, name).map_err(|m| RovikError::new(line, m))
    }

    fn type_of(&self, value: &Value) -> String {
        match (value, &self.host) {
            (Value::Object(o), Some(host)) => host.type_name(*o),
            _ => value.type_name().to_string(),
        }
    }

    /// xorshift64*: small and good enough for games.
    fn next_random(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (r >> 11) as f64 / (1u64 << 53) as f64
    }
}

// --- operators ---

impl Interpreter {
fn binary(&self, op: BinOp, left: Value, right: Value, line: usize) -> Result<Value> {
    use Value::{Num, Str};
    match op {
        BinOp::Eq => return Ok(Value::Bool(values_equal(&left, &right))),
        BinOp::NotEq => return Ok(Value::Bool(!values_equal(&left, &right))),
        _ => {}
    }

    match (op, &left, &right) {
        (BinOp::Add, Num(a), Num(b)) => Ok(Num(a + b)),
        // If either side is text, + joins them as text.
        (BinOp::Add, Str(_), _) | (BinOp::Add, _, Str(_)) => {
            Ok(Value::str(format!("{}{}", self.display(&left), self.display(&right))))
        }
        (BinOp::Add, Value::List(a), Value::List(b)) => {
            let mut items = a.read().unwrap().clone();
            let more = b.read().unwrap().clone();
            items.extend(more);
            Ok(Value::list(items))
        }
        (BinOp::Sub, Num(a), Num(b)) => Ok(Num(a - b)),
        (BinOp::Mul, Num(a), Num(b)) => Ok(Num(a * b)),
        (BinOp::Div, Num(_), Num(b)) if *b == 0.0 => Err(RovikError::new(line, "can't divide by zero")),
        (BinOp::Div, Num(a), Num(b)) => Ok(Num(a / b)),
        (BinOp::Mod, Num(_), Num(b)) if *b == 0.0 => Err(RovikError::new(line, "can't use % with zero")),
        (BinOp::Mod, Num(a), Num(b)) => Ok(Num(a.rem_euclid(*b))),

        (BinOp::Lt, Num(a), Num(b)) => Ok(Value::Bool(a < b)),
        (BinOp::LtEq, Num(a), Num(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Gt, Num(a), Num(b)) => Ok(Value::Bool(a > b)),
        (BinOp::GtEq, Num(a), Num(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::Lt, Str(a), Str(b)) => Ok(Value::Bool(a < b)),
        (BinOp::LtEq, Str(a), Str(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Gt, Str(a), Str(b)) => Ok(Value::Bool(a > b)),
        (BinOp::GtEq, Str(a), Str(b)) => Ok(Value::Bool(a >= b)),

        _ => {
            let symbol = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::Lt => "<",
                BinOp::LtEq => "<=",
                BinOp::Gt => ">",
                BinOp::GtEq => ">=",
                BinOp::Eq | BinOp::NotEq => unreachable!(),
            };
            let mut message = format!(
                "can't use {symbol} with a {} and a {}",
                self.type_of(&left),
                self.type_of(&right)
            );
            if matches!(left, Value::Nil) || matches!(right, Value::Nil) {
                message.push_str(". One side is nil, so something may not have been set");
            }
            Err(RovikError::new(line, message))
        }
    }
}
}

/// Turns a 1-based Rovik index into a 0-based position, with a clear error.
fn list_index(index: &Value, len: usize, line: usize) -> Result<usize> {
    let n = match index {
        Value::Num(n) => *n,
        other => {
            return Err(RovikError::new(
                line,
                format!("a list position has to be a number, not a {}", other.type_name()),
            ))
        }
    };
    if n.fract() != 0.0 {
        return Err(RovikError::new(
            line,
            format!("a list position has to be a whole number, not {}", format_number(n)),
        ));
    }
    if n < 1.0 || n > len as f64 {
        let hint = if n == 0.0 {
            " Positions start at 1"
        } else {
            ""
        };
        return Err(RovikError::new(
            line,
            format!(
                "position {} is outside the list, which has {}.{hint}",
                format_number(n),
                count(len, "item")
            ),
        ));
    }
    Ok(n as usize - 1)
}

fn map_key(key: &Value, line: usize) -> Result<String> {
    match key {
        Value::Str(s) => Ok(s.to_string()),
        Value::Num(n) => Ok(format_number(*n)),
        other => Err(RovikError::new(
            line,
            format!("a map key has to be text or a number, not a {}", other.type_name()),
        )),
    }
}

fn count(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// How many single-letter edits turn `a` into `b`.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut curr = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = curr;
    }
    prev[b.len()]
}
