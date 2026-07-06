//! Token-precise rewriting of cell references inside raw cell text — the
//! engine behind structural edits (insert/delete rows & columns), fill/paste
//! reference adjustment, and sheet-rename rewriting. Same technique as
//! `NamedCells::rewriting`: lex, collect token ranges, splice back-to-front
//! so spacing and `# comments` survive. Every function returns `None` when
//! nothing matched (so callers skip untouched cells).
//!
//! Two token shapes are deliberately IGNORED everywhere: compact map keys
//! (`{b:1}` lexes as a cell-reference token but the parser decomposes it into
//! key + value — detected as "directly after `{` or `,` inside braces") and
//! multi-letter columns (`age:36` — named-argument sugar; real columns are
//! single letters A–Z).

use crate::cell_address::CellAddress;
use crate::spreadsheet::Spreadsheet;
use anzan::lexer::{Lexer, TokenKind};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

pub struct ReferenceRewriter;

impl ReferenceRewriter {
    // MARK: Structural shifts (insert/delete rows & columns)

    /// Rewrites references for an insert (`delta > 0`) or delete
    /// (`delta < 0`, the slots `index ..< index - delta` are removed) on
    /// `edited_sheet`. Positions are 1-based for rows, 0-based for columns —
    /// the same spaces `CellAddress` uses.
    ///
    /// Scope mirrors resolution: a QUALIFIED ref matches when its qualifier
    /// names the edited sheet (from any sheet); an UNQUALIFIED ref matches
    /// only when the formula lives on the edited sheet (`on_edited_sheet`).
    /// Pins shift like everything else — `$` is copy-time semantics, not an
    /// anchor against structural edits (Excel agrees).
    ///
    /// Deleted references become `refError()` (qualifier included in the
    /// splice); range corners clamp inward instead, and a fully-deleted
    /// range becomes `refError()` whole.
    pub fn shifting(
        raw: &str,
        axis: Axis,
        index: i64,
        delta: i64,
        edited_sheet: &str,
        on_edited_sheet: bool,
    ) -> Option<String> {
        if delta == 0 {
            return None;
        }
        let sites = Self::scan(raw)?;

        let edited = edited_sheet.to_lowercase();
        let matches = |site: &Site| -> bool {
            if let Some(qualifier) = &site.qualifier {
                return qualifier.to_lowercase() == edited;
            }
            on_edited_sheet
        };

        let dead_count = if delta < 0 { -delta } else { 0 };
        // None = deleted; otherwise the shifted position.
        let shifted = |position: i64| -> Option<i64> {
            if position >= index + dead_count {
                return Some(position + delta);
            }
            if position >= index {
                return if delta > 0 {
                    Some(position + delta)
                } else {
                    None
                };
            }
            Some(position)
        };

        let mut splices: Vec<(Range<usize>, String)> = Vec::new();
        for unit in Self::units(&sites) {
            match unit {
                Unit::Single(site) => {
                    if !matches(site) {
                        continue;
                    }
                    let Some(position) = shifted(site.position(axis)) else {
                        splices.push((site.splice_start()..site.range.end, "refError()".into()));
                        continue;
                    };
                    if position != site.position(axis) {
                        splices.push((site.range.clone(), site.text_on(position, axis)));
                    }
                }

                Unit::Pair(first, second) => {
                    if !matches(first) {
                        continue; // the qualifier rides corner one
                    }
                    let (lo, hi) = if first.position(axis) <= second.position(axis) {
                        (first, second)
                    } else {
                        (second, first)
                    };
                    // Clamp dead corners inward: the low corner lands on the
                    // first survivor after the hole, the high on the last
                    // before.
                    let mut lo_pos = lo.position(axis);
                    let mut hi_pos = hi.position(axis);
                    if dead_count > 0 && (index..index + dead_count).contains(&lo_pos) {
                        lo_pos = index + dead_count;
                    }
                    if dead_count > 0 && (index..index + dead_count).contains(&hi_pos) {
                        hi_pos = index - 1;
                    }
                    let (new_lo, new_hi) = match (shifted(lo_pos), shifted(hi_pos)) {
                        (Some(new_lo), Some(new_hi)) if lo_pos <= hi_pos => (new_lo, new_hi),
                        _ => {
                            splices.push((
                                first.splice_start()..second.range.end,
                                "refError()".into(),
                            ));
                            continue;
                        }
                    };
                    if new_lo != lo.position(axis) {
                        splices.push((lo.range.clone(), lo.text_on(new_lo, axis)));
                    }
                    if new_hi != hi.position(axis) {
                        splices.push((hi.range.clone(), hi.text_on(new_hi, axis)));
                    }
                }
            }
        }
        Self::apply(splices, raw)
    }

