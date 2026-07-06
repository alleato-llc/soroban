//! The read-only Workbook reflection host-objects — `WorkbookObject` /
//! `WorksheetCollection` / `WorksheetObject` / `CellObject`. Object-graph
//! navigation (`Workbook.worksheets[0].cell("A", 1).value`, member access,
//! `[i]` indexing, method calls), the `.text` face of every display kind, and
//! the weak-handle discipline (a stored handle to a removed sheet — or a
//! dropped store — reads/throws cleanly). Reached by EVALUATING the accessors
//! as log lines, exactly as a user's formula would. The port of the coverage
//! Swift's `WorkbookReflection.swift` has.

use soroban_engine::{BigDecimal, Calculator, CellAddress, EvalOutcome, Sheet, SheetStore, Value};
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

fn set(sheet: &Rc<Sheet>, key: &str, raw: &str) {
    sheet.grid.set_cell(Some(raw), addr(key));
}

fn log(calculator: &Rc<RefCell<Calculator>>, line: &str) -> EvalOutcome {
    calculator
        .borrow_mut()
        .evaluate(line)
        .unwrap_or_else(|e| panic!("'{line}' failed: {e}"))
}

fn value(calculator: &Rc<RefCell<Calculator>>, line: &str) -> Value {
    match log(calculator, line) {
        EvalOutcome::Value(value) => value,
        other => panic!("'{line}' produced a non-value outcome: {other:?}"),
    }
}

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

fn string(text: &str) -> Value {
    Value::String(text.to_string())
}

// MARK: WorkbookObject — the root handle

#[test]
fn workbook_members_and_description() {
    let (calc, store) = make_store();
    assert_eq!(value(&calc, "Workbook.count"), num(1));
    // Singular in the description with one sheet.
    match value(&calc, "Workbook") {
        Value::Host(object) => assert_eq!(object.description(), "Workbook(1 sheet)"),
        other => panic!("Workbook is a host handle, got {other:?}"),
    }
    // Both `worksheets` and its `sheets` alias reach the collection.
    assert_eq!(value(&calc, "Workbook.worksheets.count"), num(1));
    assert_eq!(value(&calc, "Workbook.sheets.count"), num(1));
    // A quick array of names, and the active sheet.
    assert_eq!(value(&calc, "len(Workbook.sheetNames)"), num(1));
    assert_eq!(value(&calc, "Workbook.sheetNames[0]"), string("Sheet 1"));
    assert_eq!(value(&calc, "Workbook.activeSheet.name"), string("Sheet 1"));

    store.add_sheet().expect("adds");
    assert_eq!(value(&calc, "Workbook.count"), num(2));
    // Plural in the description now.
    match value(&calc, "Workbook") {
        Value::Host(object) => assert_eq!(object.description(), "Workbook(2 sheets)"),
        other => panic!("Workbook is a host handle, got {other:?}"),
    }

    // Unknown member.
    assert!(error(&calc, "Workbook.bogus").contains("Workbook has no member '.bogus'"));
}

// MARK: WorksheetCollection — indexing by position / name

#[test]
fn worksheet_collection_indexing() {
    let (calc, store) = make_store();
    store.add_sheet().expect("adds"); // Sheet 2
    store.add_sheet().expect("adds"); // Sheet 3

    // By position, and the collection's own description/count.
    assert_eq!(
        value(&calc, "Workbook.worksheets[0].name"),
        string("Sheet 1")
    );
    assert_eq!(value(&calc, "Workbook.worksheets.count"), num(3));
    match value(&calc, "Workbook.worksheets") {
        Value::Host(object) => assert_eq!(object.description(), "Worksheets(3)"),
        other => panic!("worksheets is a host handle, got {other:?}"),
    }
    // Negative indices count from the end.
    assert_eq!(
        value(&calc, "Workbook.worksheets[-1].name"),
        string("Sheet 3")
    );
    // By name.
    assert_eq!(
        value(&calc, "Workbook.worksheets[\"Sheet 2\"].name"),
        string("Sheet 2")
    );

    // Out-of-range position, unknown name, and a non-position key all read as
    // "not indexable" (the index() → None branches).
    assert!(error(&calc, "Workbook.worksheets[9]").contains("can't be indexed"));
    assert!(error(&calc, "Workbook.worksheets[\"Nope\"]").contains("can't be indexed"));
    assert!(error(&calc, "Workbook.worksheets[[0]]").contains("can't be indexed"));
    // Unknown member on the collection.
    assert!(error(&calc, "Workbook.worksheets.bogus").contains("Worksheets has no member"));
}

