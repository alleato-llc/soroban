//! `EngineError` — every failure the engine surfaces to a host. Lex/parse
//! errors carry character offsets into the source line so UIs can render a
//! caret under the offending column; preserve positions when changing the
//! lexer or parser.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    LexError {
        message: String,
        position: usize,
    },
    ParseError {
        message: String,
        position: usize,
    },
    DivisionByZero,
    UnknownVariable {
        name: String,
    },
    UnknownFunction {
        name: String,
    },
    ArityMismatch {
        function: String,
        expected: String,
        got: usize,
    },
    DomainError {
        message: String,
    },
}

impl EngineError {
    pub fn domain(message: impl Into<String>) -> Self {
        Self::DomainError {
            message: message.into(),
        }
    }

    /// Column to point a caret at, when the error is positional.
    pub fn position(&self) -> Option<usize> {
        match self {
            Self::LexError { position, .. } | Self::ParseError { position, .. } => Some(*position),
            _ => None,
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LexError { message, position } => {
                write!(f, "syntax error at column {}: {message}", position + 1)
            }
            Self::ParseError { message, position } => {
                write!(f, "parse error at column {}: {message}", position + 1)
            }
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::UnknownVariable { name } => write!(f, "unknown variable '{name}'"),
            Self::UnknownFunction { name } => write!(f, "unknown function '{name}'"),
            Self::ArityMismatch {
                function,
                expected,
                got,
            } => {
                let plural = if expected == "1" { "" } else { "s" };
                write!(
                    f,
                    "{function}() expects {expected} argument{plural}, got {got}"
                )
            }
            Self::DomainError { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for EngineError {}
