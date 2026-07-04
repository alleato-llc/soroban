//! The docs-can't-drift suite — the port of Swift's DocumentationTests:
//! every registered function must carry complete documentation, and every
//! example — built-in, special form, operator, constant — must actually
//! evaluate against a seeded sheet. Register a function without docs and the
//! registry's required fields stop you; ship a broken example and this
//! fails.

use soroban_engine::documentation::builtin_documentation;
use soroban_engine::{Calculator, CellAddress, FunctionRegistry, SheetStore};
use std::cell::RefCell;
use std::rc::Rc;

/// A calculator wired to a sheet store with seeded cells, so cell/range
/// examples (`sum(A:1..B:3)`) evaluate — the same seeding as the Swift
/// suite: A:1..A:3 = 10/20/30, B:1..B:3 = 100/200/300.
fn seeded() -> (Rc<RefCell<Calculator>>, SheetStore) {
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let store = SheetStore::new(Rc::clone(&calculator));
    let grid = Rc::clone(&store.active_sheet().grid);
    for (key, raw) in [
        ("A:1", "10"),
        ("A:2", "20"),
        ("A:3", "30"),
        ("B:1", "100"),
        ("B:2", "200"),
        ("B:3", "300"),
    ] {
        grid.set_cell(
            Some(raw),
            CellAddress::from_key(key).expect("a valid seed key"),
        );
    }
    (calculator, store)
}

#[test]
fn every_builtin_is_fully_documented() {
    for function in FunctionRegistry::standard().all() {
        assert!(
            !function.signature.is_empty(),
            "{} has no signature",
            function.name
        );
        assert!(
            function.signature.contains(function.name) || function.signature.contains('('),
            "{} signature looks wrong: {}",
            function.name,
            function.signature
        );
        assert!(
            function.summary.chars().count() > 10,
            "{} summary too thin",
            function.name
        );
        assert!(
            !function.examples.is_empty(),
            "{} has no examples",
            function.name
        );
    }
}

#[test]
fn every_example_evaluates() {
    // Examples within one entry run sequentially on a shared calculator
    // (so "1 + 1" then "ans * 2" works); entries are independent.
    let mut failures = Vec::new();
    for category in builtin_documentation() {
        for entry in &category.entries {
            let (calculator, _store) = seeded();
            for example in &entry.examples {
                if let Err(error) = calculator.borrow_mut().evaluate(example) {
                    failures.push(format!(
                        "{} example failed: '{example}' → {error}",
                        entry.name
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn categories_cover_the_whole_registry() {
    let documented: std::collections::HashSet<String> = builtin_documentation()
        .into_iter()
        .flat_map(|category| category.entries)
        .map(|entry| entry.name.to_lowercase())
        .collect();
    for function in FunctionRegistry::standard().all() {
        assert!(
            documented.contains(&function.name.to_lowercase()),
            "{} missing from documentation categories",
            function.name
        );
    }
}

#[test]
fn user_functions_document_themselves_live() {
    let mut calculator = Calculator::new();
    calculator
        .evaluate("tax(x) = x * 1.0825")
        .expect("definition evaluates");
    let doc = calculator
        .documentation_for("tax")
        .expect("the user's function is documented");
    // The definition line is the clickable example; the summary nudges
    // toward a # doc comment until one exists.
    assert_eq!(doc.examples, vec!["tax(x) = x * 1.0825".to_string()]);
    assert!(
        doc.summary.contains("trailing comment"),
        "summary should nudge toward a doc comment: {}",
        doc.summary
    );

    // A trailing # comment IS the documentation (no separate storage).
    calculator
        .evaluate("vat(x) = x * 1.2 # UK VAT")
        .expect("definition evaluates");
    let documented = calculator.documentation_for("vat").expect("documented");
    assert_eq!(documented.summary, "UK VAT");
}

#[test]
fn single_lookup_covers_all_kinds() {
    let mut calculator = Calculator::new();
    assert!(
        calculator
            .documentation_for("pmt")
            .expect("pmt is documented")
            .signature
            .contains("pmt("),
        "pmt signature should carry the call shape"
    );
    // Case-insensitive, like all function names.
    assert!(calculator.documentation_for("PMT").is_some());
    assert!(
        calculator
            .documentation_for("if")
            .expect("if is documented")
            .summary
            .contains("taken branch"),
        "if's summary should explain laziness"
    );
    assert!(calculator.documentation_for("sigma").is_some());
    calculator.evaluate("f(x) = x + 1").expect("defines");
    assert_eq!(
        calculator
            .documentation_for("f")
            .expect("user function documented")
            .signature,
        "f(x)"
    );
    assert!(calculator.documentation_for("nope").is_none());
}
