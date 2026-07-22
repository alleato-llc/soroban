//! Splits multi-line SOURCE into logical statements — the engine primitive
//! behind `.anzan` script files, statement-aware pipes, and (later) multi-line
//! paste in the apps. A statement ends at a newline UNLESS a `(` `[` `{` is
//! still open; continuation lines JOIN INTO ONE LOGICAL LINE, so everything
//! downstream — the parser, caret columns, error echoes, doc comments —
//! behaves exactly as if the statement had been typed on one line (newlines
//! are already insignificant to the lexer; this just decides where statements
//! END). Port of swift/Engine/Sources/Anzan/Script.swift.
//!
//! Streaming by design: `push` one physical line at a time (a pipe, a REPL, a
//! file), collect completed statements, then `finish()` — which is where an
//! unclosed block becomes a loud error instead of a silent swallow.

use crate::lexer::Lexer;
use crate::EngineError;

/// A completed logical statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The joined one-line form (first-line trailing comment re-attached).
    pub text: String,
    /// 1-based physical line where the statement began — for `file:line`
    /// error reporting and continuation prompts.
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct StatementAccumulator {
    parts: Vec<String>,
    depth: i64,
    start_line: usize,
    current_line: usize,
    first_comment: Option<String>,
}

impl StatementAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while a statement is open (brackets unbalanced) — REPLs show a
    /// continuation prompt; `finish()` would error.
    pub fn is_pending(&self) -> bool {
        !self.parts.is_empty()
    }

    /// The pending statement's text so far — for the unterminated-error echo.
    pub fn pending_text(&self) -> String {
        self.parts.join(" ")
    }

    /// Feed the next physical line. Returns a completed statement, or None
    /// while blank or continuing. Comment-only lines (including a `#!`
    /// shebang) are statements of their own — the engine treats them as notes.
    pub fn push(&mut self, line: &str) -> Option<Statement> {
        self.current_line += 1;
        let (raw_code, comment) = Lexer::split_comment(line);
        let code = raw_code.trim().to_string();

        if self.parts.is_empty() {
            if code.is_empty() {
                comment.as_ref()?; // blank line
                                   // Comment-only: a standalone note, passed through as written.
                return Some(Statement {
                    text: line.trim().to_string(),
                    line: self.current_line,
                });
            }
            self.start_line = self.current_line;
            self.first_comment = comment;
        } else if code.is_empty() {
            return None; // blank or comment-only line INSIDE a continuation
        }

        self.depth = (self.depth + Self::bracket_delta(&code)).max(0);
        self.parts.push(code);
        if self.depth > 0 {
            return None;
        }

        let joined = self.parts.join(" ");
        let text = match self.first_comment.take() {
            Some(comment) => format!("{joined}  # {comment}"),
            None => joined,
        };
        let statement = Statement {
            text,
            line: self.start_line,
        };
        self.reset();
        Some(statement)
    }

    /// End of input. A no-op when nothing is pending; an unclosed block is a
    /// parse error naming the line that opened it (statements complete in
    /// `push` the moment depth returns to zero, so a pending statement at EOF
    /// is ALWAYS unterminated).
    pub fn finish(&mut self) -> Result<(), EngineError> {
        if !self.is_pending() {
            return Ok(());
        }
        let opened = self.start_line;
        self.reset();
        Err(EngineError::ParseError {
            message: format!(
                "unterminated statement — the block opened at line {opened} is missing a closing bracket"
            ),
            position: 0,
        })
    }

    fn reset(&mut self) {
        self.parts.clear();
        self.depth = 0;
        self.first_comment = None;
    }

    /// Net bracket depth of a comment-stripped line, ignoring bracket
    /// characters inside string literals (the same string walk
    /// `Lexer::split_comment` uses).
    fn bracket_delta(code: &str) -> i64 {
        let mut delta = 0;
        let mut in_string = false;
        let mut chars = code.chars();
        while let Some(c) = chars.next() {
            if in_string {
                match c {
                    '\\' => {
                        let _ = chars.next(); // skip the escaped character
                    }
                    '"' => in_string = false,
                    _ => {}
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '(' | '[' | '{' => delta += 1,
                    ')' | ']' | '}' => delta -= 1,
                    _ => {}
                }
            }
        }
        delta
    }

    /// Convenience for whole-source callers (the SDK one-liner): split
    /// `source` into its logical statements. Errors on an unclosed block.
    pub fn statements(source: &str) -> Result<Vec<Statement>, EngineError> {
        let mut accumulator = Self::new();
        let mut result = Vec::new();
        for line in source.split('\n') {
            if let Some(statement) = accumulator.push(line) {
                result.push(statement);
            }
        }
        accumulator.finish()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
