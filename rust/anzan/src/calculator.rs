//! The `Calculator` facade — the engine's front door, mirroring the Swift
//! `Calculator`: `evaluate` is the log path (updates `ans`); formula
//! evaluation for cells arrives in the engine crate's phase.

use crate::{EngineError, LanguageMode};
use std::fmt;

/// What one log line produced (stub — grows `.value`/`.functionDefined`/
/// `.dataDefined`/`.documentation`/`.comment`/`.rawBlock` as the port lands).
#[derive(Debug, Clone, PartialEq)]
pub enum EvalOutcome {
    Value(String),
}

impl EvalOutcome {
    /// The numeric value of a `.value` outcome, if it is one (used by the
    /// tolerance steps in the gherkin harness).
    pub fn numeric_value(&self) -> Option<f64> {
        None
    }
}

impl fmt::Display for EvalOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(text) => write!(f, "{text}"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Calculator {
    pub mode: LanguageMode,
}

impl Calculator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn evaluate(&mut self, _line: &str) -> Result<EvalOutcome, EngineError> {
        Err(EngineError::domain("not yet ported: Calculator::evaluate"))
    }
}
