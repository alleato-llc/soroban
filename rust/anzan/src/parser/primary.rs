//! Primary expressions: literals, identifiers/calls, the `man`/`if`/∑/∏
//! special forms, array and map literals, and the indexed-reduction bounds.

use super::{is_reserved, parse_error, Parser};
use crate::ast::{Expression, MapLiteralEntry, ReductionOperation};
use crate::lexer::TokenKind;
use crate::{BigDecimal, EngineError};

impl Parser {
    pub(super) fn primary(&mut self) -> Result<Expression, EngineError> {
        let token = self.advance();
        let token_position = token.position();
        match token.kind {
            TokenKind::Number(value) => Ok(Expression::Number(value)),

            TokenKind::Money { value, currency } => Ok(Expression::Money { value, currency }),

            TokenKind::Grouped(value) => Ok(Expression::Grouped(value)),

            TokenKind::CellReference { column, row, .. } => {
                self.cell_reference_or_range(None, column, row)
            }

            TokenKind::QuotedName(quoted) => {
                // 'Q1 Budget'!A:1 — a sheet qualifier when ! follows;
                // otherwise 'Projected Rate' — a NAMED CELL on the owning
                // sheet.
                if self.current().kind == TokenKind::Bang {
                    return self.qualified_reference(quoted, token_position);
                }
                Ok(Expression::NameReference {
                    sheet: None,
                    name: quoted,
                })
            }

            TokenKind::Identifier(name) if self.current().kind == TokenKind::Bang => {
                // Budget!A:1 — unquoted sheet qualifier.
                self.qualified_reference(name, token_position)
            }

            TokenKind::Identifier(name) if self.current().kind == TokenKind::ColonColon => {
                // Bits::BitFormat — namespace-qualified reference /
                // constructor call.
                self.qualified_name(name, token_position)
            }

            TokenKind::Identifier(name) => {
                // ∑/∏: a plain call is the variadic function (sum/product);
                // the subscript form is the indexed reduction. Typed
                // `sigma_i` arrives as one identifier; after the ∑/∏ symbol,
                // `_i` arrives separately.
                let lowered = name.to_lowercase();
                for (keyword, operation) in [
                    ("sigma", ReductionOperation::Sum),
                    ("product", ReductionOperation::Product),
                ] {
                    if lowered == keyword {
                        if let TokenKind::Identifier(subscript) = &self.current().kind {
                            if let Some(index_name) = subscript.strip_prefix('_') {
                                let index_name = index_name.to_string();
                                self.advance();
                                return self.math_reduction(operation, index_name, token_position);
                            }
                        }
                    } else if lowered.starts_with(&format!("{keyword}_")) {
                        let index_name = name[keyword.len() + 1..].to_string();
                        return self.math_reduction(operation, index_name, token_position);
                    }
                }

                // man NAME / manual NAME / help NAME: unix-style — the
                // argument is a NAME (never evaluated), space-separated, NO
                // parentheses.
                if lowered == "man" || lowered == "manual" || lowered == "help" {
                    if self.current().kind == TokenKind::LeftParen {
                        return Err(parse_error(
                            format!("use `{name} name` — e.g. {name} pmt (no parentheses)"),
                            self.current().position(),
                        ));
                    }
                    let TokenKind::Identifier(subject) = &self.current().kind else {
                        return Err(parse_error(
                            format!("{name} needs a function name — e.g. {name} pmt"),
                            self.current().position(),
                        ));
                    };
                    let subject = subject.clone();
                    self.advance();
                    return Ok(Expression::HelpRequest { name: subject });
                }

                if self.current().kind != TokenKind::LeftParen {
                    return Ok(Expression::Variable(name));
                }
                self.advance();
                let arguments = self.argument_list()?;
                if lowered == "sigma" {
                    // ∑(1,2,3) = 6
                    return Ok(Expression::Call {
                        name: "sum".to_string(),
                        arguments,
                    });
                }
                if lowered == "if" {
                    // Special form: branches stay lazy (the untaken one may
                    // divide by zero or recurse).
                    if arguments.len() != 3 {
                        return Err(parse_error(
                            "if expects (condition, then, else)",
                            token_position,
                        ));
                    }
                    let mut args = arguments.into_iter();
                    return Ok(Expression::Conditional {
                        condition: Box::new(args.next().expect("checked len")),
                        then: Box::new(args.next().expect("checked len")),
                        otherwise: Box::new(args.next().expect("checked len")),
                    });
                }
                // ∏(…) hits product().
                Ok(Expression::Call { name, arguments })
            }

            TokenKind::LeftParen => {
                let inner = self.comparison()?;
                if self.current().kind != TokenKind::RightParen {
                    return Err(parse_error("expected ')'", self.current().position()));
                }
                self.advance();
                Ok(inner)
            }

            TokenKind::String(text) => Ok(Expression::StringLiteral(text)),

            TokenKind::LeftBracket => self.array_literal(),

            TokenKind::LeftBrace => self.map_literal(token_position),

            TokenKind::End => Err(parse_error("unexpected end of expression", token_position)),

            _ => Err(parse_error("unexpected token", token_position)),
        }
    }