    // MARK: Relative adjustment (fill / paste)

    /// Shifts every reference's unpinned axes by the copy offset — the heart
    /// of fill down/right and in-app paste. Qualified refs adjust too
    /// (Excel-style); named-cell references never do (names are the
    /// absolute-by-meaning reference). A ref pushed off the grid becomes
    /// `refError()`; a range with a dead corner dies whole.
    pub fn adjusting_relative(raw: &str, by_rows: i64, by_columns: i64) -> Option<String> {
        if by_rows == 0 && by_columns == 0 {
            return None;
        }
        let sites = Self::scan(raw)?;

        let adjusted = |site: &Site| -> Option<(i64, i64)> {
            let row = if site.pin_row {
                site.row
            } else {
                site.row + by_rows
            };
            let column = if site.pin_column {
                site.column_index
            } else {
                site.column_index + by_columns
            };
            if !(1..=Spreadsheet::ROW_COUNT as i64).contains(&row)
                || !(0..Spreadsheet::COLUMN_COUNT as i64).contains(&column)
            {
                return None;
            }
            Some((row, column))
        };

        let mut splices: Vec<(Range<usize>, String)> = Vec::new();
        for unit in Self::units(&sites) {
            match unit {
                Unit::Single(site) => {
                    let Some((row, column)) = adjusted(site) else {
                        splices.push((site.splice_start()..site.range.end, "refError()".into()));
                        continue;
                    };
                    if row != site.row || column != site.column_index {
                        splices.push((site.range.clone(), site.text(row, column, false)));
                    }
                }

                Unit::Pair(first, second) => {
                    let (Some(new_first), Some(new_second)) = (adjusted(first), adjusted(second))
                    else {
                        splices.push((first.splice_start()..second.range.end, "refError()".into()));
                        continue;
                    };
                    for (site, new) in [(first, new_first), (second, new_second)] {
                        if new.0 != site.row || new.1 != site.column_index {
                            splices.push((site.range.clone(), site.text(new.0, new.1, false)));
                        }
                    }
                }
            }
        }
        Self::apply(splices, raw)
    }

    // MARK: Sheet rename

