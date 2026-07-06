//! Tests for token-precise cell-reference rewriting.

use super::{Axis, ReferenceRewriter};
use crate::cell_address::CellAddress;
use crate::sheet_store::SheetStore;
use anzan::{BigDecimal, Calculator};
use std::cell::RefCell;
use std::rc::Rc;

fn adjusting(raw: &str, by_rows: i64, by_columns: i64) -> Option<String> {
    ReferenceRewriter::adjusting_relative(raw, by_rows, by_columns)
}

fn shifting(
    raw: &str,
    axis: Axis,
    index: i64,
    delta: i64,
    edited_sheet: &str,
    on_edited_sheet: bool,
) -> Option<String> {
    ReferenceRewriter::shifting(raw, axis, index, delta, edited_sheet, on_edited_sheet)
}

// MARK: $ pins (lexer + evaluation transparency)

#[test]
fn pins_lex_and_evaluate_like_plain_references() {
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    store
        .active_sheet()
        .grid
        .set_cell(Some("42"), CellAddress::new(0, 0));
    let value = |input: &str| {
        calculator
            .borrow_mut()
            .evaluate(input)
            .unwrap()
            .numeric_value()
            .unwrap()
    };
    assert_eq!(value("$A:$1 + A:1"), BigDecimal::from_int(84));
    assert_eq!(value("$A:1 * 2"), BigDecimal::from_int(84));
    assert_eq!(value("A:$1 * 2"), BigDecimal::from_int(84));
}

#[test]
fn dollar_alone_is_a_loud_lex_error() {
    let mut calculator = Calculator::new();
    for input in ["$", "$x", "$5", "2 + $", "$A", "$A:"] {
        let Err(error) = calculator.evaluate(input) else {
            panic!("'{input}' should be a lex error");
        };
        assert!(
            error.to_string().contains('$'),
            "'{input}' error should mention '$': {error}"
        );
    }
}

// MARK: adjusting_relative (fill / paste)

#[test]
fn adjusts_unpinned_axes_and_holds_pins() {
    assert_eq!(
        adjusting("=A:2 * rate", 1, 0),
        Some("=A:3 * rate".to_string())
    );
    assert_eq!(
        adjusting("=A:2 * $C:$1", 2, 0),
        Some("=A:4 * $C:$1".to_string())
    );
    assert_eq!(
        adjusting("=$A:2 + B:$5", 3, 1),
        Some("=$A:5 + C:$5".to_string())
    );
    // Comments and spacing survive (token-precise splices).
    assert_eq!(
        adjusting("= A:1  + 2  # note", 1, 0),
        Some("= A:2  + 2  # note".to_string())
    );
    // Nothing to adjust → None.
    assert_eq!(adjusting("= 1 + 2", 1, 0), None);
    assert_eq!(adjusting("=A:1", 0, 0), None);
}

#[test]
fn adjusting_moves_qualified_refs_and_skips_named_cells() {
    assert_eq!(
        adjusting("=Budget!A:1 * 2", 1, 0),
        Some("=Budget!A:2 * 2".to_string())
    );
    assert_eq!(
        adjusting("='Q1 Budget'!B:3", 0, 1),
        Some("='Q1 Budget'!C:3".to_string())
    );
    // Named cells are the absolute-by-meaning reference.
    assert_eq!(adjusting("='Projected Rate' * 2", 5, 0), None);
    assert_eq!(
        adjusting("=Budget!'Rate' + A:1", 1, 0),
        Some("=Budget!'Rate' + A:2".to_string())
    );
}

#[test]
fn adjusting_off_the_grid_becomes_ref_error() {
    assert_eq!(
        adjusting("=A:1 * 2", -1, 0),
        Some("=refError() * 2".to_string())
    );
    assert_eq!(
        adjusting("=A:1 + B:1", 0, -1),
        Some("=refError() + A:1".to_string())
    );
    assert_eq!(adjusting("=Z:1", 0, 1), Some("=refError()".to_string()));
    assert_eq!(adjusting("=A:1000", 1, 0), Some("=refError()".to_string()));
    // A dead corner kills the whole range; the qualifier goes with it.
    assert_eq!(
        adjusting("=sum(A:1..A:9)", -1, 0),
        Some("=sum(refError())".to_string())
    );
    assert_eq!(
        adjusting("=sum(Budget!A:1..A:9)", -1, 0),
        Some("=sum(refError())".to_string())
    );
}

#[test]
fn adjusting_skips_map_keys_and_named_arguments() {
    // {b:1} lexes as a cell-reference token but is a map KEY.
    assert_eq!(adjusting("={b:1}", 1, 0), None);
    assert_eq!(adjusting("={a:1, b:2}", 1, 0), None);
    // …while a map VALUE that's a real reference adjusts.
    assert_eq!(adjusting("={x: A:1}", 1, 0), Some("={x: A:2}".to_string()));
    // Multi-letter "columns" are named-argument sugar, never cells.
    assert_eq!(adjusting("=Person(age:36)", 1, 0), None);
    // Compact single-letter call args ARE cell references (documented).
    assert_eq!(adjusting("=f(a:1)", 1, 0), Some("=f(a:2)".to_string()));
    // An array of refs adjusts each element (brackets aren't braces).
    assert_eq!(
        adjusting("=[A:1, B:2]", 1, 0),
        Some("=[A:2, B:3]".to_string())
    );
}

