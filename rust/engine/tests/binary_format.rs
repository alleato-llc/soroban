//! Port of the one `BinaryFormatTests.swift` case that needs the engine
//! crate: `aSavedFormatSurvivesSaveAndReopen` — the user-visible chain of
//! saving a bit-editor format as a typed workbook variable, closing, and
//! reopening. The rest of the Swift suite lives in
//! `rust/anzan/tests/binary_format.rs`.

use soroban_engine::workbook::restore_session;
use soroban_engine::{
    BinaryEditorBits, BinaryFieldSpec, BinaryView, Calculator, UserFunction, Workbook,
};

/// The user-visible chain: save a format as a typed variable, close, reopen.
#[test]
fn a_saved_format_survives_save_and_reopen() {
    let layout = vec![
        BinaryFieldSpec::new("ver", 4)
            .with_color("blue")
            .with_base(16),
        BinaryFieldSpec::new("flags", 4)
            .with_flags(&["A", "B", "C", "D"])
            .with_color("green"),
    ];
    let mut calc = Calculator::new();
    calc.evaluate(BinaryEditorBits::SCHEMA_SOURCE)
        .expect("schema evaluates");
    calc.evaluate(&format!(
        "perm = {}",
        BinaryEditorBits::format_source(&layout)
    ))
    .expect("saved format evaluates");

    let environment = calc.environment();
    let functions: Vec<UserFunction> = environment
        .all_user_functions()
        .into_iter()
        .cloned()
        .collect();
    let workbook = Workbook::new(
        Vec::new(),
        None,
        environment.user_variables(),
        &functions,
        environment.user_data_types(),
        environment.namespace_sources().to_vec(),
        environment.imported_namespaces().to_vec(),
    );
    let decoded = Workbook::decode(&workbook.encode().expect("encodes")).expect("decodes");

    let mut fresh = Calculator::new();
    restore_session(&mut fresh, &decoded);
    let restored = fresh
        .environment()
        .user_variables()
        .get("perm")
        .expect("perm restored")
        .clone();
    assert_eq!(BinaryView::layout(&restored), Some(layout));
}
