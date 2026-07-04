//! Dependency-graph invalidation, resolver wiring, and worksheet structure
//! rules — the ports of Swift's SpreadsheetTests/SheetStoreTests cases the
//! shared Gherkin scenarios don't cover (the features prove same-sheet
//! cycles and cross-sheet reads; targeted invalidation, cross-sheet cycle
//! detection, ans protection, and the store's structural contracts live
//! here).

use soroban_engine::{
    BigDecimal, Calculator, CellAddress, CellDisplay, EvalOutcome, Sheet, SheetStore, Value,
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

fn set(sheet: &Rc<Sheet>, key: &str, raw: &str) {
    sheet.grid.set_cell(Some(raw), addr(key));
}

fn shown(store: &SheetStore, sheet: &Rc<Sheet>, key: &str) -> CellDisplay {
    store.display_value_on(sheet, addr(key))
}

fn value(n: i64) -> CellDisplay {
    CellDisplay::Value(BigDecimal::from_int(n))
}

fn log(calculator: &Rc<RefCell<Calculator>>, line: &str) -> EvalOutcome {
    calculator
        .borrow_mut()
        .evaluate(line)
        .unwrap_or_else(|e| panic!("'{line}' failed: {e}"))
}

// MARK: Dependency graph

#[test]
fn edits_invalidate_dependents_across_sheets() {
    // The dependency graph at work: an edit reaches only its readers —
    // including readers on OTHER sheets (this was a staleness bug when
    // recalc was per-sheet memo clearing).
    let (_calc, store) = make_store();
    store.add_sheet().expect("adds");
    let sheets = store.sheets();
    let (s1, s2) = (&sheets[0], &sheets[1]);

    set(s1, "A:1", "10");
    set(s1, "A:2", "A:1 * 2"); // same-sheet reader
    set(s2, "A:1", "'Sheet 1'!A:1 + 5"); // cross-sheet reader
    set(s2, "A:2", "sum('Sheet 1'!A:1..A:5)"); // cross-sheet range reader

    // Evaluate everything once so the graph is recorded.
    assert_eq!(shown(&store, s1, "A:2"), value(20));
    assert_eq!(shown(&store, s2, "A:1"), value(15));
    assert_eq!(shown(&store, s2, "A:2"), value(30)); // 10 + A:2(20)

    // Edit the source: every reader updates, with no full recalc.
    set(s1, "A:1", "100");
    assert_eq!(shown(&store, s1, "A:2"), value(200));
    assert_eq!(shown(&store, s2, "A:1"), value(105));
    assert_eq!(shown(&store, s2, "A:2"), value(300)); // 100 + 200

    // A NEW cell inside an already-recorded range is picked up too.
    set(s1, "A:3", "1"); // inside A:1..A:5
    assert_eq!(shown(&store, s2, "A:2"), value(301));
}

#[test]
fn dependency_chains_invalidate_transitively() {
    let (_calc, store) = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "1");
    set(&sheet, "A:2", "A:1 + 1");
    set(&sheet, "A:3", "A:2 + 1");
    assert_eq!(shown(&store, &sheet, "A:3"), value(3));

    set(&sheet, "A:1", "10");
    assert_eq!(shown(&store, &sheet, "A:3"), value(12));
    assert_eq!(shown(&store, &sheet, "A:2"), value(11));
}

#[test]
fn cross_sheet_cycles_are_caught() {
    // Cycle detection is context-wide, keyed by (sheet, address) — per-sheet
    // detection would hang on this loop.
    let (_calc, store) = make_store();
    store.add_sheet().expect("adds"); // "Sheet 2"
    let sheets = store.sheets();
    set(&sheets[0], "A:1", "'Sheet 2'!A:1 + 1");
    set(&sheets[1], "A:1", "'Sheet 1'!A:1 + 1");

    match shown(&store, &sheets[0], "A:1") {
        CellDisplay::Error(message) => {
            assert!(message.contains("circular reference"), "{message}");
            assert!(
                message.contains('!'),
                "the report names the sheet: {message}"
            );
        }
        other => panic!("expected a circular-reference error, not a hang: {other:?}"),
    }
}

// MARK: Resolver wiring (log ↔ sheet)

#[test]
fn cell_evaluation_never_clobbers_ans() {
    let (calc, store) = make_store();
    log(&calc, "rate = 0.1"); // log defines a variable; ans = 0.1
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "100");
    set(&sheet, "A:2", "A:1 * rate");
    store.recalculate();

    assert_eq!(
        shown(&store, &sheet, "A:2"),
        CellDisplay::Value(BigDecimal::parse("10").unwrap())
    );
    // Cell evaluation must not have clobbered ans.
    assert_eq!(
        calc.borrow().environment().ans(),
        Value::Number(BigDecimal::parse("0.1").unwrap())
    );
}