// MARK: shifting (insert/delete rows & columns)

#[test]
fn insert_shifts_references_at_or_below() {
    // Insert one row at row 3: refs to 3+ move down.
    assert_eq!(
        shifting("=A:2 + A:3 + A:9", Axis::Row, 3, 1, "Sheet 1", true),
        Some("=A:2 + A:4 + A:10".to_string())
    );
    // Unqualified refs on OTHER sheets stay put…
    assert_eq!(shifting("=A:3", Axis::Row, 3, 1, "Sheet 1", false), None);
    // …but qualified ones follow the edited sheet from anywhere.
    assert_eq!(
        shifting("='Sheet 1'!A:3 + A:3", Axis::Row, 3, 1, "Sheet 1", false),
        Some("='Sheet 1'!A:4 + A:3".to_string())
    );
    // Columns shift by index (insert at B pushes B→C).
    assert_eq!(
        shifting("=B:1 * $B:$2", Axis::Column, 1, 1, "Sheet 1", true),
        Some("=C:1 * $C:$2".to_string())
    );
}

#[test]
fn delete_rewrites_dead_refs_to_ref_error() {
    // Delete row 3: refs above stay, below shift up, AT it die loudly.
    assert_eq!(
        shifting("=A:2 + A:3 + A:9", Axis::Row, 3, -1, "Sheet 1", true),
        Some("=A:2 + refError() + A:8".to_string())
    );
    // The qualifier dies with the reference.
    assert_eq!(
        shifting("=Budget!A:3 * 2", Axis::Row, 3, -1, "Budget", false),
        Some("=refError() * 2".to_string())
    );
    // Delete column B: C slides into B; B itself dies.
    assert_eq!(
        shifting("=B:1 + C:1", Axis::Column, 1, -1, "Sheet 1", true),
        Some("=refError() + B:1".to_string())
    );
}

#[test]
fn delete_shrinks_ranges_inward() {
    // Interior delete: the range just shortens at the far end.
    assert_eq!(
        shifting("=sum(A:1..A:5)", Axis::Row, 3, -1, "Sheet 1", true),
        Some("=sum(A:1..A:4)".to_string())
    );
    // Endpoint deletes clamp inward.
    assert_eq!(
        shifting("=sum(A:3..A:5)", Axis::Row, 3, -1, "Sheet 1", true),
        Some("=sum(A:3..A:4)".to_string())
    );
    assert_eq!(
        shifting("=sum(A:1..A:5)", Axis::Row, 5, -1, "Sheet 1", true),
        Some("=sum(A:1..A:4)".to_string())
    );
    // Reversed corners keep their orientation.
    assert_eq!(
        shifting("=sum(A:5..A:1)", Axis::Row, 5, -1, "Sheet 1", true),
        Some("=sum(A:4..A:1)".to_string())
    );
    // Deleting the whole span kills the range.
    assert_eq!(
        shifting("=sum(A:3..A:4) + 1", Axis::Row, 3, -2, "Sheet 1", true),
        Some("=sum(refError()) + 1".to_string())
    );
    // Multi-row delete spanning an endpoint.
    assert_eq!(
        shifting("=sum(A:2..A:6)", Axis::Row, 4, -3, "Sheet 1", true),
        Some("=sum(A:2..A:3)".to_string())
    );
}

#[test]
fn range_pairing_needs_the_dot_dot_token() {
    // Two refs an operator apart are NOT a range — each shifts alone.
    assert_eq!(
        shifting("=A:3 + A:5", Axis::Row, 3, -1, "Sheet 1", true),
        Some("=refError() + A:4".to_string())
    );
}

// MARK: renaming_sheet

#[test]
fn rename_rewrites_both_quoting_styles() {
    assert_eq!(
        ReferenceRewriter::renaming_sheet(
            "=Budget!A:1 + budget!B:2 * 'Budget'!C:3",
            "Budget",
            "Plan"
        ),
        Some("=Plan!A:1 + Plan!B:2 * Plan!C:3".to_string())
    );
    // A new name that needs quoting gets it.
    assert_eq!(
        ReferenceRewriter::renaming_sheet("=Budget!A:1", "Budget", "Q1 Plan"),
        Some("='Q1 Plan'!A:1".to_string())
    );
    // Named-cell references with the same spelling stay put (no bang).
    assert_eq!(
        ReferenceRewriter::renaming_sheet("='Budget' + Budget!A:1", "Budget", "Plan"),
        Some("='Budget' + Plan!A:1".to_string())
    );
    // Other sheets' qualifiers are untouched; None when nothing matched.
    assert_eq!(
        ReferenceRewriter::renaming_sheet("=Costs!A:1", "Budget", "Plan"),
        None
    );
    assert_eq!(
        ReferenceRewriter::renaming_sheet("plain label", "Budget", "Plan"),
        None
    );
}
