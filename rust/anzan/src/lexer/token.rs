//! A lexical token with its half-open character range in the source line.

use crate::eval::currency::Currency;
use crate::BigDecimal;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(BigDecimal),
    /// A finance-mode currency literal ($10, €10) — the glyph resolved to a
    /// `Currency`. Thousands grouping rides the value. Lexed only in finance
    /// mode.
    Money {
        value: BigDecimal,
        currency: Currency,
    },
    /// A finance-mode grouped plain number (138,561) — presentation only.
    Grouped(BigDecimal),
    Identifier(String),
    /// "…" — double-quoted string literal, escapes applied.
    String(String),
    /// A:1 — column as typed, row 1-based. The pins ($A:$1) are COPY-TIME
    /// data for the reference rewriter (fill/paste hold pinned axes);
    /// evaluation ignores them — $A:$1 and A:1 are the same cell.
    CellReference {
        column: String,
        row: i64,
        pin_column: bool,
        pin_row: bool,
    },
    /// .. — cell range separator (A:1..A:9).
    DotDot,
    /// . — map member access (m.name); .5 stays a number.
    Dot,
    /// -> — lambda (x -> x * 2); lexes before minus.
    Arrow,
    /// ! — sheet qualifier (Budget!A:1); != lexes first.
    Bang,
    /// 'Q1 Budget' — sheet names with spaces.
    QuotedName(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    /// << >> — Programmer-mode bit shifts (lexed mode-agnostically; parser
    /// gates).
    ShiftLeft,
    ShiftRight,
    /// & | — Programmer-mode bitwise AND/OR.
    Ampersand,
    Pipe,
    /// ~ — Programmer-mode bitwise NOT (needs a fixed width).
    Tilde,
    /// √ — prefix square root.
    SqrtSign,
    LessThan,
    GreaterThan,
    LessOrEqual,
    GreaterOrEqual,
    EqualEqual,
    NotEqual,
    /// =
    Assign,
    LeftParen,
    RightParen,
    /// [ ] — array literals and indexing.
    LeftBracket,
    RightBracket,
    /// { } — map literals.
    LeftBrace,
    RightBrace,
    /// : — map key separator (A:1 consumes its own ':').
    Colon,
    /// :: — namespace qualifier (Geometry::Point).
    ColonColon,
    /// ; — separates declarations inside a namespace block.
    Semicolon,
    Comma,
    /// Synthesized end-of-input marker.
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
}

impl Token {
    pub fn position(&self) -> usize {
        self.range.start
    }
}