    /// Rewrites `Old!…` / `'Old Name'!…` qualifiers to the new spelling —
    /// bare when the new name is identifier-shaped, quoted otherwise.
    /// A quoted name NOT followed by `!` is a named cell and stays put.
    pub fn renaming_sheet(raw: &str, old_name: &str, new_name: &str) -> Option<String> {
        let tokens = Lexer::tokenize(raw).ok()?;

        let old = old_name.to_lowercase();
        let mut splices: Vec<(Range<usize>, String)> = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let name = match &token.kind {
                TokenKind::Identifier(n) | TokenKind::QuotedName(n) => n,
                _ => continue,
            };
            if index + 1 >= tokens.len()
                || !matches!(tokens[index + 1].kind, TokenKind::Bang)
                || name.to_lowercase() != old
            {
                continue;
            }
            splices.push((token.range.clone(), Self::spelled(new_name)));
        }
        Self::apply(splices, raw)
    }

    /// Sheet names render bare when the identifier syntax can carry them.
    fn spelled(name: &str) -> String {
        let mut chars = name.chars();
        let identifier_shaped = match chars.next() {
            Some(first) => {
                (first.is_alphabetic() || first == '_')
                    && name
                        .chars()
                        .all(|c| c.is_alphabetic() || c.is_numeric() || c == '_')
            }
            None => false,
        };
        if identifier_shaped {
            name.to_string()
        } else {
            format!("'{name}'")
        }
    }

    // MARK: Token scanning

    /// All real reference sites in the raw text, or `None` when it doesn't
    /// lex (plain labels) or holds none.
    fn scan(raw: &str) -> Option<Vec<Site>> {
        let tokens = Lexer::tokenize(raw).ok()?;

        let mut sites: Vec<Site> = Vec::new();
        let mut brackets: Vec<&TokenKind> = Vec::new(); // innermost-bracket tracking for {b:1}
        for (index, token) in tokens.iter().enumerate() {
            match &token.kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                    brackets.push(&token.kind);
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                    brackets.pop();
                }

                TokenKind::CellReference {
                    column,
                    row,
                    pin_column,
                    pin_row,
                } => {
                    // Multi-letter columns are named-argument sugar, never
                    // cells.
                    if column.chars().count() != 1 {
                        continue;
                    }
                    let Some(column_index) = CellAddress::column_index(column) else {
                        continue;
                    };
                    // Compact map key: directly after `{` or `,` while the
                    // innermost bracket is a brace (mirrors Parser's map
                    // literal).
                    if matches!(brackets.last(), Some(TokenKind::LeftBrace))
                        && index > 0
                        && matches!(
                            tokens[index - 1].kind,
                            TokenKind::LeftBrace | TokenKind::Comma
                        )
                    {
                        continue;
                    }
                    let mut qualifier: Option<String> = None;
                    let mut qualifier_start: Option<usize> = None;
                    if index >= 2 && matches!(tokens[index - 1].kind, TokenKind::Bang) {
                        match &tokens[index - 2].kind {
                            TokenKind::Identifier(sheet) | TokenKind::QuotedName(sheet) => {
                                qualifier = Some(sheet.clone());
                                qualifier_start = Some(tokens[index - 2].range.start);
                            }
                            _ => {}
                        }
                    }
                    let followed_by_dot_dot = index + 1 < tokens.len()
                        && matches!(tokens[index + 1].kind, TokenKind::DotDot);
                    sites.push(Site {
                        column: column.clone(),
                        column_index: column_index as i64,
                        row: *row,
                        pin_column: *pin_column,
                        pin_row: *pin_row,
                        range: token.range.clone(),
                        qualifier,
                        qualifier_start,
                        token_index: index,
                        followed_by_dot_dot,
                    });
                }
                _ => {}
            }
        }

        // Pairing happens against the token stream: site, `..`, site.
        if sites.is_empty() {
            None
        } else {
            Some(sites)
        }
    }

    /// Groups consecutive sites joined by `..` into range pairs.
    fn units(sites: &[Site]) -> Vec<Unit<'_>> {
        let mut units: Vec<Unit<'_>> = Vec::new();
        let mut index = 0;
        while index < sites.len() {
            let site = &sites[index];
            if site.followed_by_dot_dot
                && index + 1 < sites.len()
                && sites[index + 1].token_index == site.token_index + 2
                && sites[index + 1].qualifier.is_none()
            // corner two is always bare
            {
                units.push(Unit::Pair(site, &sites[index + 1]));
                index += 2;
                continue;
            }
            units.push(Unit::Single(site));
            index += 1;
        }
        units
    }

    /// Splices replacements back-to-front; `None` when there are none.
    fn apply(mut splices: Vec<(Range<usize>, String)>, raw: &str) -> Option<String> {
        if splices.is_empty() {
            return None;
        }
        let mut characters: Vec<char> = raw.chars().collect();
        splices.sort_by_key(|splice| std::cmp::Reverse(splice.0.start));
        for (range, text) in splices {
            characters.splice(range, text.chars());
        }
        Some(characters.into_iter().collect())
    }
}

/// One real cell-reference token and everything a rewrite needs.
struct Site {
    /// As typed.
    column: String,
    column_index: i64,
    row: i64,
    pin_column: bool,
    pin_row: bool,
    range: Range<usize>,
    /// Budget!A:1 — set on the ref AFTER the bang.
    qualifier: Option<String>,
    /// Char start of the qualifier token.
    qualifier_start: Option<usize>,
    token_index: usize,
    /// A:1.. — opens a range.
    followed_by_dot_dot: bool,
}

impl Site {
    /// refError() splices swallow the qualifier too — `Budget!refError()`
    /// wouldn't parse.
    fn splice_start(&self) -> usize {
        self.qualifier_start.unwrap_or(self.range.start)
    }

    fn position(&self, axis: Axis) -> i64 {
        match axis {
            Axis::Row => self.row,
            Axis::Column => self.column_index,
        }
    }

    /// The token re-rendered with one axis changed (pins and, where the
    /// column is unchanged, its typed case are preserved).
    fn text_on(&self, position: i64, axis: Axis) -> String {
        match axis {
            Axis::Row => self.text(position, self.column_index, true),
            Axis::Column => self.text(self.row, position, false),
        }
    }

    fn text(&self, new_row: i64, new_column: i64, keep_column_case: bool) -> String {
        let column_text = if keep_column_case || new_column == self.column_index {
            self.column.clone()
        } else {
            CellAddress::column_name_for(new_column as usize)
        };
        format!(
            "{}{}:{}{}",
            if self.pin_column { "$" } else { "" },
            column_text,
            if self.pin_row { "$" } else { "" },
            new_row
        )
    }
}

/// A standalone reference or a `lo..hi` range pair (corners share the
/// first corner's qualifier — that's how the parser scopes ranges).
enum Unit<'a> {
    Single(&'a Site),
    Pair(&'a Site, &'a Site),
}

#[cfg(test)]
mod tests;
