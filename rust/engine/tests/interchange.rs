//! Cross-ecosystem `.soroban` interchange — the Rust half. `examples/
//! interchange.soroban` is **Rust-authored** (regenerate with `cargo run -p
//! soroban-engine --example author_interchange`) and is opened + computed by
//! BOTH ecosystems: here, and by Swift's `InterchangeTests`. Its mirror,
//! `examples/mortgage.soroban`, is Swift-authored and read by both suites too —
//! so a workbook written by either side is proven to compute on the other.
//!
//! This one exercises what mortgage doesn't: a log variable, a user function, a
//! `data` type + a record instance, a named cell, and a saved bit-format
//! variable.

use soroban_engine::workbook::restore_session;
use soroban_engine::{Calculator, CellAddress, CellDisplay, Sheet, SheetStore, Workbook};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn addr(key: &str) -> CellAddress {
    CellAddress::from_key(key).expect("valid cell key")
}

/// The computed value at `key`, as its canonical string.
fn value(store: &SheetStore, key: &str) -> String {
    match store.display_value(addr(key)) {
        CellDisplay::Value(v) => v.to_string(),
        other => panic!("{key}: expected a value, got {other:?}"),
    }
}

#[test]
fn rust_opens_the_rust_authored_interchange_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/interchange.soroban"
    );
    let workbook =
        Workbook::decode(&std::fs::read(path).expect("fixture readable")).expect("fixture decodes");
    assert_eq!(workbook.sheets.iter().map(|s| &s.name).collect::<Vec<_>>(), ["Sheet 1"]);

    // Restore the way the app does — env first (types → functions → variables),
    // then the sheets into a store wired to the same calculator.
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    restore_session(&mut calculator.borrow_mut(), &workbook);

    let mut sheets: Vec<Rc<Sheet>> = Vec::new();
    for payload in &workbook.sheets {
        let sheet = store.make_sheet(&payload.name);
        let mut contents: HashMap<CellAddress, String> = HashMap::new();
        for (key, raw) in &payload.cells {
            contents.insert(addr(key), raw.clone());
        }
        sheet.grid.load(&contents);
        for (key, name) in &payload.names {
            sheet
                .grid
                .set_cell_name(Some(name), addr(key))
                .expect("name restores");
        }
        sheets.push(sheet);
    }
    store.replace_sheets(sheets, workbook.active_sheet.as_deref());

    // Cell values a user would see.
    assert_eq!(value(&store, "A:2"), "2400"); // =A:1 * 2
    assert_eq!(value(&store, "B:1"), "42"); // =double(21) — user function
    assert_eq!(value(&store, "B:2"), "8.25"); // =100 * taxRate — log variable
    assert_eq!(value(&store, "C:1"), "1201"); // ='Base' + 1 — named cell

    // Env restored: the data-type record and the saved bit-format variable.
    // Scoped so the borrow drops before display_value borrows again below.
    {
        let calc = calculator.borrow();
        let vars = calc.environment().user_variables();
        assert_eq!(
            vars.get("origin").map(|v| v.to_string()).as_deref(),
            Some("Point(x: 3, y: 4)")
        );
        assert!(vars.contains_key("myfmt"), "saved bit-format restored");
    }

    // Nothing decoded to an error.
    for payload in &workbook.sheets {
        for key in payload.cells.keys() {
            assert!(
                !matches!(store.display_value(addr(key)), CellDisplay::Error(_)),
                "{key} should not be an error"
            );
        }
    }
}
