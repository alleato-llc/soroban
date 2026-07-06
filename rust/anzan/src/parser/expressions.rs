//! The operator-precedence expression ladder: comparison → the Programmer-
//! mode bitwise band → additive → term (with implicit multiplication) →
//! unary → power → postfix accessors.

use super::{parse_error, Parser};
use crate::ast::{ComparisonOperator, Expression};
use crate::lexer::TokenKind;
use crate::{EngineError, LanguageMode};

impl Parser {
    /// One comparison level: `additive (op additive)?`. Single comparison
    /// only — `a < b < c` is rejected (1/0 chaining is never what you meant).
    /// Lambdas are checked first: every expression entry point comes through
    /// here, so `x -> …` works as an argument, an assignment value, a body…
    pub(super) fn comparison(&mut self) -> Result<Expression, EngineError> {
        if let Some(lambda) = self.lambda_expression()? {
            return Ok(lambda);
        }
        let lhs = self.bitwise_or()?;
        let Some(op) = comparison_operator(&self.current().kind) else {
            return Ok(lhs);
        };
        self.advance();
        let rhs = self.bitwise_or()?;
        if comparison_operator(&self.current().kind).is_some() {
            return Err(parse_error(
                "comparisons can't be chained — use and(a < b, b < c)",
                self.current().position(),
            ));
        }
        Ok(Expression::Comparison(op, Box::new(lhs), Box::new(rhs)))
    }

    // MARK: Programmer-mode bitwise band (Python precedence)
    //
    // Loosest-to-tightest: `|` · `^` · `&` · `<< >>`, sitting between
    // comparison and additive (so bitwise binds below arithmetic, above
    // comparison — no C-style `a & b == c` trap). Active only in
    // `Programmer`; in other modes these are pass-throughs, and the glyphs
    // that have no other meaning (`| & << >>`) raise a mode-scoped error
    // rather than a vague "unexpected input". `^` is NOT errored here: in
    // `Normal`/`Finance` it is power, consumed deeper by power().

    fn bitwise_or(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.bitwise_xor()?;
        while self.current().kind == TokenKind::Pipe {
            if self.mode != LanguageMode::Programmer {
                return Err(self.mode_operator_error("|", "bitOr", self.current().position()));
            }
            self.advance();
            lhs = Expression::Call {
                name: "bitOr".to_string(),
                arguments: vec![lhs, self.bitwise_xor()?],
            };
        }
        Ok(lhs)
    }

    fn bitwise_xor(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.bitwise_and()?;
        while self.mode == LanguageMode::Programmer && self.current().kind == TokenKind::Caret {
            self.advance();
            lhs = Expression::Call {
                name: "bitXor".to_string(),
                arguments: vec![lhs, self.bitwise_and()?],
            };
        }
        Ok(lhs)
    }

    fn bitwise_and(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.shift()?;
        while self.current().kind == TokenKind::Ampersand {
            if self.mode != LanguageMode::Programmer {
                return Err(self.mode_operator_error("&", "bitAnd", self.current().position()));
            }
            self.advance();
            lhs = Expression::Call {
                name: "bitAnd".to_string(),
                arguments: vec![lhs, self.shift()?],
            };
        }
        Ok(lhs)
    }

