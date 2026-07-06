//! Tests for the workbook JSON envelope codec.

use super::*;
use anzan::EvalOutcome;

fn decode_fixture() -> Workbook {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/mortgage.soroban"
    );
    let data = std::fs::read(path).expect("fixture readable");
    Workbook::decode(&data).expect("fixture decodes")
}

/// The Rust half of the interchange proof: a REAL Swift-written workbook
/// decodes, sheets and all.
#[test]
fn decodes_swift_written_fixture() {
    let workbook = decode_fixture();
    let names: Vec<&str> = workbook.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Loan", "What If"]);
    assert_eq!(workbook.version, 1);
    assert_eq!(workbook.active_sheet.as_deref(), Some("Loan"));
    // v1 legacy name→source functions map flattens to sorted source lines.
    assert_eq!(workbook.functions.len(), 1);
    assert!(workbook.functions[0].starts_with("monthly(apr, years, principal)"));
    assert_eq!(workbook.sheets[0].cells["B:1"], "350000");
    assert_eq!(workbook.sheets[0].column_widths["A"], 150.0);
    assert!(workbook.sheets[1].row_heights.is_empty());
}

#[test]
fn fixture_restores_into_a_calculator() {
    let workbook = decode_fixture();
    let mut calculator = Calculator::new();
    restore_session(&mut calculator, &workbook);
    // The workbook's function is live again (self-contained file).
    match calculator.evaluate("monthly(0.065, 30, 350000)") {
        Ok(EvalOutcome::Value(_)) => {}
        other => panic!("expected a value, got {other:?}"),
    }
}

#[test]
fn round_trips_through_encode() {
    let workbook = decode_fixture();
    let encoded = workbook.encode().expect("encodes");
    let again = Workbook::decode(&encoded).expect("re-decodes");
    assert_eq!(workbook, again);
}

#[test]
fn rejects_future_versions_and_foreign_files() {
    let future = br#"{"format":"soroban-workbook","version":99,"variables":{}}"#;
    assert_eq!(
        Workbook::decode(future),
        Err(WorkbookError::UnsupportedVersion(99))
    );
    let foreign = br#"{"format":"something-else","version":1,"variables":{}}"#;
    assert_eq!(Workbook::decode(foreign), Err(WorkbookError::NotAWorkbook));
    assert_eq!(
        Workbook::decode(b"not json"),
        Err(WorkbookError::NotAWorkbook)
    );
}

#[test]
fn legacy_flat_single_sheet_still_opens() {
    let flat = br#"{
        "format": "soroban-workbook", "version": 1,
        "cells": {"A:1": "42"}, "columnWidths": {"A": 120},
        "variables": {"x": "1.5"}
    }"#;
    let workbook = Workbook::decode(flat).expect("legacy decodes");
    assert_eq!(workbook.sheets.len(), 1);
    assert_eq!(workbook.sheets[0].name, "Sheet 1");
    assert_eq!(workbook.sheets[0].cells["A:1"], "42");
    assert_eq!(workbook.sheets[0].column_widths["A"], 120.0);
    let parsed = workbook.parsed_variables();
    assert_eq!(parsed["x"].to_string(), "1.5");
}

#[test]
fn missing_variables_is_not_a_workbook() {
    // Swift's decode requires `variables`; keep the strictness identical.
    let no_vars = br#"{"format":"soroban-workbook","version":1}"#;
    assert_eq!(Workbook::decode(no_vars), Err(WorkbookError::NotAWorkbook));
}

#[test]
fn formats_round_trip_as_typed_cell_formats() {
    use crate::cell_format::{CellFormat, NumberFormat};
    // A Swift-shaped `formats` object decodes into a typed CellFormat…
    let with_formats = br#"{
        "format": "soroban-workbook", "version": 2, "variables": {},
        "sheets": [{"name": "S", "cells": {},
                    "formats": {"A:1": {"bold": true, "style": "currency",
                                        "decimals": 2, "symbol": "$"}}}]
    }"#;
    let workbook = Workbook::decode(with_formats).expect("decodes");
    let format = &workbook.sheets[0].formats["A:1"];
    assert!(format.bold);
    assert_eq!(
        format.number_format,
        NumberFormat::Currency {
            symbol: "$".into(),
            decimals: 2
        }
    );
    // …and re-encoding it is lossless (Rust can now *originate* formats).
    let encoded = workbook.encode().expect("encodes");
    let again = Workbook::decode(&encoded).expect("re-decodes");
    assert_eq!(workbook, again);

    // The compact codec omits default fields and matches Swift's shape.
    let plain_bold = CellFormat {
        bold: true,
        ..CellFormat::default()
    };
    assert_eq!(
        serde_json::to_value(&plain_bold).unwrap(),
        serde_json::json!({ "bold": true })
    );
}

#[test]
fn encode_omits_empty_namespaces_and_imports() {
    let workbook = Workbook::single_sheet(
        HashMap::new(),
        &HashMap::new(),
        &[],
        HashMap::new(),
        HashMap::new(),
    );
    let text = String::from_utf8(workbook.encode().unwrap()).unwrap();
    assert!(!text.contains("\"namespaces\""));
    assert!(!text.contains("\"imports\""));
    assert!(!text.contains("\"activeSheet\""));
    assert!(text.contains("\"dataTypes\""));
}