    /// `[1, 2, 3]` after the consumed '[' — elements are full expressions.
    fn array_literal(&mut self) -> Result<Expression, EngineError> {
        if self.current().kind == TokenKind::RightBracket {
            self.advance();
            return Ok(Expression::ArrayLiteral(Vec::new()));
        }
        let mut items = vec![self.comparison()?];
        while self.current().kind == TokenKind::Comma {
            self.advance();
            items.push(self.comparison()?);
        }
        if self.current().kind != TokenKind::RightBracket {
            return Err(parse_error(
                "expected ']' or ','",
                self.current().position(),
            ));
        }
        self.advance();
        Ok(Expression::ArrayLiteral(items))
    }

    /// `{name: "Ada", age: 36}` after the consumed '{'. Keys are identifiers
    /// or string literals. One lexing wrinkle: a compact single-letter key
    /// with a number value (`{b:1}`) arrives as a cell-reference TOKEN — in
    /// key position it decomposes back into key + number value.
    fn map_literal(&mut self, position: usize) -> Result<Expression, EngineError> {
        let mut entries: Vec<MapLiteralEntry> = Vec::new();

        fn append(
            entries: &mut Vec<MapLiteralEntry>,
            key: String,
            value: Expression,
            position: usize,
        ) -> Result<(), EngineError> {
            if entries.iter().any(|e| e.key == key) {
                return Err(parse_error(format!("duplicate key '{key}'"), position));
            }
            entries.push(MapLiteralEntry { key, value });
            Ok(())
        }

        if self.current().kind == TokenKind::RightBrace {
            self.advance();
            return Ok(Expression::MapLiteral(entries));
        }
        loop {
            match &self.current().kind {
                TokenKind::CellReference { column, row, .. } => {
                    // {b:1} — the lexer saw a cell reference; here it's a key
                    // and its numeric value.
                    let (key, row) = (column.clone(), *row);
                    self.advance();
                    append(
                        &mut entries,
                        key,
                        Expression::Number(BigDecimal::from_int(row)),
                        position,
                    )?;
                }

                TokenKind::Identifier(key) | TokenKind::String(key) => {
                    let key = key.clone();
                    self.advance();
                    if self.current().kind != TokenKind::Colon {
                        return Err(parse_error(
                            format!("expected ':' after key '{key}'"),
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    let value = self.comparison()?;
                    append(&mut entries, key, value, position)?;
                }

                _ => {
                    return Err(parse_error(
                        "expected a key — e.g. {name: \"Ada\", age: 36}",
                        self.current().position(),
                    ));
                }
            }

            match self.current().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightBrace => break,
                _ => {
                    return Err(parse_error(
                        "expected '}' or ','",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // '}'
        Ok(Expression::MapLiteral(entries))
    }

    /// Indexed reduction in math notation: `∑_i=1^10(i^2)` / `∏_i=1^5(i)`
    /// (typeable as `sigma_i=…` / `product_i=…`). Special forms — the
    /// parenthesized term is NOT evaluated eagerly; it re-evaluates per index
    /// value.
    ///
    /// Bounds are signed primaries (number, variable, cell ref, or
    /// parenthesized expression): that's what keeps the `^` separator
    /// unambiguous with exponentiation. Compound bounds need parentheses —
    /// the plaintext equivalent of LaTeX braces.
    fn math_reduction(
        &mut self,
        operation: ReductionOperation,
        index_name: String,
        position: usize,
    ) -> Result<Expression, EngineError> {
        let symbol = operation.symbol();
        if index_name.is_empty() || !index_name.chars().all(char::is_alphabetic) {
            return Err(parse_error(
                format!(
                    "the {symbol} index must be a plain variable name (e.g. {symbol}_i=1^10(i))"
                ),
                position,
            ));
        }
        if is_reserved(&index_name) {
            return Err(parse_error(
                format!("cannot use '{index_name}' as the {symbol} index"),
                position,
            ));
        }

        if self.current().kind != TokenKind::Assign {
            return Err(parse_error(
                format!("expected '=' after the {symbol} index — e.g. {symbol}_i=1^10(i)"),
                self.current().position(),
            ));
        }
        self.advance();
        let lower = self.signed_primary()?;

        if self.current().kind != TokenKind::Caret {
            return Err(parse_error(
                format!(
                    "expected '^' before the {symbol} upper bound — parenthesize compound bounds, e.g. {symbol}_i=(n-1)^10(i)"
                ),
                self.current().position(),
            ));
        }
        self.advance();
        let upper = self.signed_primary()?;

        if self.current().kind != TokenKind::LeftParen {
            return Err(parse_error(
                format!("the {symbol} term must be parenthesized — e.g. {symbol}_i=1^10(i)"),
                self.current().position(),
            ));
        }
        self.advance();
        let body = self.comparison()?;
        if self.current().kind != TokenKind::RightParen {
            return Err(parse_error("expected ')'", self.current().position()));
        }
        self.advance();

        Ok(Expression::Reduction {
            operation,
            index: index_name,
            lower: Box::new(lower),
            upper: Box::new(upper),
            body: Box::new(body),
        })
    }

    /// A ∑ bound: optional minus + bound primary. Deliberately not `unary()`
    /// (which falls into `power()` and would consume the `^` bound separator)
    /// and not `primary()` (which would treat `n(` as a call, swallowing the
    /// term's parentheses in `∑_i=1^n(i)`).
    fn signed_primary(&mut self) -> Result<Expression, EngineError> {
        if self.current().kind == TokenKind::Minus {
            self.advance();
            return Ok(Expression::UnaryMinus(Box::new(self.bound_primary()?)));
        }
        self.bound_primary()
    }

    fn bound_primary(&mut self) -> Result<Expression, EngineError> {
        let token = self.advance();
        let token_position = token.position();
        match token.kind {
            TokenKind::Number(value) => Ok(Expression::Number(value)),

            TokenKind::Money { value, currency } => Ok(Expression::Money { value, currency }),
            TokenKind::Grouped(value) => Ok(Expression::Grouped(value)),
            TokenKind::CellReference { column, row, .. } => Ok(Expression::CellReference {
                sheet: None,
                column,
                row,
            }),
            // Never a call — a following '(' is the term.
            TokenKind::Identifier(name) => Ok(Expression::Variable(name)),
            TokenKind::LeftParen => {
                let inner = self.comparison()?;
                if self.current().kind != TokenKind::RightParen {
                    return Err(parse_error("expected ')'", self.current().position()));
                }
                self.advance();
                Ok(inner)
            }
            _ => Err(parse_error(
                "expected a ∑ bound (number, variable, or parenthesized expression)",
                token_position,
            )),
        }
    }
}