    fn shift(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.additive()?;
        loop {
            match self.current().kind {
                TokenKind::ShiftLeft => {
                    if self.mode != LanguageMode::Programmer {
                        return Err(self.mode_operator_error(
                            "<<",
                            "bitShift",
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    lhs = Expression::Call {
                        name: "bitShift".to_string(),
                        arguments: vec![lhs, self.additive()?],
                    };
                }
                TokenKind::ShiftRight => {
                    if self.mode != LanguageMode::Programmer {
                        return Err(self.mode_operator_error(
                            ">>",
                            "bitShift",
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    // `a >> n` ≡ bitShift(a, -n) — bitShift shifts right on a
                    // negative count.
                    lhs = Expression::Call {
                        name: "bitShift".to_string(),
                        arguments: vec![lhs, Expression::UnaryMinus(Box::new(self.additive()?))],
                    };
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn mode_operator_error(&self, glyph: &str, function: &str, position: usize) -> EngineError {
        parse_error(
            format!(
                "'{glyph}' is a Programmer-mode operator — use {function}(…), or switch to Programmer mode"
            ),
            position,
        )
    }

    fn additive(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.term()?;
        loop {
            match self.current().kind {
                TokenKind::Plus => {
                    self.advance();
                    lhs = Expression::Binary(
                        crate::ast::BinaryOperator::Add,
                        Box::new(lhs),
                        Box::new(self.term()?),
                    );
                }
                TokenKind::Minus => {
                    self.advance();
                    lhs = Expression::Binary(
                        crate::ast::BinaryOperator::Subtract,
                        Box::new(lhs),
                        Box::new(self.term()?),
                    );
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn term(&mut self) -> Result<Expression, EngineError> {
        use crate::ast::BinaryOperator::{Divide, Multiply};
        let mut lhs = self.unary()?;
        loop {
            match &self.current().kind {
                TokenKind::Star => {
                    self.advance();
                    lhs = Expression::Binary(Multiply, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Slash => {
                    self.advance();
                    lhs = Expression::Binary(Divide, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Percent if self.mode == LanguageMode::Programmer => {
                    // Programmer mode: `%` is modulo (mod(a, b)), at
                    // multiplicative precedence. In other modes `%` is
                    // postfix percent (postfix()).
                    self.advance();
                    lhs = Expression::Call {
                        name: "mod".to_string(),
                        arguments: vec![lhs, self.unary()?],
                    };
                }
                TokenKind::LeftParen
                | TokenKind::Identifier(_)
                | TokenKind::CellReference { .. } => {
                    // Implicit multiplication: `2(3+4)`, `2x`, `(a)(b)`,
                    // `2 A:1` — a value against a name, paren, or cell.
                    lhs = Expression::Binary(Multiply, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Number(_) => {
                    // A number directly following another value (`3 4`,
                    // `3 % 4`) is almost always a missing operator, not
                    // implicit ×. Error toward it instead of silently
                    // multiplying.
                    return Err(parse_error(
                        "a number can't directly follow another value — add an operator (e.g. 3 * 4)",
                        self.current().position(),
                    ));
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn unary(&mut self) -> Result<Expression, EngineError> {
        match self.current().kind {
            TokenKind::Minus => {
                self.advance();
                Ok(Expression::UnaryMinus(Box::new(self.unary()?)))
            }
            TokenKind::Plus => {
                // Unary plus is a no-op.
                self.advance();
                self.unary()
            }
            TokenKind::SqrtSign => {
                // √x desugars to sqrt(x).
                self.advance();
                Ok(Expression::Call {
                    name: "sqrt".to_string(),
                    arguments: vec![self.unary()?],
                })
            }
            TokenKind::Tilde => {
                // ~x is bitwise NOT (Programmer mode only).
                if self.mode != LanguageMode::Programmer {
                    return Err(self.mode_operator_error("~", "bitNot", self.current().position()));
                }
                self.advance();
                Ok(Expression::Call {
                    name: "bitNot".to_string(),
                    arguments: vec![self.unary()?],
                })
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expression, EngineError> {
        let base = self.postfix()?;
        // In Programmer mode `^` is XOR (consumed up the chain by
        // bitwise_xor); only in Normal/Finance is it power.
        if self.mode == LanguageMode::Programmer || self.current().kind != TokenKind::Caret {
            return Ok(base);
        }
        self.advance();
        // Right-associative; the exponent may carry its own unary minus
        // (2^-1).
        Ok(Expression::Binary(
            crate::ast::BinaryOperator::Power,
            Box::new(base),
            Box::new(self.unary()?),
        ))
    }

    /// Postfix accessors, binding tighter than `^`: `arr[0]`, `m.name`,
    /// chained freely (`people[0].age`, `grid[1][2]`).
    fn postfix(&mut self) -> Result<Expression, EngineError> {
        let mut expr = self.primary()?;
        loop {
            match self.current().kind {
                TokenKind::LeftBracket => {
                    self.advance();
                    let index_expr = self.comparison()?;
                    if self.current().kind != TokenKind::RightBracket {
                        return Err(parse_error("expected ']'", self.current().position()));
                    }
                    self.advance();
                    expr = Expression::Index {
                        base: Box::new(expr),
                        index: Box::new(index_expr),
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    let TokenKind::Identifier(name) = &self.current().kind else {
                        return Err(parse_error(
                            "expected a key name after '.' — e.g. person.age",
                            self.current().position(),
                        ));
                    };
                    let name = name.clone();
                    self.advance();
                    if self.current().kind == TokenKind::LeftParen {
                        // Method call: base.name(args) — hosts dispatch these.
                        self.advance();
                        let arguments = self.argument_list()?;
                        expr = Expression::MethodCall {
                            base: Box::new(expr),
                            name,
                            arguments,
                        };
                    } else {
                        expr = Expression::Member {
                            base: Box::new(expr),
                            name,
                        };
                    }
                }
                TokenKind::Percent if self.mode != LanguageMode::Programmer => {
                    // Postfix percent: `3%` → 0.03. In Normal/Finance `%` is
                    // always percent; chains like other postfixes (`A:1%`,
                    // `arr[0]%`). In Programmer mode `%` is modulo — left for
                    // term() to consume.
                    self.advance();
                    expr = Expression::Percent(Box::new(expr));
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}

fn comparison_operator(kind: &TokenKind) -> Option<ComparisonOperator> {
    match kind {
        TokenKind::LessThan => Some(ComparisonOperator::Less),
        TokenKind::GreaterThan => Some(ComparisonOperator::Greater),
        TokenKind::LessOrEqual => Some(ComparisonOperator::LessOrEqual),
        TokenKind::GreaterOrEqual => Some(ComparisonOperator::GreaterOrEqual),
        TokenKind::EqualEqual => Some(ComparisonOperator::Equal),
        TokenKind::NotEqual => Some(ComparisonOperator::NotEqual),
        _ => None,
    }
}