// MARK: WorksheetObject — members, methods, equality

#[test]
fn worksheet_members_methods_and_equality() {
    let (calc, store) = make_store();
    assert_eq!(
        value(&calc, "Workbook.worksheets[0].name"),
        string("Sheet 1")
    );
    assert_eq!(value(&calc, "Workbook.worksheets[0].rowCount"), num(1000));
    assert_eq!(value(&calc, "Workbook.worksheets[0].columnCount"), num(26));
    assert_eq!(value(&calc, "Workbook.worksheets[0].isData"), num(0)); // false
    match value(&calc, "Workbook.worksheets[0]") {
        Value::Host(object) => assert_eq!(object.description(), "Worksheet(Sheet 1)"),
        other => panic!("worksheet is a host handle, got {other:?}"),
    }

    // Identity equality — same live sheet is equal, different sheets aren't.
    store.add_sheet().expect("adds");
    assert_eq!(
        value(&calc, "Workbook.worksheets[0] == Workbook.worksheets[0]"),
        num(1)
    );
    assert_eq!(
        value(&calc, "Workbook.worksheets[0] == Workbook.worksheets[1]"),
        num(0)
    );

    // Unknown member and unknown method.
    assert!(error(&calc, "Workbook.worksheets[0].bogus").contains("Worksheet has no member"));
    assert!(
        error(&calc, "Workbook.worksheets[0].bogus()").contains("Worksheet has no method 'bogus'")
    );
}

// MARK: CellObject — construction, the object graph, members, equality

#[test]
fn cell_object_graph_navigation_and_members() {
    let (calc, store) = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "5");
    set(&sheet, "A:11", "=A:1 + 1");

    // The full graph: Workbook → worksheets → cell → value.
    assert_eq!(
        value(&calc, "Workbook.worksheets[0].cell(\"A\", 1).value"),
        num(5)
    );
    // The flat cell() accessor reaches the same handle on the scope sheet.
    assert_eq!(value(&calc, "cell(\"A\", 1).value"), num(6 - 1));
    assert_eq!(value(&calc, "cell(\"A\", 11).raw"), string("=A:1 + 1"));
    assert_eq!(value(&calc, "cell(\"A\", 11).formula"), string("=A:1 + 1"));
    assert_eq!(value(&calc, "cell(\"A\", 1).address"), string("A:1"));
    assert_eq!(value(&calc, "cell(\"Z\", 99).isEmpty"), num(1)); // true
    assert_eq!(value(&calc, "cell(\"A\", 1).isEmpty"), num(0)); // false

    // A cell's description and identity equality.
    match value(&calc, "cell(\"B\", 3)") {
        Value::Host(object) => assert_eq!(object.description(), "Cell(B:3)"),
        other => panic!("cell is a host handle, got {other:?}"),
    }
    assert_eq!(value(&calc, "cell(\"A\", 1) == cell(\"A\", 1)"), num(1));
    assert_eq!(value(&calc, "cell(\"A\", 1) == cell(\"A\", 2)"), num(0));

    // Unknown member.
    assert!(error(&calc, "cell(\"A\", 1).bogus").contains("Cell has no member '.bogus'"));
}

#[test]
fn cell_value_falls_back_to_text_for_non_numbers() {
    // `.value` routes through numeric_value; a label cell can't be a number,
    // so it reads as its placeholder text (member() can't throw).
    let (calc, store) = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "Hello"); // an unknown-variable label → text
    assert_eq!(value(&calc, "cell(\"A\", 1).value"), string("Hello"));
    assert_eq!(value(&calc, "cell(\"A\", 1).text"), string("Hello"));
}

