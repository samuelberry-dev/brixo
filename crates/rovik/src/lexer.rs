//! Turns source text into tokens.

use crate::error::{Result, RovikError};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Number(f64),
    Str(String),
    Ident(String),

    // keywords
    Fn,
    End,
    If,
    Then,
    Elseif,
    Else,
    While,
    Do,
    For,
    In,
    Return,
    Break,
    Continue,
    And,
    Or,
    Not,
    True,
    False,
    Nil,
    On,
    Every,

    // symbols
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Dot,
    DotDot,

    /// Ends a statement.
    Newline,
    Eof,
}

impl Tok {
    /// How the token reads in an error message.
    pub fn describe(&self) -> String {
        match self {
            Tok::Number(n) => format!("the number {n}"),
            Tok::Str(_) => "some text".to_string(),
            Tok::Ident(name) => format!("'{name}'"),
            Tok::Newline => "the end of the line".to_string(),
            Tok::Eof => "the end of the script".to_string(),
            other => format!("'{}'", other.symbol()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Tok::Fn => "fn",
            Tok::End => "end",
            Tok::If => "if",
            Tok::Then => "then",
            Tok::Elseif => "elseif",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::Do => "do",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Return => "return",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::And => "and",
            Tok::Or => "or",
            Tok::Not => "not",
            Tok::True => "true",
            Tok::False => "false",
            Tok::Nil => "nil",
            Tok::On => "on",
            Tok::Every => "every",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::Eq => "=",
            Tok::EqEq => "==",
            Tok::NotEq => "!=",
            Tok::Lt => "<",
            Tok::LtEq => "<=",
            Tok::Gt => ">",
            Tok::GtEq => ">=",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::Comma => ",",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::Number(_) | Tok::Str(_) | Tok::Ident(_) | Tok::Newline | Tok::Eof => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "fn" => Tok::Fn,
        "end" => Tok::End,
        "if" => Tok::If,
        "then" => Tok::Then,
        "elseif" => Tok::Elseif,
        "else" => Tok::Else,
        "while" => Tok::While,
        "do" => Tok::Do,
        "for" => Tok::For,
        "in" => Tok::In,
        "return" => Tok::Return,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "and" => Tok::And,
        "or" => Tok::Or,
        "not" => Tok::Not,
        "true" => Tok::True,
        "false" => Tok::False,
        "nil" => Tok::Nil,
        "on" => Tok::On,
        "every" => Tok::Every,
        _ => return None,
    })
}

pub fn lex(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let mut line = 1;
    // Inside (), [] or {} newlines don't end statements, so lists and
    // calls can span several lines.
    let mut depth: usize = 0;

    let push = |tokens: &mut Vec<Token>, tok: Tok, line: usize| tokens.push(Token { tok, line });

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();

        match c {
            ' ' | '\t' | '\r' => i += 1,

            '\n' => {
                let last_is_newline = matches!(tokens.last(), Some(Token { tok: Tok::Newline, .. }));
                if depth == 0 && !tokens.is_empty() && !last_is_newline {
                    push(&mut tokens, Tok::Newline, line);
                }
                line += 1;
                i += 1;
            }

            // -- line comment
            '-' if next == Some('-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }

            // *** block comment ***
            '*' if next == Some('*') && chars.get(i + 2) == Some(&'*') => {
                let start_line = line;
                i += 3;
                loop {
                    // Not enough characters left for a closing ***.
                    if i + 2 >= chars.len() {
                        return Err(RovikError::new(
                            start_line,
                            "this *** comment is never closed. End it with another ***",
                        ));
                    }
                    if chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
                        i += 3;
                        break;
                    }
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }

            '0'..='9' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                // A '.' only belongs to the number when a digit follows,
                // so that 1..10 reads as 1, .., 10.
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let value: f64 = text
                    .parse()
                    .map_err(|_| RovikError::new(line, format!("'{text}' isn't a valid number")))?;
                push(&mut tokens, Tok::Number(value), line);
            }

            '"' | '\'' => {
                let quote = c;
                let start_line = line;
                i += 1;
                let mut text = String::new();
                loop {
                    let Some(&ch) = chars.get(i) else {
                        return Err(RovikError::new(
                            start_line,
                            format!("this text is missing its closing {quote}"),
                        ));
                    };
                    if ch == '\n' {
                        return Err(RovikError::new(
                            start_line,
                            format!("this text is missing its closing {quote} before the line ends"),
                        ));
                    }
                    i += 1;
                    if ch == quote {
                        break;
                    }
                    if ch == '\\' {
                        let escaped = chars.get(i).copied();
                        i += 1;
                        match escaped {
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            Some('\\') => text.push('\\'),
                            Some('"') => text.push('"'),
                            Some('\'') => text.push('\''),
                            Some(other) => {
                                return Err(RovikError::new(
                                    line,
                                    format!("'\\{other}' isn't a known escape. Try \\n, \\t, \\\\ or \\\""),
                                ))
                            }
                            None => {
                                return Err(RovikError::new(
                                    start_line,
                                    format!("this text is missing its closing {quote}"),
                                ))
                            }
                        }
                    } else {
                        text.push(ch);
                    }
                }
                push(&mut tokens, Tok::Str(text), line);
            }

            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let tok = keyword(&word).unwrap_or(Tok::Ident(word));
                push(&mut tokens, tok, line);
            }

            _ => {
                let (tok, width) = match (c, next) {
                    ('+', Some('=')) => (Tok::PlusEq, 2),
                    ('-', Some('=')) => (Tok::MinusEq, 2),
                    ('*', Some('=')) => (Tok::StarEq, 2),
                    ('/', Some('=')) => (Tok::SlashEq, 2),
                    ('=', Some('=')) => (Tok::EqEq, 2),
                    ('!', Some('=')) => (Tok::NotEq, 2),
                    ('<', Some('=')) => (Tok::LtEq, 2),
                    ('>', Some('=')) => (Tok::GtEq, 2),
                    ('.', Some('.')) => (Tok::DotDot, 2),
                    ('+', _) => (Tok::Plus, 1),
                    ('-', _) => (Tok::Minus, 1),
                    ('*', _) => (Tok::Star, 1),
                    ('/', _) => (Tok::Slash, 1),
                    ('%', _) => (Tok::Percent, 1),
                    ('=', _) => (Tok::Eq, 1),
                    ('<', _) => (Tok::Lt, 1),
                    ('>', _) => (Tok::Gt, 1),
                    ('(', _) => (Tok::LParen, 1),
                    (')', _) => (Tok::RParen, 1),
                    ('[', _) => (Tok::LBracket, 1),
                    (']', _) => (Tok::RBracket, 1),
                    ('{', _) => (Tok::LBrace, 1),
                    ('}', _) => (Tok::RBrace, 1),
                    (',', _) => (Tok::Comma, 1),
                    ('.', _) => (Tok::Dot, 1),
                    ('!', _) => {
                        return Err(RovikError::new(
                            line,
                            "Rovik uses 'not' instead of '!'. For 'not equal', write !=",
                        ))
                    }
                    _ => {
                        return Err(RovikError::new(
                            line,
                            format!("'{c}' isn't something Rovik understands"),
                        ))
                    }
                };
                match tok {
                    Tok::LParen | Tok::LBracket | Tok::LBrace => depth += 1,
                    Tok::RParen | Tok::RBracket | Tok::RBrace => depth = depth.saturating_sub(1),
                    _ => {}
                }
                push(&mut tokens, tok, line);
                i += width;
            }
        }
    }

    let last_is_newline = matches!(tokens.last(), Some(Token { tok: Tok::Newline, .. }));
    if !tokens.is_empty() && !last_is_newline {
        push(&mut tokens, Tok::Newline, line);
    }
    push(&mut tokens, Tok::Eof, line);
    Ok(tokens)
}
