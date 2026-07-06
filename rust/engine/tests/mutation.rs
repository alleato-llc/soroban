//! Workbook mutation — the log-only commands' DIRECT (no-undo) default that
//! `SheetStore::new` installs: `updateCell` / `addWorksheet` /
//! `renameWorksheet` / `deleteWorksheet`, reached by EVALUATING them as log
//! lines. The port of the behavior Swift's `SheetStore+Mutation.swift` proves
//! through its session gherkin — happy paths, the worksheet-target resolution
//! (handle or name), the reference-rewrite on rename, the weak-handle stale
//! reads, and every argument-shape error branch, plus the log-only gate (a
//! mutation attempted from a CELL must throw).

use soroban_engine::{
    BigDecimal, Calculator, CellAddress, CellDisplay, EvalOutcome, SheetStore, Value,
};
use std::cell::RefCell;
use std::rc::Rc;

fn make_store() -> (Rc<RefCell<Calculator>>, SheetStore) {
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    (calculator, store)
}

fn addr(key: &str) -> CellAddress {
    CellAddress::from_key(key).expect("a valid test address")
}

fn log(calculator: &Rc<RefCell<Calculator>>, line: &str) -> EvalOutcome {
    calculator
        .borrow_mut()
        .evaluate(line)
        .unwrap_or_else(|e| panic!("'{line}' failed: {e}"))
}

/// The Value behind a log line that evaluates to one.
fn value(calculator: &Rc<RefCell<Calculator>>, line: &str) -> Value {
    match log(calculator, line) {
        EvalOutcome::Value(value) => value,
        other => panic!("'{line}' produced a non-value outcome: {other:?}"),
    }
}

/// The EngineError message a log line raises.
fn error(calculator: &Rc<RefCell<Calculator>>, line: &str) -> String {
    calculator
        .borrow_mut()
        .evaluate(line)
        .expect_err(&format!("'{line}' should fail"))
        .to_string()
}

fn num(n: i64) -> Value {
    Value::Number(BigDecimal::from_int(n))
}

// MARK: updateCell

#[test]
fn update_cell_writes_a_number_and_recalculates() {
    let (calc, store) = make_store();
    // The command returns its value argument; the cell then shows it.
    assert_eq!(value(&calc, "updateCell(cell(\"A\", 1), 42)"), num(42));
    assert_eq!(
        store.display_value(addr("A:1")),
        CellDisplay::Value(BigDecimal::from_int(42))
    );
    // A dependent formula written afterwards sees the number.
    log(&calc, "updateCell(cell(\"A\", 2), \"=A:1 * 2\")");
    assert_eq!(
        store.display_value(addr("A:2")),
        CellDisplay::Value(BigDecimal::from_int(84))
    );
}

#[test]
fn update_cell_writes_verbatim_strings_and_clears_on_empty() {
    let (calc, store) = make_store();
    // A string is taken verbatim: a leading `=` writes a formula, plain text
    // a label.
    log(&calc, "updateCell(cell(\"B\", 1), 10)");
    log(&calc, "updateCell(cell(\"A\", 1), \"=B:1 + 5\")");
    assert_eq!(
        store.display_value(addr("A:1")),
        CellDisplay::Value(BigDecimal::from_int(15))
    );
    log(&calc, "updateCell(cell(\"A\", 2), \"Total\")");
    assert_eq!(
        store.display_value(addr("A:2")),
        CellDisplay::Text("Total".to_string())
    );
    // An empty string clears the cell.
    log(&calc, "updateCell(cell(\"A\", 1), \"\")");
    assert_eq!(store.display_value(addr("A:1")), CellDisplay::Empty);
}

#[test]
fn update_cell_argument_errors() {
    let (calc, _store) = make_store();
    assert!(error(&calc, "updateCell(cell(\"A\", 1))")
        .contains("updateCell(cell, value) takes a cell and a value"));
    // First argument must be a cell handle, not a number.
    assert!(error(&calc, "updateCell(5, 3)").contains("first argument is a cell"));
    // The value must be a number or text — a structure can't live in a cell.
    assert!(error(&calc, "updateCell(cell(\"A\", 1), [1, 2])").contains("a cell holds a number"));
}

// MARK: addWorksheet

