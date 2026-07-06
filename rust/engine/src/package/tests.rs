//! Tests for the `.soroban` document package codec.

use super::*;
use crate::workbook::SheetPayload;
use anzan::Value;
use num_bigint::BigInt;
use std::collections::HashMap;

fn sample() -> Workbook {
    let variables = HashMap::from([(
        "rate".to_string(),
        Value::Number(anzan::BigDecimal::new(BigInt::from(1), -1)), // 0.1
    )]);
    Workbook::new(
        vec![SheetPayload::new(
            "Sheet 1",
            HashMap::from([("A:1".into(), "42".into())]),
        )],
        None,
        &variables,
        &[],
        &HashMap::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("soroban-tests-{label}-{}", unique_suffix()));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Port of `WorkbookPackageTests.packageRoundTrip`.
#[test]
fn package_round_trip() {
    let dir = temp_dir("round-trip");
    let path = dir.join("model.soroban");

    write(&sample(), &path, None).expect("writes");

    assert!(path.is_dir()); // it's a package, not a flat file
    assert_eq!(database_path(&path), None); // no data sheets

    let read_back = read(&path).expect("reads");
    assert_eq!(read_back.sheets[0].cells["A:1"], "42");

    // Overwriting an existing package is atomic-replace, not append.
    let mut updated = sample();
    updated.sheets[0].cells.insert("A:2".into(), "7".into());
    write(&updated, &path, None).expect("rewrites");
    assert_eq!(read(&path).expect("re-reads").sheets[0].cells["A:2"], "7");

    fs::remove_dir_all(&dir).ok();
}

/// Port of `WorkbookPackageTests.legacyFlatFilesStillRead` — the real
/// Swift-written flat fixture reads via the legacy path, and saving over
/// a flat file upgrades it to a package in place.
#[test]
fn legacy_flat_files_still_read() {
    // The repo fixture: a flat JSON `.soroban` written by the Swift app.
    let fixture = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/mortgage.soroban"
    ));
    let workbook = read(fixture).expect("fixture reads via the legacy path");
    assert_eq!(workbook.sheets[0].cells["B:1"], "350000");

    // A locally written flat file behaves the same, then upgrades.
    let dir = temp_dir("legacy");
    let path = dir.join("legacy.soroban");
    fs::write(&path, sample().encode().unwrap()).unwrap(); // old-style single JSON file
    let read_back = read(&path).expect("flat file reads");
    assert_eq!(read_back.sheets[0].cells["A:1"], "42");

    // Saving over a flat file upgrades it to a package in place.
    write(&read_back, &path, None).expect("upgrade write");
    assert!(path.is_dir());
    assert_eq!(
        read(&path).expect("package reads").sheets[0].cells["A:1"],
        "42"
    );

    fs::remove_dir_all(&dir).ok();
}

/// Port of `WorkbookPackageTests.packageCarriesTheDatabase`.
#[test]
fn package_carries_the_database() {
    let dir = temp_dir("with-db");
    let fake_db = dir.join("working.sqlite");
    fs::write(&fake_db, b"not really sqlite, just bytes").unwrap();
    let path = dir.join("with-data.soroban");

    write(&sample(), &path, Some(&fake_db)).expect("writes");
    let inside = database_path(&path).expect("database travels with the package");
    assert_eq!(fs::read(&inside).unwrap(), fs::read(&fake_db).unwrap());

    fs::remove_dir_all(&dir).ok();
}

/// Port of `WorkbookPackageTests.emptyDirectoryIsNotAWorkbook`.
#[test]
fn empty_directory_is_not_a_workbook() {
    let dir = temp_dir("hollow");
    let path = dir.join("hollow.soroban");
    fs::create_dir_all(&path).unwrap();
    match read(&path) {
        Err(PackageError::MissingManifest) => {}
        other => panic!("expected MissingManifest, got {other:?}"),
    }
    assert_eq!(
        PackageError::MissingManifest.to_string(),
        "the package has no workbook.json"
    );
    fs::remove_dir_all(&dir).ok();
}
