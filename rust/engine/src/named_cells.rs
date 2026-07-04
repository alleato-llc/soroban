//! Reference-rewriting for named cells: renaming a name updates every
//! formula that uses it; deleting offers to inline the cell address instead.
//! Token-precise (the lexer's character ranges), so spacing and `# comments`
//! survive — the same technique control expressions use.

use anzan::lexer::{Lexer, TokenKind};
use std::ops::Range;

pub struct NamedCells;

impl NamedCells {
    /// Rewrites every reference to `old_name` in one raw cell text.
    ///
    /// Scoping rules mirror resolution: an UNQUALIFIED `'Old'` only refers to
    /// this name when the formula lives on the name's own sheet
    /// (`on_owning_sheet`); a QUALIFIED `Sheet!'Old'` refers to it from
    /// anywhere when the qualifier is the owning sheet. References to
    /// same-spelled names on OTHER sheets are left alone.
    ///
    /// `replacement` is spliced over the quoted token only — pass `'New
    /// Name'` (quoted) for renames or `B:7` for address inlining; qualifiers
    /// stay put either way. Returns `None` when nothing matched.
    pub fn rewriting(
        raw: &str,
        old_name: &str,
        owning_sheet: Option<&str>,
        on_owning_sheet: bool,
        replacement: &str,
    ) -> Option<String> {
        let tokens = Lexer::tokenize(raw).ok()?;
        let old = old_name.to_lowercase();
        let owning = owning_sheet.map(str::to_lowercase);

        // Collect matching quotedName token ranges, back to front.
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let TokenKind::QuotedName(name) = &token.kind else {
                continue;
            };
            if name.to_lowercase() != old {
                continue;
            }
            // A quoted name FOLLOWED by ! is a sheet qualifier, not a name.
            if index + 1 < tokens.len() && matches!(tokens[index + 1].kind, TokenKind::Bang) {
                continue;
            }

            // Qualified? Look back for `sheet` `!`.
            let mut qualifier: Option<&String> = None;
            if index >= 2 && matches!(tokens[index - 1].kind, TokenKind::Bang) {
                match &tokens[index - 2].kind {
                    TokenKind::Identifier(sheet) | TokenKind::QuotedName(sheet) => {
                        qualifier = Some(sheet);
                    }
                    _ => {}
                }
            }

            let matches = match qualifier {
                Some(qualifier) => owning
                    .as_deref()
                    .is_some_and(|owning| qualifier.to_lowercase() == owning),
                None => on_owning_sheet,
            };
            if matches {
                ranges.push(token.range.clone());
            }
        }
        if ranges.is_empty() {
            return None;
        }

        let mut characters: Vec<char> = raw.chars().collect();
        for range in ranges.into_iter().rev() {
            characters.splice(range, replacement.chars());
        }
        Some(characters.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::NamedCells;

    // Port of NamedCellTests.rewritingRespectsScoping.
    #[test]
    fn rewriting_respects_scoping() {
        // Unqualified — rewritten only on the owning sheet.
        assert_eq!(
            NamedCells::rewriting(
                "'Rate' * 12  # note",
                "rate",
                Some("Sheet 1"),
                true,
                "'APR'"
            ),
            Some("'APR' * 12  # note".to_string())
        );
        assert_eq!(
            NamedCells::rewriting("'Rate' * 12", "Rate", Some("Sheet 1"), false, "'APR'"),
            None
        );

        // Qualified — rewritten anywhere, but only with the owning sheet.
        assert_eq!(
            NamedCells::rewriting(
                "Budget!'Rate' + 'Rate'",
                "Rate",
                Some("Budget"),
                false,
                "B:7"
            ),
            Some("Budget!B:7 + 'Rate'".to_string())
        );
        assert_eq!(
            NamedCells::rewriting("Other!'Rate'", "Rate", Some("Budget"), false, "B:7"),
            None
        );

        // A quoted SHEET qualifier with the same spelling is left alone.
        assert_eq!(
            NamedCells::rewriting("'Rate'!A:1 + 'Rate'", "Rate", Some("Sheet 1"), true, "B:7"),
            Some("'Rate'!A:1 + B:7".to_string())
        );

        // Multiple occurrences, all spliced.
        assert_eq!(
            NamedCells::rewriting("'x' + 'x'", "x", None, true, "A:1"),
            Some("A:1 + A:1".to_string())
        );
    }

    // Port of EdgePathTests.namedCellRewritingTokenForms.
    #[test]
    fn rewriting_token_forms() {
        // Quoted sheet qualifier.
        assert_eq!(
            NamedCells::rewriting(
                "'My Sheet'!'Rate' * 2",
                "rate",
                Some("My Sheet"),
                false,
                "B:7"
            ),
            Some("'My Sheet'!B:7 * 2".to_string())
        );
        // Unparseable raws are left alone.
        assert_eq!(
            NamedCells::rewriting("'unterminated", "x", None, true, "B:7"),
            None
        );
        // Qualified reference with no owning sheet recorded → no match.
        assert_eq!(
            NamedCells::rewriting("Budget!'Rate'", "Rate", None, false, "B:7"),
            None
        );
    }
}