#[test]
fn add_worksheet_appends_and_returns_the_handle() {
    let (calc, store) = make_store();
    assert_eq!(
        value(&calc, "addWorksheet(\"Budget\").name"),
        Value::String("Budget".into())
    );
    assert_eq!(store.sheet_count(), 2);
    assert!(store.sheet_named("Budget").is_some());
    // A new sheet can satisfy a formula that named it before it existed.
    log(&calc, "updateCell(cell(\"A\", 1), \"='Budget'!A:1 + 1\")");
    log(&calc, "updateCell(cell(\"Budget\", \"A\", 1), 9)");
    assert_eq!(
        store.display_value(addr("A:1")),
        CellDisplay::Value(BigDecimal::from_int(10))
    );
}

#[test]
fn add_worksheet_argument_and_name_errors() {
    let (calc, _store) = make_store();
    assert!(error(&calc, "addWorksheet()").contains("addWorksheet(name) takes a sheet name"));
    assert!(error(&calc, "addWorksheet(5)").contains("addWorksheet(name) takes a sheet name"));
    // Two arguments is also the wrong shape.
    assert!(error(&calc, "addWorksheet(\"a\", \"b\")").contains("addWorksheet(name)"));
    // Duplicate (case-insensitive) and syntactically illegal names are refused.
    assert!(error(&calc, "addWorksheet(\"sheet 1\")").contains("already exists"));
    assert!(error(&calc, "addWorksheet(\"Bad!Name\")").contains("! or '"));
}

// MARK: renameWorksheet

#[test]
fn rename_worksheet_by_name_and_by_handle() {
    let (calc, store) = make_store();
    // By name.
    log(&calc, "renameWorksheet(\"Sheet 1\", \"Main\")");
    assert_eq!(store.sheets()[0].name(), "Main");
    // By handle — the returned value is a live Worksheet handle.
    assert_eq!(
        value(
            &calc,
            "renameWorksheet(Workbook.worksheets[0], \"Home\").name"
        ),
        Value::String("Home".into())
    );
    assert_eq!(store.sheets()[0].name(), "Home");
}

#[test]
fn rename_worksheet_rewrites_cross_sheet_references() {
    let (calc, store) = make_store();
    log(&calc, "addWorksheet(\"Data\")");
    log(&calc, "updateCell(cell(\"Data\", \"A\", 1), 10)");
    log(&calc, "updateCell(cell(\"A\", 1), \"='Data'!A:1 + 1\")");
    assert_eq!(
        store.display_value(addr("A:1")),
        CellDisplay::Value(BigDecimal::from_int(11))
    );

    // Renaming Data auto-rewrites the qualifier; the value survives.
    log(&calc, "renameWorksheet(\"Data\", \"Facts\")");
    assert_eq!(
        store.display_value(addr("A:1")),
        CellDisplay::Value(BigDecimal::from_int(11))
    );
    assert!(store.active_sheet().grid.raw(addr("A:1")).contains("Facts"));
}

#[test]
fn rename_worksheet_to_the_same_name_skips_the_rewrite() {
    // old_name == resolved: the rename succeeds but the reference-rewrite loop
    // is skipped (the `if old_name != resolved` false branch).
    let (calc, store) = make_store();
    log(&calc, "renameWorksheet(\"Sheet 1\", \"Sheet 1\")");
    assert_eq!(store.sheets()[0].name(), "Sheet 1");
}

#[test]
fn rename_worksheet_argument_errors() {
    let (calc, _store) = make_store();
    log(&calc, "addWorksheet(\"Data\")");
    assert!(
        error(&calc, "renameWorksheet(\"Sheet 1\")").contains("renameWorksheet(sheet, newName)")
    );
    // The new name must be text.
    assert!(error(&calc, "renameWorksheet(\"Sheet 1\", 5)").contains("new name is text"));
    // Unknown target sheet.
    assert!(error(&calc, "renameWorksheet(\"Nope\", \"X\")").contains("unknown sheet 'Nope'"));
    // A duplicate destination name is refused by validation.
    assert!(error(&calc, "renameWorksheet(\"Data\", \"Sheet 1\")").contains("already exists"));
    // A non-worksheet, non-name target.
    assert!(
        error(&calc, "renameWorksheet(5, \"X\")").contains("expected a worksheet or a sheet name")
    );
    // A host handle that isn't a worksheet (a cell) is not a valid target.
    assert!(error(&calc, "renameWorksheet(cell(\"A\", 1), \"X\")")
        .contains("no longer in the workbook"));
}

// MARK: deleteWorksheet

#[test]
fn delete_worksheet_by_name_and_by_handle() {
    let (calc, store) = make_store();
    log(&calc, "addWorksheet(\"Data\")");
    log(&calc, "addWorksheet(\"Scratch\")");
    assert_eq!(store.sheet_count(), 3);
    // By name → returns the new count.
    assert_eq!(value(&calc, "deleteWorksheet(\"Scratch\")"), num(2));
    // By handle.
    assert_eq!(
        value(&calc, "deleteWorksheet(Workbook.worksheets[\"Data\"])"),
        num(1)
    );
    assert_eq!(store.sheet_count(), 1);
}

