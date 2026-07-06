//! Tests for structural row/column insert/delete edits.

use super::*;
use crate::spreadsheet::CellDisplay;
use crate::Calculator;
use std::cell::RefCell;

fn make_store() -> SheetStore {
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    SheetStore::new(calculator)
}

fn addr(key: &str) -> CellAddress {
    CellAddress::from_key(key).expect("a valid test address")
}

fn set(sheet: &Rc<Sheet>, key: &str, raw: &str) {
    sheet.grid.set_cell(Some(raw), addr(key));
}

fn raw(sheet: &Rc<Sheet>, key: &str) -> String {
    sheet.grid.raw(addr(key))
}

fn number(store: &SheetStore, sheet: &Rc<Sheet>, key: &str) -> i64 {
    match store.display_value_on(sheet, addr(key)) {
        CellDisplay::Value(value) => value.to_string().parse().expect("an integer value"),
        other => panic!("expected a number at {key}, got {other:?}"),
    }
}

#[test]
fn insert_row_shifts_content_and_references_down() {
    let store = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "10");
    set(&sheet, "A:2", "A:1 + 5"); // reads the row above
    assert_eq!(number(&store, &sheet, "A:2"), 15);

    // Insert one row at slot 0 (before row 1): everything moves down one.
    store
        .insert_slots(Axis::Row, 0, 1, &sheet)
        .expect("insert succeeds");
    assert_eq!(raw(&sheet, "A:1"), ""); // the new empty row
    assert_eq!(raw(&sheet, "A:2"), "10");
    assert_eq!(raw(&sheet, "A:3"), "A:2 + 5"); // reference followed the shift
    assert_eq!(number(&store, &sheet, "A:3"), 15);
}

#[test]
fn delete_row_removes_content_and_kills_references() {
    let store = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "10");
    set(&sheet, "A:2", "20");
    set(&sheet, "A:3", "A:2 + 1"); // reads the row being deleted

    // Delete row 2 (slot 1).
    store
        .delete_slots(Axis::Row, 1, 1, &sheet)
        .expect("delete succeeds");
    assert_eq!(raw(&sheet, "A:1"), "10");
    // Old A:3 slid up to A:2, and its reference into the deleted band died.
    assert_eq!(raw(&sheet, "A:2"), "refError() + 1");
    assert!(matches!(
        store.display_value_on(&sheet, addr("A:2")),
        CellDisplay::Error(_)
    ));
}

#[test]
fn insert_refuses_when_content_would_fall_off_the_grid() {
    let store = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1000", "1"); // the last row is occupied

    let error = store
        .insert_slots(Axis::Row, 0, 1, &sheet)
        .expect_err("insert must refuse");
    assert!(
        error.to_string().contains("push cells off the grid"),
        "{error}"
    );
}

#[test]
fn insert_column_rewrites_qualified_cross_sheet_references() {
    let store = make_store();
    store.add_sheet().expect("adds Sheet 2");
    let sheets = store.sheets();
    let (s1, s2) = (&sheets[0], &sheets[1]);
    set(s1, "B:1", "7");
    set(s2, "A:1", "'Sheet 1'!B:1 + 1"); // qualified ref into Sheet 1
    assert_eq!(number(&store, s2, "A:1"), 8);

    // Insert a column at slot 0 on Sheet 1: B → C, and the qualified ref
    // on Sheet 2 follows.
    store
        .insert_slots(Axis::Column, 0, 1, s1)
        .expect("insert succeeds");
    assert_eq!(raw(s1, "C:1"), "7");
    assert_eq!(raw(s2, "A:1"), "'Sheet 1'!C:1 + 1");
    assert_eq!(number(&store, s2, "A:1"), 8);
}

#[test]
fn undo_restores_the_pre_insert_state_exactly() {
    let store = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "10");
    set(&sheet, "A:2", "A:1 + 5");

    let change = store
        .insert_slots(Axis::Row, 0, 1, &sheet)
        .expect("insert succeeds");
    store.revert(&change);

    assert_eq!(raw(&sheet, "A:1"), "10");
    assert_eq!(raw(&sheet, "A:2"), "A:1 + 5");
    assert_eq!(raw(&sheet, "A:3"), "");
    assert_eq!(number(&store, &sheet, "A:2"), 15);
}

#[test]
fn undo_restores_the_deleted_slice() {
    let store = make_store();
    let sheet = store.active_sheet();
    set(&sheet, "A:1", "10");
    set(&sheet, "A:2", "20");
    set(&sheet, "A:3", "30");

    let change = store
        .delete_slots(Axis::Row, 1, 1, &sheet)
        .expect("delete succeeds");
    // After delete: A:1=10, A:2=30 (slid up).
    assert_eq!(raw(&sheet, "A:2"), "30");

    store.revert(&change);
    assert_eq!(raw(&sheet, "A:1"), "10");
    assert_eq!(raw(&sheet, "A:2"), "20"); // the deleted row is back
    assert_eq!(raw(&sheet, "A:3"), "30");
    assert_eq!(number(&store, &sheet, "A:1"), 10);
}