#[test]
fn store_wide_recalc_picks_up_variables() {
    let (calc, store) = make_store();
    store.add_sheet().expect("adds");
    let sheets = store.sheets();
    set(&sheets[1], "A:1", "100 * rate");
    log(&calc, "rate = 0.1");
    store.recalculate();
    assert_eq!(shown(&store, &sheets[1], "A:1"), value(10));

    log(&calc, "rate = 0.2");
    store.recalculate();
    assert_eq!(shown(&store, &sheets[1], "A:1"), value(20));
}

#[test]
fn unqualified_refs_belong_to_the_owning_sheet() {
    // The crux: a formula on Sheet 2 reads ITS OWN A:1, even while Sheet 1
    // is active and triggers the evaluation.
    let (calc, store) = make_store();
    store.add_sheet().expect("adds");
    let sheets = store.sheets();
    set(&sheets[0], "A:1", "111");
    set(&sheets[1], "A:1", "222");
    set(&sheets[1], "A:2", "A:1 * 2"); // on Sheet 2

    store.set_active_index(0); // user is looking at Sheet 1
    assert_eq!(shown(&store, &sheets[1], "A:2"), value(444));

    // From the log, unqualified refs follow the ACTIVE sheet.
    assert_eq!(
        log(&calc, "A:1"),
        EvalOutcome::Value(Value::Number(BigDecimal::from_int(111)))
    );
    store.set_active_index(1);
    assert_eq!(
        log(&calc, "A:1"),
        EvalOutcome::Value(Value::Number(BigDecimal::from_int(222)))
    );
}

#[test]
fn unknown_sheet_is_a_clean_error() {
    let (calc, store) = make_store();
    let error = calc
        .borrow_mut()
        .evaluate("Nope!A:1")
        .expect_err("unknown-sheet failure");
    assert!(
        error.to_string().contains("unknown sheet 'Nope'"),
        "{error}"
    );
    drop(store);
}

#[test]
fn out_of_range_rows_error_and_blank_clears() {
    let (_calc, store) = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "42");
    set(&sheet, "A:1", "  "); // blank clears
    assert_eq!(shown(&store, &sheet, "A:1"), CellDisplay::Empty);
    assert!(sheet.grid.raws().is_empty());

    set(&sheet, "A:2", "A:1001 + 1"); // row out of range (max 1000)
    match shown(&store, &sheet, "A:2") {
        CellDisplay::Error(message) => assert!(message.contains("out of range"), "{message}"),
        other => panic!("expected an out-of-range error: {other:?}"),
    }
}

// MARK: Worksheet structure rules

#[test]
fn structure_rules() {
    let (_calc, store) = make_store();
    // Can't remove the last sheet.
    assert!(store.remove_sheet(0).is_err());

    // Names: validation.
    store.add_sheet().expect("adds");
    assert!(store.rename(1, "   ").is_err());
    assert!(store.rename(1, "Bad!Name").is_err());
    assert!(store.rename(1, "Bad'Name").is_err());
    assert!(store.rename(1, "sheet 1").is_err()); // dup, case-insensitive
    assert!(store.rename(1, &"x".repeat(129)).is_err());
    store
        .rename(1, &"x".repeat(128))
        .expect("exactly the cap is fine");

    // Auto-naming skips taken names.
    store.rename(1, "Sheet 3").expect("renames");
    let added = store.add_sheet().expect("adds");
    assert_eq!(added.name(), "Sheet 4");

    // Removal clamps the active index.
    store.set_active_index(2);
    store.remove_sheet(2).expect("removes");
    assert_eq!(store.active_sheet().name(), "Sheet 3");
}

#[test]
fn sheet_cap_is_256() {
    let (_calc, store) = make_store();
    for _ in 1..SheetStore::MAX_SHEETS {
        store.add_sheet().expect("under the cap");
    }
    assert_eq!(store.sheets().len(), 256);
    assert!(store.add_sheet().is_err());
}

// MARK: CellAddress conversions (the one centralized home)

#[test]
fn address_formatting_and_parsing() {
    assert_eq!(CellAddress::new(0, 0).to_string(), "A:1");
    assert_eq!(CellAddress::new(25, 99).to_string(), "Z:100");

    assert_eq!(CellAddress::from_key("A:1"), Some(CellAddress::new(0, 0)));
    // Case-insensitive.
    assert_eq!(
        CellAddress::from_key("z:1000"),
        Some(CellAddress::new(25, 999))
    );
    assert_eq!(CellAddress::from_key("A:0"), None);
    assert_eq!(CellAddress::from_key("A:1001"), None);
    assert_eq!(CellAddress::from_key("AA:1"), None);
    assert_eq!(CellAddress::from_key("A1"), None);
    assert_eq!(
        CellAddress::from_column_name("B", 3),
        Some(CellAddress::new(1, 2))
    );
    assert_eq!(CellAddress::column_index("a"), Some(0));
    assert_eq!(CellAddress::column_name_for(25), "Z");
}
