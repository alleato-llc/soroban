//! Tests for the SQLite-backed data store / data sheets.

use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

fn make_store() -> (Rc<DataStore>, PathBuf) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "soroban-data-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let url = dir.join("data.sqlite");
    (Rc::new(DataStore::new(&url).expect("store opens")), url)
}

fn table(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// Port of `DataStoreTests.createReadDropPersist`.
#[test]
fn create_read_drop_persist() {
    let (store, url) = make_store();

    store
        .create_table(
            "sales",
            &table(&[
                &["month", "amount"],
                &["jan", "1200.50"],
                &["feb", "", "stray"], // ragged row, empty cell skipped
            ]),
        )
        .expect("creates");

    let info = store.info("sales").expect("has info");
    assert_eq!(info.rows, 3);
    assert_eq!(info.columns, 3); // widest row wins
    assert_eq!(store.value("sales", 1, 1).as_deref(), Some("1200.50"));
    assert_eq!(store.value("sales", 2, 1), None); // empty skipped
    assert_eq!(store.value("SALES", 0, 0).as_deref(), Some("month")); // NOCASE

    // Reopen from disk — it's real persistence, not memory.
    let reopened = DataStore::new(&url).expect("reopens");
    assert_eq!(reopened.value("sales", 1, 0).as_deref(), Some("jan"));
    drop(reopened);

    store.drop_table("sales").expect("drops");
    assert_eq!(store.info("sales"), None);
    assert_eq!(store.value("sales", 0, 0), None);

    fs::remove_dir_all(url.parent().unwrap()).ok();
}

/// A byte copy of the main `.sqlite` file (what the package writer does)
/// only captures committed rows after a `checkpoint()` flushes the WAL —
/// without it, a freshly imported table is lost when saved into a package.
#[test]
fn checkpoint_flushes_wal_into_the_main_file() {
    let (store, url) = make_store();
    store
        .create_table("t", &table(&[&["x"], &["10"], &["20"]]))
        .expect("creates");
    store.checkpoint().expect("checkpoints");

    // Byte-copy ONLY the main db file — no -wal — into a sibling package.
    let copy = url.parent().unwrap().join("data.sqlite.copy");
    fs::copy(&url, &copy).expect("copies the main file");
    let reopened = DataStore::new(&copy).expect("reopens the copy");
    assert_eq!(reopened.info("t").expect("table present").rows, 3);
    assert_eq!(reopened.value("t", 1, 0).as_deref(), Some("10"));

    fs::remove_dir_all(url.parent().unwrap()).ok();
}

/// Port of `DataStoreTests.rectangleQuery`.
#[test]
fn rectangle_query() {
    let (store, url) = make_store();
    let rows: Vec<Vec<String>> = (0..100)
        .map(|r| vec![r.to_string(), format!("x{r}")])
        .collect();
    store.create_table("t", &rows).expect("creates");

    let values = store.values("t", 10..=12, 0..=0).expect("queries");
    let just_values: Vec<&str> = values.iter().map(|(_, _, v)| v.as_str()).collect();
    assert_eq!(just_values, ["10", "11", "12"]);

    fs::remove_dir_all(url.parent().unwrap()).ok();
}

/// Port of `DataStoreTests.dataSheetSemantics`.
#[test]
fn data_sheet_semantics() {
    let (store, url) = make_store();
    store
        .create_table(
            "sales",
            &table(&[
                &["month", "amount"], // header row (text)
                &["jan", "100"],
                &["feb", "250.5"],
                &["mar", ""], // empty amount
            ]),
        )
        .expect("creates");
    let sheet = DataSheet::new("sales", Rc::clone(&store)).expect("sheet resolves");
    assert_eq!(sheet.row_count(), 4);
    assert_eq!(sheet.column_count(), 2);

    // 1-based reference semantics, bounded by the table.
    assert_eq!(
        sheet.numeric_value("B", 2).unwrap(),
        BigDecimal::from_int(100)
    );
    assert_eq!(sheet.numeric_value("B", 4).unwrap(), BigDecimal::zero()); // empty → 0
    assert!(sheet.numeric_value("B", 1).is_err()); // header text
    assert_eq!(
        sheet.numeric_value("B", 1).unwrap_err().to_string(),
        "cell B:1 is not a number"
    );
    assert!(sheet.numeric_value("C", 1).is_err()); // beyond cols
    assert_eq!(
        sheet.numeric_value("C", 1).unwrap_err().to_string(),
        "cell C:1 is outside this data sheet"
    );
    assert!(sheet.numeric_value("B", 5).is_err()); // beyond rows

    // Ranges skip the header text and empties (grid-consistent).
    let values = sheet.numeric_values("B", 1, "B", 4).expect("range reads");
    assert_eq!(
        values,
        vec![
            BigDecimal::from_int(100),
            BigDecimal::parse("250.5").unwrap()
        ]
    );
    assert_eq!(sheet.raw_value(0, 1), "amount");

    fs::remove_dir_all(url.parent().unwrap()).ok();
}

/// Port of `DataStoreTests.editsWriteThroughAndPersist`.
#[test]
fn edits_write_through_and_persist() {
    let (store, url) = make_store();
    store
        .create_table("sales", &table(&[&["month", "amount"], &["jan", "100"]]))
        .expect("creates");
    let sheet = DataSheet::new("sales", Rc::clone(&store)).expect("sheet resolves");

    // Overwrite, blank (sparse delete), and fill an empty cell.
    sheet.set_raw_value("125.5", 1, 1).expect("edits");
    assert_eq!(sheet.raw_value(1, 1), "125.5");
    assert_eq!(
        sheet.numeric_value("B", 2).unwrap(),
        BigDecimal::parse("125.5").unwrap()
    );
    sheet.set_raw_value("", 1, 0).expect("blanks");
    assert_eq!(sheet.raw_value(1, 0), "");
    assert_eq!(sheet.numeric_value("A", 2).unwrap(), BigDecimal::zero()); // empty → 0

    // Bounds: the table's shape is fixed — no growing yet.
    let out_of_rows = sheet.set_raw_value("x", 2, 0).unwrap_err();
    assert_eq!(out_of_rows.to_string(), "cell is outside this data sheet");
    assert!(sheet.set_raw_value("x", 0, 2).is_err());

    // Durable: a fresh handle on the same file sees the edits.
    let reopened = DataStore::new(&url).expect("reopens");
    assert_eq!(reopened.value("sales", 1, 1).as_deref(), Some("125.5"));
    assert_eq!(reopened.value("sales", 1, 0), None);

    fs::remove_dir_all(url.parent().unwrap()).ok();
}

/// Port of `DataStoreTests.bigTableAggregatesStayExact` — 50,000 rows,
/// far beyond the grid, summed exactly.
#[test]
fn big_table_aggregates_stay_exact() {
    let (store, url) = make_store();
    let rows: Vec<Vec<String>> = (1..=50_000)
        .map(|r: i64| vec![r.to_string(), "0.1".to_string()])
        .collect();
    store.create_table("big", &rows).expect("creates");

    let sheet = DataSheet::new("big", Rc::clone(&store)).expect("sheet resolves");
    let values = sheet
        .numeric_values("B", 1, "B", 50_000)
        .expect("range reads");
    let total = values
        .iter()
        .fold(BigDecimal::zero(), |acc, value| &acc + value);
    assert_eq!(total, BigDecimal::from_int(5000)); // 0.1 × 50,000 exactly — no float drift
    assert_eq!(
        sheet.numeric_value("A", 50_000).unwrap(),
        BigDecimal::from_int(50_000)
    );

    fs::remove_dir_all(url.parent().unwrap()).ok();
}
