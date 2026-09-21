//! Turns tokens into a syntax tree.

use std::rc::Rc;

use crate::ast::*;
use crate::error::{Result, RovikError};
use crate::lexer::{Tok, Token};

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        loop_depth: 0,
    };
    parser.program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Tracks whether `break`/`continue` are inside a loop.
    loop_depth: usize,
}

impl Parser {
    // --- token helpers ---

    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn check(&self, tok: &Tok) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(tok)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<Token> {
        if self.check(&tok) {
            Ok(self.advance())
        } else {
            Err(self.error_here(format!("expected {what}, but found {}", self.peek().describe())))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error_here(format!(
                "expected {what}, but found {}",
                other.describe()
            ))),
        }
    }

    fn error_here(&self, message: impl Into<String>) -> RovikError {
        RovikError::new(self.line(), message)
    }

    fn skip_newlines(&mut self) {
        while self.eat(&Tok::Newline) {}
    }

    // --- structure ---

    fn program(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !self.check(&Tok::Eof) {
            if self.check(&Tok::End) {
                return Err(self.error_here(
                    "this 'end' doesn't close anything. Is there an extra one?",
                ));
            }
            if self.check(&Tok::Else) || self.check(&Tok::Elseif) {
                return Err(self.error_here(format!(
                    "{} needs to come after an 'if'",
                    self.peek().describe()
                )));
            }
            stmts.push(self.statement()?);
            self.end_of_statement()?;
            self.skip_newlines();
        }
        Ok(stmts)
    }

    /// After a statement we need a new line, or a keyword that closes the
    /// block (so one-liners like `if x then print(x) end` work).
    fn end_of_statement(&mut self) -> Result<()> {
        match self.peek() {
            Tok::Newline => {
                self.advance();
                Ok(())
            }
            Tok::End | Tok::Else | Tok::Elseif | Tok::Eof => Ok(()),
            other => Err(self.error_here(format!(
                "expected a new line here, but found {}. Put each statement on its own line",
                other.describe()
            ))),
        }
    }

    /// Parses statements until one of `stop` appears. `opener` names the
    /// block for the error message if its `end` is missing.
    fn block(&mut self, opener: &str, open_line: usize, stop: &[Tok]) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if stop.iter().any(|t| self.check(t)) {
                return Ok(stmts);
            }
            if self.check(&Tok::Eof) {
                return Err(RovikError::new(
                    open_line,
                    format!("this '{opener}' is missing its 'end'"),
                ));
            }
            if self.check(&Tok::Else) || self.check(&Tok::Elseif) {
                return Err(self.error_here(format!(
                    "{} can only be used inside an 'if'",
                    self.peek().describe()
                )));
            }
            stmts.push(self.statement()?);
            self.end_of_statement()?;
        }
    }

    fn statement(&mut self) -> Result<Stmt> {
        let line = self.line();
        let kind = match self.peek() {
            Tok::Fn if matches!(self.tokens.get(self.pos + 1).map(|t| &t.tok), Some(Tok::Ident(_))) => {
                self.advance();
                let name = self.expect_ident("a name for the function")?;
                let def = self.function_rest(Some(name), line)?;
                StmtKind::Fn(def)
            }
            Tok::If => self.if_statement()?,
            Tok::While => {
                self.advance();
                let cond = self.expression()?;
                self.expect_do("while")?;
                self.loop_depth += 1;
                let body = self.block("while", line, &[Tok::End]);
                self.loop_depth -= 1;
                let body = body?;
                self.expect(Tok::End, "'end'")?;
                StmtKind::While(cond, body)
            }
            Tok::For => self.for_statement()?,
            Tok::On => {
                self.advance();
                let event = self.expect_ident("an event name, like 'touched'")?;
                let params = if self.check(&Tok::LParen) {
                    self.params()?
                } else {
                    Vec::new()
                };
                let body = self.function_body("on", line)?;
                StmtKind::On {
                    event: event.clone(),
                    handler: Rc::new(FnDef {
                        name: Some(format!("on {event}")),
                        params,
                        body,
                        line,
                    }),
                }
            }
            Tok::Every => {
                self.advance();
                let interval = self.expression()?;
                match self.peek().clone() {
                    Tok::Ident(unit) if unit == "seconds" || unit == "second" => {
                        self.advance();
                    }
                    other => {
                        return Err(self.error_here(format!(
                            "expected 'seconds' after the number, but found {}",
                            other.describe()
                        )))
                    }
                }
                let body = self.function_body("every", line)?;
                StmtKind::Every {
                    interval,
                    handler: Rc::new(FnDef {
                        name: Some("every".to_string()),
                        params: Vec::new(),
                        body,
                        line,
                    }),
                }
            }
            Tok::Return => {
                self.advance();
                let value = match self.peek() {
                    Tok::Newline | Tok::End | Tok::Else | Tok::Elseif | Tok::Eof => None,
                    _ => Some(self.expression()?),
                };
                StmtKind::Return(value)
            }
            Tok::Break => {
                if self.loop_depth == 0 {
                    return Err(self.error_here("'break' can only be used inside a loop"));
                }
                self.advance();
                StmtKind::Break
            }
            Tok::Continue => {
                if self.loop_depth == 0 {
                    return Err(self.error_here("'continue' can only be used inside a loop"));
                }
                self.advance();
                StmtKind::Continue
            }
            _ => self.expression_statement()?,
        };
        Ok(Stmt { kind, line })
    }

    fn if_statement(&mut self) -> Result<StmtKind> {
        let if_line = self.line();
        self.advance(); // if
        let mut branches = Vec::new();

        let cond = self.expression()?;
        self.expect_then()?;
        let body = self.block("if", if_line, &[Tok::Elseif, Tok::Else, Tok::End])?;
        branches.push((cond, body));

        let mut otherwise = None;
        loop {
            if self.eat(&Tok::Elseif) {
                let cond = self.expression()?;
                self.expect_then()?;
                let body = self.block("if", if_line, &[Tok::Elseif, Tok::Else, Tok::End])?;
                branches.push((cond, body));
            } else if self.eat(&Tok::Else) {
                otherwise = Some(self.block("if", if_line, &[Tok::End])?);
                if self.check(&Tok::Elseif) || self.check(&Tok::Else) {
                    return Err(self.error_here("'else' has to be the last part of an 'if'"));
                }
            } else {
                break;
            }
        }
        self.expect(Tok::End, "'end'")?;
        Ok(StmtKind::If {
            branches,
            otherwise,
        })
    }

    fn for_statement(&mut self) -> Result<StmtKind> {
        let line = self.line();
        self.advance(); // for
        let var = self.expect_ident("a variable name after 'for'")?;
        self.expect(Tok::In, "'in'")?;
        let first = self.expression()?;
        let range_end = if self.eat(&Tok::DotDot) {
            Some(self.expression()?)
        } else {
            None
        };
        self.expect_do("for")?;
        self.loop_depth += 1;
        let body = self.block("for", line, &[Tok::End]);
        self.loop_depth -= 1;
        let body = body?;
        self.expect(Tok::End, "'end'")?;

        Ok(match range_end {
            Some(end) => StmtKind::ForRange(var, first, end, body),
            None => StmtKind::ForIn(var, first, body),
        })
    }

    fn expect_then(&mut self) -> Result<()> {
        if self.check(&Tok::Eq) {
            return Err(self.error_here(
                "'=' gives a variable a value. To compare two things, use '=='",
            ));
        }
        self.expect(Tok::Then, "'then'")?;
        Ok(())
    }

    fn expect_do(&mut self, keyword: &str) -> Result<()> {
        if self.check(&Tok::Then) {
            return Err(self.error_here(format!("a '{keyword}' loop uses 'do', not 'then'")));
        }
        self.expect(Tok::Do, "'do'")?;
        Ok(())
    }

    fn params(&mut self) -> Result<Vec<String>> {
        self.expect(Tok::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                let name = self.expect_ident("a parameter name")?;
                if params.contains(&name) {
                    return Err(self.error_here(format!("'{name}' is listed twice")));
                }
                params.push(name);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "')'")?;
        Ok(params)
    }

    /// Function bodies reset the loop counter: `break` inside a function
    /// can't reach a loop outside it.
    fn function_body(&mut self, opener: &str, line: usize) -> Result<Vec<Stmt>> {
        let saved = self.loop_depth;
        self.loop_depth = 0;
        let body = self.block(opener, line, &[Tok::End]);
        self.loop_depth = saved;
        let body = body?;
        self.expect(Tok::End, "'end'")?;
        Ok(body)
    }

    fn function_rest(&mut self, name: Option<String>, line: usize) -> Result<Rc<FnDef>> {
        let params = self.params()?;
        let body = self.function_body("fn", line)?;
        Ok(Rc::new(FnDef {
            name,
            params,
            body,
            line,
        }))
    }

    fn expression_statement(&mut self) -> Result<StmtKind> {
        let target = self.expression()?;

        let compound = match self.peek() {
            Tok::PlusEq => Some(BinOp::Add),
            Tok::MinusEq => Some(BinOp::Sub),
            Tok::StarEq => Some(BinOp::Mul),
            Tok::SlashEq => Some(BinOp::Div),
            _ => None,
        };

        if self.check(&Tok::Eq) || compound.is_some() {
            if !matches!(
                target.kind,
                ExprKind::Var(_) | ExprKind::Index(..) | ExprKind::Field(..)
            ) {
                return Err(self.error_here(
                    "the left side of '=' has to be a variable, like score = 10",
                ));
            }
            let line = self.line();
            self.advance();
            let value = self.expression()?;
            let value = match compound {
                Some(op) => Expr {
                    kind: ExprKind::Binary(op, Box::new(target.clone()), Box::new(value)),
                    line,
                },
                None => value,
            };
            return Ok(StmtKind::Assign(target, value));
        }

        // A bare value on its own line does nothing, which is almost always a mistake.
        if !matches!(target.kind, ExprKind::Call(..)) {
            if let ExprKind::Binary(BinOp::Eq, ..) = target.kind {
                return Err(self.error_here(
                    "this compares two values but doesn't use the answer. To set a variable, use a single '='",
                ));
            }
            return Err(self.error_here(
                "this line works out a value but doesn't do anything with it. Did you mean to store it, like x = ...?",
            ));
        }
        Ok(StmtKind::Expr(target))
    }

    // --- expressions, lowest precedence first ---

    fn expression(&mut self) -> Result<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr> {
        let mut left = self.and_expr()?;
        while self.check(&Tok::Or) {
            let line = self.advance().line;
            let right = self.and_expr()?;
            left = Expr {
                kind: ExprKind::Or(Box::new(left), Box::new(right)),
                line,
            };
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut left = self.not_expr()?;
        while self.check(&Tok::And) {
            let line = self.advance().line;
            let right = self.not_expr()?;
            left = Expr {
                kind: ExprKind::And(Box::new(left), Box::new(right)),
                line,
            };
        }
        Ok(left)
    }

    /// `not` sits below comparisons, so `not x == 5` means `not (x == 5)`.
    fn not_expr(&mut self) -> Result<Expr> {
        if self.check(&Tok::Not) {
            let line = self.advance().line;
            let inner = self.not_expr()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Not, Box::new(inner)),
                line,
            });
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr> {
        let mut left = self.additive()?;
        loop {
            let op = match self.peek() {
                Tok::EqEq => BinOp::Eq,
                Tok::NotEq => BinOp::NotEq,
                Tok::Lt => BinOp::Lt,
                Tok::LtEq => BinOp::LtEq,
                Tok::Gt => BinOp::Gt,
                Tok::GtEq => BinOp::GtEq,
                _ => break,
            };
            let line = self.advance().line;
            let right = self.additive()?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                line,
            };
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            let line = self.advance().line;
            let right = self.multiplicative()?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                line,
            };
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            let line = self.advance().line;
            let right = self.unary()?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                line,
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr> {
        if self.check(&Tok::Minus) {
            let line = self.advance().line;
            let inner = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Neg, Box::new(inner)),
                line,
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut expr = self.primary()?;
        loop {
            let line = self.line();
            if self.eat(&Tok::LParen) {
                let mut args = Vec::new();
                if !self.check(&Tok::RParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                        if self.check(&Tok::RParen) {
                            break; // trailing comma
                        }
                    }
                }
                self.expect(Tok::RParen, "')' to finish the call")?;
                expr = Expr {
                    kind: ExprKind::Call(Box::new(expr), args),
                    line,
                };
            } else if self.eat(&Tok::LBracket) {
                let index = self.expression()?;
                self.expect(Tok::RBracket, "']'")?;
                expr = Expr {
                    kind: ExprKind::Index(Box::new(expr), Box::new(index)),
                    line,
                };
            } else if self.eat(&Tok::Dot) {
                let name = self.expect_ident("a name after '.'")?;
                expr = Expr {
                    kind: ExprKind::Field(Box::new(expr), name),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr> {
        let line = self.line();
        let tok = self.peek().clone();
        let kind = match tok {
            Tok::Number(n) => {
                self.advance();
                ExprKind::Number(n)
            }
            Tok::Str(s) => {
                self.advance();
                ExprKind::Str(s)
            }
            Tok::True => {
                self.advance();
                ExprKind::Bool(true)
            }
            Tok::False => {
                self.advance();
                ExprKind::Bool(false)
            }
            Tok::Nil => {
                self.advance();
                ExprKind::Nil
            }
            Tok::Ident(name) => {
                self.advance();
                ExprKind::Var(name)
            }
            Tok::LParen => {
                self.advance();
                let inner = self.expression()?;
                self.expect(Tok::RParen, "')'")?;
                return Ok(inner);
            }
            Tok::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !self.check(&Tok::RBracket) {
                    items.push(self.expression()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBracket, "']' to finish the list")?;
                ExprKind::List(items)
            }
            Tok::LBrace => {
                self.advance();
                let mut entries = Vec::new();
                while !self.check(&Tok::RBrace) {
                    let key = match self.peek().clone() {
                        Tok::Ident(k) | Tok::Str(k) => {
                            self.advance();
                            k
                        }
                        other => {
                            return Err(self.error_here(format!(
                                "expected a name for this entry, like {{name = \"Sam\"}}, but found {}",
                                other.describe()
                            )))
                        }
                    };
                    self.expect(Tok::Eq, "'=' after the entry name")?;
                    let value = self.expression()?;
                    entries.push((key, value));
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBrace, "'}' to finish the map")?;
                ExprKind::Map(entries)
            }
            Tok::Fn => {
                self.advance();
                ExprKind::Lambda(self.function_rest(None, line)?)
            }
            other => {
                return Err(self.error_here(format!(
                    "expected a value here, but found {}",
                    other.describe()
                )))
            }
        };
        Ok(Expr { kind, line })
    }
}