#[test]
fn cell_text_renders_every_display_kind() {
    // CellObject::text covers every CellDisplay variant.
    let (calc, store) = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "42"); // value
    set(&sheet, "A:2", "Label"); // text
    set(&sheet, "A:3", "# a note"); // note
    set(&sheet, "A:4", "=1 / 0"); // error
    set(&sheet, "A:5", "rate = 5"); // 𝑖 definition
    set(&sheet, "A:6", "slider(5, 0, 10)"); // slider
    set(&sheet, "A:7", "stepper(3, 0, 10)"); // stepper
    set(&sheet, "A:8", "checkbox(true)"); // checkbox
    set(&sheet, "A:9", "dropdown(\"x\", [\"x\", \"y\"])"); // dropdown

    assert_eq!(value(&calc, "cell(\"A\", 1).text"), string("42"));
    assert_eq!(value(&calc, "cell(\"A\", 2).text"), string("Label"));
    assert_eq!(value(&calc, "cell(\"A\", 3).text"), string("# a note"));
    assert!(matches!(value(&calc, "cell(\"A\", 4).text"), Value::String(m) if !m.is_empty()));
    assert_eq!(value(&calc, "cell(\"A\", 5).text"), string("𝑖 rate"));
    assert_eq!(value(&calc, "cell(\"A\", 6).text"), string("5"));
    assert_eq!(value(&calc, "cell(\"A\", 7).text"), string("3"));
    assert_eq!(value(&calc, "cell(\"A\", 8).text"), string("true"));
    assert_eq!(value(&calc, "cell(\"A\", 9).text"), string("x"));
    // The empty cell.
    assert_eq!(value(&calc, "cell(\"Z\", 50).text"), string(""));
}

#[test]
fn cell_make_argument_errors() {
    // The method form `.cell(...)` reaches CellObject::make directly, so its
    // argument validation branches are exercised here.
    let (calc, _store) = make_store();
    assert!(error(&calc, "Workbook.worksheets[0].cell(\"A\")")
        .contains("cell() takes a column and a row"));
    assert!(error(&calc, "Workbook.worksheets[0].cell(5, 1)")
        .contains("first argument is a column letter"));
    assert!(error(&calc, "Workbook.worksheets[0].cell(\"A\", \"x\")")
        .contains("second argument is a row number"));
    assert!(error(&calc, "Workbook.worksheets[0].cell(\"A\", 9999)").contains("is out of range"));
}

// MARK: Weak handles — reads after the store is gone throw cleanly

#[test]
fn handles_to_a_dropped_store_read_cleanly() {
    // A stored handle holds the store WEAKLY; dropping the store makes every
    // read degrade to "no member"/"not indexable" rather than dangle.
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    log(&calculator, "wb = Workbook");
    log(&calculator, "wc = Workbook.worksheets");
    drop(store);

    // WorkbookObject / WorksheetCollection upgrade to None → members vanish,
    // indexing fails.
    assert!(error(&calculator, "wb.count").contains("Workbook has no member"));
    assert!(error(&calculator, "wb.worksheets").contains("Workbook has no member"));
    assert!(error(&calculator, "wc.count").contains("Worksheets has no member"));
    assert!(error(&calculator, "wc[0]").contains("can't be indexed"));
}

#[test]
fn a_stale_worksheet_handle_describes_as_gone() {
    // WorksheetObject.description with a dead weak → "Worksheet(—)".
    let (calc, store) = make_store();
    store.add_sheet().expect("adds"); // Sheet 2
    log(&calc, "w = Workbook.worksheets[1]");
    store.remove_sheet(1).expect("removes Sheet 2");
    match value(&calc, "w") {
        Value::Host(object) => assert_eq!(object.description(), "Worksheet(—)"),
        other => panic!("w is a host handle, got {other:?}"),
    }
    // A method call on the dead worksheet throws cleanly.
    assert!(error(&calc, "w.cell(\"A\", 1)").contains("no longer available"));
}
