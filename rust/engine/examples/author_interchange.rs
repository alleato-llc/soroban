//! Regenerates `examples/interchange.soroban` — a Rust-authored `.soroban`
//! whose mirror image, `examples/mortgage.soroban`, is Swift-authored. Both are
//! opened and computed by BOTH ecosystems' test suites (see
//! `rust/engine/tests/interchange.rs` and Swift's `InterchangeTests`), so the
//! `.soroban` interchange format is proven to round-trip in both directions.
//!
//! Run after a format change (`Workbook::CURRENT_VERSION` bump) to refresh the
//! fixture:  `cargo run -p soroban-engine --example author_interchange`
//!
//! The file is flat JSON (like mortgage) — pretty-printed with sorted keys, so
//! it diffs cleanly and is byte-identical to what Swift would write.

use soroban_engine::workbook::{SheetPayload, Workbook};
use soroban_engine::{Calculator, UserFunction};
use std::collections::HashMap;

fn main() {
    let mut calc = Calculator::new();
    // Log-defined names: a variable, a user function, a data type + a record
    // instance of it, and a saved bit-format (a layout-shaped map variable).
    for line in [
        "taxRate = 0.0825",
        "double(x) = x * 2",
        "data Point { x: Number, y: Number }",
        "origin = Point(x: 3, y: 4)",
        "myfmt = {hi: 4, lo: 4}",
    ] {
        calc.evaluate(line)
            .unwrap_or_else(|e| panic!("setup `{line}`: {e:?}"));
    }

    // One sheet: a value, a cross-cell formula, a call to the user function, a
    // use of the log variable, and a named-cell reference.
    let cells: HashMap<String, String> = [
        ("A:1", "1200"),
        ("A:2", "=A:1 * 2"),
        ("B:1", "=double(21)"),
        ("B:2", "=100 * taxRate"),
        ("C:1", "='Base' + 1"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mut sheet = SheetPayload::new("Sheet 1", cells);
    sheet.names.insert("A:1".to_string(), "Base".to_string());

    let environment = calc.environment();
    let functions: Vec<UserFunction> = environment
        .all_user_functions()
        .into_iter()
        .cloned()
        .collect();
    let workbook = Workbook::new(
        vec![sheet],
        Some("Sheet 1".to_string()),
        environment.user_variables(),
        &functions,
        environment.user_data_types(),
        environment.namespace_sources().to_vec(),
        environment.imported_namespaces().to_vec(),
    );

    // examples/ is two levels up from rust/engine.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/interchange.soroban"
    );
    std::fs::write(path, workbook.encode().expect("encode")).expect("write fixture");
    println!("wrote {path}");
}
