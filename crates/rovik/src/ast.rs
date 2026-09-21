//! The syntax tree the parser builds and the interpreter walks.

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Number(f64),
    Str(String),
    Bool(bool),
    Nil,
    Var(String),
    List(Vec<Expr>),
    Map(Vec<(String, Expr)>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    /// `fn (a, b) ... end` used as a value.
    Lambda(Arc<FnDef>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug)]
pub struct FnDef {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// A call on its own line, like `print("hi")`.
    Expr(Expr),
    /// `target = value`. Compound forms like `+=` are rewritten into this.
    Assign(Expr, Expr),
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    While(Expr, Vec<Stmt>),
    ForIn(String, Expr, Vec<Stmt>),
    ForRange(String, Expr, Expr, Vec<Stmt>),
    Fn(Arc<FnDef>),
    /// `on touched(player) ... end`
    On {
        event: String,
        handler: Arc<FnDef>,
    },
    /// `every 5 seconds ... end`
    Every {
        interval: Expr,
        handler: Arc<FnDef>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
}