#[test]
fn delete_worksheet_argument_errors() {
    let (calc, _store) = make_store();
    assert!(error(&calc, "deleteWorksheet()").contains("deleteWorksheet(sheet)"));
    assert!(error(&calc, "deleteWorksheet(\"a\", \"b\")").contains("deleteWorksheet(sheet)"));
    // The last sheet can't be removed.
    assert!(error(&calc, "deleteWorksheet(\"Sheet 1\")")
        .contains("a workbook needs at least one sheet"));
    // Unknown target.
    assert!(error(&calc, "deleteWorksheet(\"Nope\")").contains("unknown sheet 'Nope'"));
}

// MARK: The log-only gate (a mutation from a CELL must throw)

#[test]
fn a_mutation_from_a_cell_is_refused() {
    // `in_log` is false during cell recalc, so the resolver throws — recalc
    // must stay reproducible (the rand() principle).
    let (_calc, store) = make_store();
    let sheet = store.active_sheet();
    sheet
        .grid
        .set_cell(Some("=updateCell(cell(\"A\", 2), 5)"), addr("A:1"));
    match store.display_value_on(&sheet, addr("A:1")) {
        CellDisplay::Error(message) => {
            assert!(
                message.contains("runs in the calculation log, not a cell"),
                "{message}"
            );
        }
        other => panic!("expected the log-only gate to fire: {other:?}"),
    }
    // The target cell stayed untouched (the mutation never ran).
    assert_eq!(
        store.display_value_on(&sheet, addr("A:2")),
        CellDisplay::Empty
    );
}

// MARK: Weak handles — reads after the sheet is gone throw cleanly

#[test]
fn stale_worksheet_and_cell_handles_read_cleanly() {
    let (calc, store) = make_store();
    log(&calc, "addWorksheet(\"Temp\")");
    // Stash live handles, then delete the sheet they point at.
    log(&calc, "w = Workbook.worksheets[\"Temp\"]");
    log(&calc, "c = w.cell(\"A\", 1)");
    log(&calc, "deleteWorksheet(\"Temp\")");
    assert_eq!(store.sheet_count(), 1);

    // A method call on the dead worksheet fails with a clean message.
    assert!(error(&calc, "w.cell(\"A\", 1)").contains("worksheet is no longer available"));
    // A member read on the dead worksheet: member() returns None → "no member".
    assert!(error(&calc, "w.name").contains("has no member"));
    // The cell handle's grid is gone too.
    assert!(error(&calc, "c.value").contains("has no member"));
    // And an updateCell through the stale cell handle is refused.
    assert!(error(&calc, "updateCell(c, 5)").contains("no longer in the workbook"));
}

// MARK: The public mutation seams (a host's undoable override reuses them)

#[test]
fn public_seams_resolve_targets_and_raw_text() {
    let (calc, store) = make_store();
    log(&calc, "addWorksheet(\"Second\")");

    // A worksheet TARGET resolves from a name string and from a handle.
    assert_eq!(
        store
            .sheet_index_for_target(&Value::String("Second".into()))
            .expect("name resolves"),
        1
    );
    let handle = value(&calc, "Workbook.worksheets[1]");
    assert_eq!(
        store
            .sheet_index_for_target(&handle)
            .expect("handle resolves"),
        1
    );

    // worksheet_handle round-trips back to the same index.
    let built = store.worksheet_handle(0);
    assert_eq!(
        store.sheet_index_for_target(&built).expect("built handle"),
        0
    );

    // A CELL handle resolves to (sheet index, address).
    let cell = value(&calc, "cell(\"B\", 3)");
    assert_eq!(
        store.cell_target(&cell).expect("cell target"),
        (0, addr("B:3"))
    );

    // cell_target refuses non-cells and worksheet handles.
    assert!(store.cell_target(&num(5)).is_err());
    assert!(store.cell_target(&handle).is_err());

    // raw_text_from: numbers → digits, strings verbatim, structures refused.
    assert_eq!(SheetStore::raw_text_from(&num(42)).unwrap(), "42");
    assert_eq!(
        SheetStore::raw_text_from(&Value::String("=A:1".into())).unwrap(),
        "=A:1"
    );
    assert!(SheetStore::raw_text_from(&Value::Array(vec![num(1)])).is_err());
}
