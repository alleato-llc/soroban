//! Workbook codec round-trip edges — the Swift WorkbookTests cases the
//! Phase 2b unit tests (in workbook.rs) and the Gherkin round-trip scenario
//! don't already cover: data types, namespaces/imports, typed overloads,
//! hand-edited files, layout, value precision, and the shipped example
//! workbook computing end-to-end through a SheetStore.

use soroban_engine::workbook::{restore_session, SheetPayload};
use soroban_engine::{
    BigDecimal, Calculator, CellAddress, CellDisplay, Sheet, SheetStore, UserFunction, Value,
    Workbook,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn round_trip(workbook: &Workbook) -> Workbook {
    Workbook::decode(&workbook.encode().expect("encodes")).expect("re-decodes")
}

fn owned_functions(calculator: &Calculator) -> Vec<UserFunction> {
    calculator
        .environment()
        .all_user_functions()
        .into_iter()
        .cloned()
        .collect()
}

fn from_environment(calculator: &Calculator) -> Workbook {
    Workbook::new(
        vec![SheetPayload::new("Sheet 1", HashMap::new())],
        None,
        calculator.environment().user_variables(),
        &owned_functions(calculator),
        calculator.environment().user_data_types(),
        calculator.environment().namespace_sources().to_vec(),
        calculator.environment().imported_namespaces().to_vec(),
    )
}

#[test]
fn variables_round_trip_at_full_precision() {
    let mut calculator = Calculator::new();
    calculator.evaluate("rate = 0.0825").expect("assigns");
    calculator.evaluate("big = 1e+40").expect("assigns");
    calculator.evaluate("third = 1 / 3").expect("assigns"); // 50 significant digits
    let original = calculator.environment().user_variables().clone();

    let decoded = round_trip(&from_environment(&calculator));
    let parsed = decoded.parsed_variables();
    assert_eq!(parsed.len(), 3);
    for (name, value) in &original {
        assert_eq!(parsed.get(name), Some(value), "'{name}' lost precision");
    }
}

#[test]
fn encodes_a_versioned_pretty_envelope() {
    let workbook = Workbook::single_sheet(
        HashMap::from([("A:1".to_string(), "1".to_string())]),
        &HashMap::new(),
        &[],
        HashMap::new(),
        HashMap::new(),
    );
    let text = String::from_utf8(workbook.encode().expect("encodes")).expect("utf8");
    assert!(text.contains("soroban-workbook"), "{text}");
    assert!(text.contains("\"version\""), "{text}");
    assert!(text.contains('\n'), "pretty-printed for diffability");
    let decoded = Workbook::decode(text.as_bytes()).expect("decodes");
    assert_eq!(decoded.version, 2);
}

#[test]
fn data_types_round_trip_and_older_files_decode_empty() {
    let mut calculator = Calculator::new();
    calculator
        .evaluate("data Person { name: String, age: Number } # who")
        .expect("declares");

    let decoded = round_trip(&from_environment(&calculator));
    assert_eq!(
        decoded.data_types.get("Person").map(String::as_str),
        Some("data Person { name: String, age: Number } # who")
    );

    // Files written before data types existed decode with the default.
    let older = Workbook::decode(
        br#"{"format": "soroban-workbook", "version": 1, "cells": {}, "variables": {}}"#,
    )
    .expect("older decodes");
    assert!(older.data_types.is_empty());
}

#[test]
fn namespaces_and_imports_round_trip_and_restore() {
    let mut calculator = Calculator::new();
    calculator
        .evaluate(
            "namespace Geo { data Point { x: Number, y: Number }; dist(p: Point) = sqrt(p.x^2 + p.y^2) }",
        )
        .expect("declares");
    calculator.evaluate("import Geo").expect("imports");

    let decoded = round_trip(&from_environment(&calculator));
    // The namespace declaration persists; the qualified members do NOT leak
    // into the flat function/type maps.
    assert_eq!(decoded.namespaces.len(), 1);
    assert_eq!(decoded.imports, vec!["Geo".to_string()]);
    assert!(
        decoded.functions.is_empty(),
        "Geo::dist is a namespace member"
    );
    assert!(decoded.data_types.is_empty(), "Geo::Point too");

    // Restoring into a fresh session re-registers the members and the import.
    let mut fresh = Calculator::new();
    restore_session(&mut fresh, &decoded);
    assert_eq!(
        fresh
            .evaluate("Geo::dist(Geo::Point(x: 3, y: 4))")
            .expect("qualified call works")
            .to_string(),
        "5"
    );
    assert_eq!(
        fresh
            .evaluate("dist(Point(x: 6, y: 8))")
            .expect("import restored")
            .to_string(),
        "10"
    );
}

#[test]
fn typed_overloads_round_trip() {
    // Two operator overloads of `+`/`*` for Point survive save/reload —
    // the reason `functions` is a list, not a name→source map.
    let mut calculator = Calculator::new();
    calculator
        .evaluate("data Point { x: Number, y: Number }")
        .expect("declares");
    calculator
        .evaluate("+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)")
        .expect("defines");
    calculator
        .evaluate("*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)")
        .expect("defines");

    let decoded = round_trip(&from_environment(&calculator));
    assert_eq!(
        decoded.functions.len(),
        2,
        "both overloads persisted, not collapsed"
    );

    let mut fresh = Calculator::new();
    restore_session(&mut fresh, &decoded);
    fresh.evaluate("p = Point(x: 1, y: 2)").expect("constructs");
    fresh
        .evaluate("q = Point(x: 10, y: 20)")
        .expect("constructs");
    assert_eq!(fresh.evaluate("(p + q).x").expect("+").to_string(), "11");
    assert_eq!(fresh.evaluate("(p * 3).y").expect("*").to_string(), "6");
}

#[test]
fn hand_edited_bad_variables_are_dropped() {
    let decoded = Workbook::decode(
        br#"{"format": "soroban-workbook", "version": 1,
             "cells": {}, "variables": {"good": "1.5", "bad": "not-a-number"}}"#,
    )
    .expect("decodes");
    let parsed = decoded.parsed_variables();
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed.get("good"),
        Some(&Value::Number(BigDecimal::parse("1.5").unwrap()))
    );
}

