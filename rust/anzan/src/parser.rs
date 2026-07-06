//! Pratt (precedence-climbing) parser.
//!
//! Grammar, low to high precedence:
//!   assignment:  IDENT = expr          (only at top level)
//!   additive:    + -
//!   term:        * / %  and implicit multiplication (`2(3+4)`, `2x`, `(a)(b)`)
//!   unary:       -x
//!   power:       ^ (right-associative, binds tighter than unary: -2^2 == -4)
//!   primary:     number | ident | ident(args) | (expr)
//!
//! The productions are split across sibling submodules: `definitions`
//! (statements/declarations/lambdas), `expressions` (the operator ladder),
//! `primary` (literals + special forms), and `references` (qualified names,
//! cell references, argument lists).

use crate::ast::Expression;
use crate::lexer::{Lexer, Token, TokenKind};
use crate::{EngineError, LanguageMode};

mod definitions;
mod expressions;
mod primary;
mod references;

/// Identifiers that cannot be assigned to (or defined as functions).
/// `sigma` is the special summation form, so a user function would be
/// uncallable anyway.
pub(crate) const RESERVED_NAMES: [&str; 15] = [
    "ans", "pi", "e", "tau", "π", "τ", "sigma", "if", "man", "manual", "help", "true", "false",
    "json", "rounding",
];

pub(crate) fn is_reserved(name: &str) -> bool {
    RESERVED_NAMES.contains(&name.to_lowercase().as_str())
}

pub struct Parser {
    tokens: Vec<Token>,
    /// The dialect to parse under — affects only overloaded glyphs
    /// (`^ % & | << >>`). `Normal` (the default) is today's grammar exactly.
    /// See `docs/MODES.md`.
    mode: LanguageMode,
    index: usize,
}

fn parse_error(message: impl Into<String>, position: usize) -> EngineError {
    EngineError::ParseError {
        message: message.into(),
        position,
    }
}

impl Parser {
    pub fn parse(source: &str, mode: LanguageMode) -> Result<Expression, EngineError> {
        let mut parser = Parser {
            tokens: Lexer::tokenize(source)?,
            mode,
            index: 0,
        };
        let expr = parser.statement()?;
        parser.expect_end()?;
        Ok(expr)
    }

    /// `sigma_x`/`product_x` spellings are reserved for the indexed
    /// reduction forms.
    pub(crate) fn is_reduction_name(name: &str) -> bool {
        let lowered = name.to_lowercase();
        lowered.starts_with("sigma_") || lowered.starts_with("product_")
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn kind_at(&self, i: usize) -> Option<&TokenKind> {
        self.tokens.get(i).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if self.index < self.tokens.len() - 1 {
            self.index += 1;
        }
        token
    }

    fn expect_end(&self) -> Result<(), EngineError> {
        if self.current().kind == TokenKind::End {
            Ok(())
        } else {
            Err(parse_error(
                "unexpected trailing input",
                self.current().position(),
            ))
        }
    }
}

#[cfg(test)]
mod tests;
