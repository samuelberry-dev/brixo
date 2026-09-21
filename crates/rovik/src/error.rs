use std::fmt;

/// Every Rovik error points at a line, because that's the first thing a
/// beginner needs to find the mistake.
#[derive(Debug, Clone, PartialEq)]
pub struct RovikError {
    pub line: usize,
    pub message: String,
}

impl RovikError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for RovikError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for RovikError {}

pub type Result<T> = std::result::Result<T, RovikError>;