#[test]
fn layout_round_trips() {
    let workbook = Workbook::single_sheet(
        HashMap::new(),
        &HashMap::new(),
        &[],
        HashMap::from([("A".to_string(), 150.0), ("C".to_string(), 60.0)]),
        HashMap::from([("5".to_string(), 48.0)]),
    );
    let decoded = round_trip(&workbook);
    assert_eq!(decoded.sheets[0].column_widths["A"], 150.0);
    assert_eq!(decoded.sheets[0].column_widths["C"], 60.0);
    assert_eq!(decoded.sheets[0].row_heights["5"], 48.0);
}

#[test]
fn shipped_example_workbook_opens_and_computes() {
    // examples/mortgage.soroban is documentation — keep it honest. This is
    // the full open path: decode, restore functions, load sheets into a
    // SheetStore, and check the numbers a user would see.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/mortgage.soroban"
    );
    let workbook =
        Workbook::decode(&std::fs::read(path).expect("fixture readable")).expect("fixture decodes");
    let names: Vec<String> = workbook.sheets.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names, ["Loan", "What If"]);

    // Apply the way the app does: a SheetStore + functions + cells.
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    {
        let mut calc = calculator.borrow_mut();
        let mut sources = workbook.functions.clone();
        sources.sort();
        for source in &sources {
            calc.evaluate(source).expect("workbook function restores");
        }
    }
    let mut sheets: Vec<Rc<Sheet>> = Vec::new();
    for payload in &workbook.sheets {
        let sheet = store.make_sheet(&payload.name);
        let mut contents: HashMap<CellAddress, String> = HashMap::new();
        for (key, raw) in &payload.cells {
            contents.insert(
                CellAddress::from_key(key).expect("a valid cell key"),
                raw.clone(),
            );
        }
        sheet.grid.load(&contents);
        sheets.push(sheet);
    }
    store.replace_sheets(sheets, workbook.active_sheet.as_deref());

    // $350k at 6.5% APR over 30 years → -$2,212.24/month.
    let loan = store.sheet_named("Loan").expect("Loan sheet");
    match store.display_value_on(&loan, CellAddress::new(1, 4)) {
        CellDisplay::Value(monthly) => {
            assert_eq!(monthly, BigDecimal::parse("-2212.24").unwrap())
        }
        other => panic!("expected a computed payment, got {other:?}"),
    }

    // The What If sheet reads the Loan sheet cross-sheet: +1% APR costs
    // $235.01/mo more.
    let what_if = store.sheet_named("What If").expect("What If sheet");
    match store.display_value_on(&what_if, CellAddress::new(1, 1)) {
        CellDisplay::Value(extra) => assert_eq!(extra, BigDecimal::parse("235.01").unwrap()),
        other => panic!("expected cross-sheet extra-cost value, got {other:?}"),
    }

    // The documented function carries its doc comment.
    assert!(calculator
        .borrow()
        .documentation_for("monthly")
        .expect("monthly is documented")
        .summary
        .contains("monthly loan payment"));

    // No #ERR anywhere in the example.
    for sheet in store.sheets() {
        for address in sheet.grid.raws().keys() {
            if let CellDisplay::Error(message) = store.display_value_on(&sheet, *address) {
                panic!("example cell {}!{address} errors: {message}", sheet.name());
            }
        }
    }
}
