//! Reference and call-tail productions: namespace-qualified names, sheet-
//! qualified cell/named-cell references, cell ranges, and argument lists
//! (positional and the named-argument sugar that desugars to a map).

use super::{parse_error, Parser};
use crate::ast::{Expression, MapLiteralEntry};
use crate::lexer::TokenKind;
use crate::{BigDecimal, EngineError};

impl Parser {
    /// `Bits::BitFormat`, `A::B::c` — a namespace-qualified reference
    /// (nesting chains `::`); with `(` it's a qualified call (the constructor
    /// of a namespaced type). The whole qualified name flows as one string
    /// ("A::B::c") that the evaluator resolves.
    pub(super) fn qualified_name(
        &mut self,
        namespace: String,
        _position: usize,
    ) -> Result<Expression, EngineError> {
        let mut qualified = namespace.clone();
        loop {
            self.advance(); // '::'
            let TokenKind::Identifier(member) = &self.current().kind else {
                return Err(parse_error(
                    format!("expected a name after '::' — e.g. {namespace}::Point"),
                    self.current().position(),
                ));
            };
            let member = member.clone();
            self.advance();
            qualified.push_str("::");
            qualified.push_str(&member);
            if self.current().kind != TokenKind::ColonColon {
                break;
            }
        }
        if self.current().kind != TokenKind::LeftParen {
            return Ok(Expression::Variable(qualified));
        }
        self.advance();
        Ok(Expression::Call {
            name: qualified,
            arguments: self.argument_list()?,
        })
    }

    /// After a sheet name: `!` then a cell reference, range, or 'named cell'.
    pub(super) fn qualified_reference(
        &mut self,
        sheet: String,
        _position: usize,
    ) -> Result<Expression, EngineError> {
        if self.current().kind != TokenKind::Bang {
            return Err(parse_error(
                format!("expected '!' after sheet name '{sheet}' — e.g. '{sheet}'!A:1"),
                self.current().position(),
            ));
        }
        self.advance();
        match &self.current().kind {
            TokenKind::CellReference { column, row, .. } => {
                let (column, row) = (column.clone(), *row);
                self.advance();
                self.cell_reference_or_range(Some(sheet), column, row)
            }
            TokenKind::QuotedName(name) => {
                // Budget!'Projected Rate'
                let name = name.clone();
                self.advance();
                Ok(Expression::NameReference {
                    sheet: Some(sheet),
                    name,
                })
            }
            _ => Err(parse_error(
                format!("expected a cell or 'named cell' after '{sheet}!' — e.g. {sheet}!A:1"),
                self.current().position(),
            )),
        }
    }

    /// A (possibly qualified) cell, optionally extended to a range by `..`.
    pub(super) fn cell_reference_or_range(
        &mut self,
        sheet: Option<String>,
        column: String,
        row: i64,
    ) -> Result<Expression, EngineError> {
        if self.current().kind != TokenKind::DotDot {
            return Ok(Expression::CellReference { sheet, column, row });
        }
        self.advance();
        let TokenKind::CellReference {
            column: to_column,
            row: to_row,
            ..
        } = &self.current().kind
        else {
            return Err(parse_error(
                "expected a cell after '..' (e.g. A:1..A:9)",
                self.current().position(),
            ));
        };
        let (to_column, to_row) = (to_column.clone(), *to_row);
        self.advance();
        Ok(Expression::CellRange {
            sheet,
            from_column: column,
            from_row: row,
            to_column,
            to_row,
        })
    }

    /// Arguments after a consumed '(' — empty list allowed (`pi()` style not
    /// required, but `rand()` future-proofing is free).
    pub(super) fn argument_list(&mut self) -> Result<Vec<Expression>, EngineError> {
        if self.current().kind == TokenKind::RightParen {
            self.advance();
            return Ok(Vec::new());
        }
        // Named arguments — Person(name: "Ada", age: 36) — desugar to ONE map
        // literal, which makes them exactly the from-map constructor form.
        if self.is_named_argument_start() {
            return Ok(vec![self.named_arguments()?]);
        }
        let mut arguments = vec![self.comparison()?];
        while self.current().kind == TokenKind::Comma {
            self.advance();
            arguments.push(self.comparison()?);
        }
        if self.current().kind != TokenKind::RightParen {
            return Err(parse_error(
                "expected ')' or ','",
                self.current().position(),
            ));
        }
        self.advance();
        Ok(arguments)
    }

    /// Does this argument list open with `name: value`? Two shapes commit:
    /// an identifier directly followed by ':', or a compact `age:36` that the
    /// lexer fused into a cell-reference token with a MULTI-letter column —
    /// real columns are single letters, so that can't be a cell. A compact
    /// single-letter `f(a:1)` stays a cell reference; write `f(a: 1)`.
    fn is_named_argument_start(&self) -> bool {
        if matches!(self.current().kind, TokenKind::Identifier(_))
            && self.kind_at(self.index + 1) == Some(&TokenKind::Colon)
        {
            return true;
        }
        if let TokenKind::CellReference { column, .. } = &self.current().kind {
            if column.chars().count() > 1 {
                return true;
            }
        }
        false
    }

    /// `name: "Ada", age: 36)` after the consumed '(' — consumes the ')'.
    /// Same lexing wrinkle as map literals: a compact `key:number` arrives as
    /// one cell-reference token and decomposes back into key + value.
    fn named_arguments(&mut self) -> Result<Expression, EngineError> {
        let position = self.current().position();
        let mut entries: Vec<MapLiteralEntry> = Vec::new();

        fn append(
            entries: &mut Vec<MapLiteralEntry>,
            key: String,
            value: Expression,
            position: usize,
        ) -> Result<(), EngineError> {
            if entries.iter().any(|e| e.key == key) {
                return Err(parse_error(format!("duplicate field '{key}'"), position));
            }
            entries.push(MapLiteralEntry { key, value });
            Ok(())
        }

        loop {
            match &self.current().kind {
                TokenKind::CellReference { column, row, .. } => {
                    let (key, row) = (column.clone(), *row);
                    self.advance();
                    append(
                        &mut entries,
                        key,
                        Expression::Number(BigDecimal::from_int(row)),
                        position,
                    )?;
                }

                TokenKind::Identifier(key) => {
                    let key = key.clone();
                    self.advance();
                    if self.current().kind != TokenKind::Colon {
                        return Err(parse_error(
                            format!("expected ':' after '{key}' — named arguments are name: value"),
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    let value = self.comparison()?;
                    append(&mut entries, key, value, position)?;
                }

                _ => {
                    return Err(parse_error(
                        "expected another name: value argument",
                        self.current().position(),
                    ));
                }
            }

            match self.current().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightParen => break,
                _ => {
                    return Err(parse_error(
                        "expected ')' or ','",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // ')'
        Ok(Expression::MapLiteral(entries))
    }
}
