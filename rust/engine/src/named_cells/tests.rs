//! Tests for named-cell reference rewriting.

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
