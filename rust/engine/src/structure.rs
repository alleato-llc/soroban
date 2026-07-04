//! Structural edits (insert/delete rows & columns) — the port of Swift's
//! `SheetStore+Structure.swift`.
//!
//! One operation = two distinct effects, recorded separately so undo is exact:
//!   1. RAW REWRITES across all grid sheets (shift / refError / range clamps),
//!      recorded with PRE-move addresses and old+new text.
//!   2. A CONTENT MOVE on the edited sheet (cells, names, formats, sizes
//!      re-key; a delete also captures the removed slice).
//!
//! Undo (`revert`) runs the inverse move, restores the removed slice, then
//! re-applies the OLD raws at their pre-op addresses. Redo just re-executes the
//! op — the state at redo time is identical to op time, so the recompute is
//! deterministic.
//!
//! (Data sheets are not yet wired into `Sheet`, so the Swift `isData` refusal
//! has no counterpart here — every sheet is a grid sheet.)

use crate::cell_address::CellAddress;
use crate::cell_format::CellFormat;
use crate::reference_rewriter::{Axis, ReferenceRewriter};
use crate::sheet_store::{Sheet, SheetStore};
use crate::spreadsheet::Spreadsheet;
use anzan::EngineError;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// One cell's raw-text change, at its PRE-move address.
#[derive(Debug, Clone)]
pub struct CellRewrite {
    pub address: CellAddress,
    pub old: String,
    pub new: String,
}

/// Everything needed to undo (or describe) one structural edit. Slot indices
/// are 0-based on BOTH axes (CellAddress space).
#[derive(Debug, Clone)]
pub struct StructuralChange {
    pub axis: Axis,
    pub index: usize,
    pub count: usize,
    pub is_insert: bool,
    pub sheet_name: String,
    pub rewrites: HashMap<String, Vec<CellRewrite>>,
    pub removed_cells: HashMap<CellAddress, String>,
    pub removed_names: HashMap<CellAddress, String>,
    pub removed_formats: HashMap<CellAddress, CellFormat>,
    /// Heights (row axis) / widths (column axis).
    pub removed_sizes: HashMap<usize, f64>,
}

impl SheetStore {
    /// Inserts `count` empty rows/columns at `slot`, shifting content and every
    /// reference (its own sheet's unqualified refs; qualified refs from
    /// anywhere). Refuses when occupied content would fall off the grid.
    pub fn insert_slots(
        &self,
        axis: Axis,
        slot: usize,
        count: usize,
        sheet: &Rc<Sheet>,
    ) -> Result<StructuralChange, EngineError> {
        self.validate_structural(axis, slot, count, true)?;
        let bound = axis_bound(axis);
        let occupied = sheet
            .grid
            .raws()
            .keys()
            .chain(sheet.grid.cell_names().keys())
            .map(|a| position(a, axis))
            .collect::<Vec<_>>();
        if occupied
            .iter()
            .any(|&p| p >= bound.saturating_sub(count) && p >= slot)
        {
            let unit = if axis == Axis::Row {
                "row(s)"
            } else {
                "column(s)"
            };
            return Err(EngineError::domain(format!(
                "inserting would push cells off the grid (the last {count} {unit} must be empty)"
            )));
        }
        Ok(self.execute_structural(axis, slot, count, true, sheet))
    }

    /// Deletes `count` rows/columns at `slot`. References INTO the deleted band
    /// become `refError()`; range corners clamp inward.
    pub fn delete_slots(
        &self,
        axis: Axis,
        slot: usize,
        count: usize,
        sheet: &Rc<Sheet>,
    ) -> Result<StructuralChange, EngineError> {
        self.validate_structural(axis, slot, count, false)?;
        Ok(self.execute_structural(axis, slot, count, false, sheet))
    }

    /// Exact inverse of a recorded change (undo). The caller guarantees the
    /// stack discipline: the sheet is in the op's post state.
    pub fn revert(&self, change: &StructuralChange) {
        let Some(sheet) = self.sheet_named(&change.sheet_name) else {
            return;
        };
        let delta: i64 = if change.is_insert {
            -(change.count as i64)
        } else {
            change.count as i64
        };
        let drop_band: Option<Range<usize>> = if change.is_insert {
            Some(change.index..change.index + change.count)
        } else {
            None
        };
        let from = change.index + if change.is_insert { change.count } else { 0 };
        let needle = change.sheet_name.to_lowercase();

        // 1. Other sheets: restore old raws.
        for (sheet_name, rewrites) in &change.rewrites {
            if sheet_name.to_lowercase() == needle {
                continue;
            }
            let Some(other) = self.sheet_named(sheet_name) else {
                continue;
            };
            for rewrite in rewrites {
                other.grid.set_cell(Some(&rewrite.old), rewrite.address);
            }
        }

        // 2. Edited sheet: inverse content move (+ removed-slice restore), then
        //    old raws at their pre-op addresses.
        let mut raws: HashMap<CellAddress, String> = HashMap::new();
        for (address, raw) in sheet.grid.raws() {
            if let Some(moved) = moved(&address, change.axis, from, delta, drop_band.as_ref()) {
                raws.insert(moved, raw);
            }
        }
        for (address, raw) in &change.removed_cells {
            raws.insert(*address, raw.clone());
        }
        if let Some(own) = change.rewrites.get(&change.sheet_name) {
            for rewrite in own {
                raws.insert(rewrite.address, rewrite.old.clone());
            }
        }

        let mut names: HashMap<CellAddress, String> = HashMap::new();
        for (address, name) in sheet.grid.cell_names() {
            if let Some(moved) = moved(&address, change.axis, from, delta, drop_band.as_ref()) {
                names.insert(moved, name);
            }
        }
        for (address, name) in &change.removed_names {
            names.insert(*address, name.clone());
        }

        let mut formats: HashMap<CellAddress, CellFormat> = HashMap::new();
        for (address, format) in sheet.formats.borrow().iter() {
            if let Some(moved) = moved(address, change.axis, from, delta, drop_band.as_ref()) {
                formats.insert(moved, format.clone());
            }
        }
        for (address, format) in &change.removed_formats {
            formats.insert(*address, format.clone());
        }

        let mut sizes = moved_sizes(
            &sizes_of(&sheet, change.axis),
            from,
            delta,
            drop_band.as_ref(),
        );
        for (slot, size) in &change.removed_sizes {
            sizes.insert(*slot, *size);
        }

        self.commit_structural(&raws, names, formats, sizes, change.axis, &sheet);
    }

    // MARK: Mechanics

    fn validate_structural(
        &self,
        axis: Axis,
        slot: usize,
        count: usize,
        is_insert: bool,
    ) -> Result<(), EngineError> {
        let bound = axis_bound(axis);
        if count < 1 || slot >= bound || (!is_insert && slot + count > bound) {
            return Err(EngineError::domain("out of grid bounds"));
        }
        Ok(())
    }

    fn execute_structural(
        &self,
        axis: Axis,
        slot: usize,
        count: usize,
        is_insert: bool,
        sheet: &Rc<Sheet>,
    ) -> StructuralChange {
        let delta: i64 = if is_insert {
            count as i64
        } else {
            -(count as i64)
        };
        // The rewriter speaks 1-based rows (token space), 0-based columns.
        let rewrite_from: i64 = if axis == Axis::Row {
            slot as i64 + 1
        } else {
            slot as i64
        };

        // 1. Raw rewrites everywhere, recorded at pre-move addresses.
        let mut all_rewrites: HashMap<String, Vec<CellRewrite>> = HashMap::new();
        let sheets = self.sheets();
        for other in &sheets {
            let on_edited = Rc::ptr_eq(other, sheet);
            let mut rewrites: Vec<CellRewrite> = Vec::new();
            for (address, raw) in other.grid.raws() {
                if let Some(rewritten) = ReferenceRewriter::shifting(
                    &raw,
                    axis,
                    rewrite_from,
                    delta,
                    &sheet.name(),
                    on_edited,
                ) {
                    rewrites.push(CellRewrite {
                        address,
                        old: raw,
                        new: rewritten,
                    });
                }
            }
            if rewrites.is_empty() {
                continue;
            }
            // Other sheets change text in place; the edited sheet's rewrites
            // fold into the rebuilt map below.
            if !on_edited {
                for rewrite in &rewrites {
                    other.grid.set_cell(Some(&rewrite.new), rewrite.address);
                }
            }
            all_rewrites.insert(other.name(), rewrites);
        }
        let own_rewrites: HashMap<CellAddress, String> = all_rewrites
            .get(&sheet.name())
            .map(|rewrites| {
                rewrites
                    .iter()
                    .map(|r| (r.address, r.new.clone()))
                    .collect()
            })
            .unwrap_or_default();

        // 2. Content move on the edited sheet.
        let drop_band: Option<Range<usize>> = if is_insert {
            None
        } else {
            Some(slot..slot + count)
        };

        let mut removed_cells: HashMap<CellAddress, String> = HashMap::new();
        let mut raws: HashMap<CellAddress, String> = HashMap::new();
        for (address, raw) in sheet.grid.raws() {
            let effective = own_rewrites
                .get(&address)
                .cloned()
                .unwrap_or_else(|| raw.clone());
            match moved(&address, axis, slot, delta, drop_band.as_ref()) {
                Some(moved) => {
                    raws.insert(moved, effective);
                }
                None => {
                    // pre-rewrite text: it's leaving the grid.
                    removed_cells.insert(address, raw);
                }
            }
        }

        let mut removed_names: HashMap<CellAddress, String> = HashMap::new();
        let mut names: HashMap<CellAddress, String> = HashMap::new();
        for (address, name) in sheet.grid.cell_names() {
            match moved(&address, axis, slot, delta, drop_band.as_ref()) {
                Some(moved) => {
                    names.insert(moved, name);
                }
                None => {
                    removed_names.insert(address, name);
                }
            }
        }

        let mut removed_formats: HashMap<CellAddress, CellFormat> = HashMap::new();
        let mut formats: HashMap<CellAddress, CellFormat> = HashMap::new();
        for (address, format) in sheet.formats.borrow().iter() {
            match moved(address, axis, slot, delta, drop_band.as_ref()) {
                Some(moved) => {
                    formats.insert(moved, format.clone());
                }
                None => {
                    removed_formats.insert(*address, format.clone());
                }
            }
        }

        let mut removed_sizes: HashMap<usize, f64> = HashMap::new();
        if let Some(band) = &drop_band {
            for (key, size) in sizes_of(sheet, axis) {
                if band.contains(&key) {
                    removed_sizes.insert(key, size);
                }
            }
        }
        let moved_sizes = moved_sizes(&sizes_of(sheet, axis), slot, delta, drop_band.as_ref());

        self.commit_structural(&raws, names, formats, moved_sizes, axis, sheet);

        StructuralChange {
            axis,
            index: slot,
            count,
            is_insert,
            sheet_name: sheet.name(),
            rewrites: all_rewrites,
            removed_cells,
            removed_names,
            removed_formats,
            removed_sizes,
        }
    }

    fn commit_structural(
        &self,
        raws: &HashMap<CellAddress, String>,
        names: HashMap<CellAddress, String>,
        formats: HashMap<CellAddress, CellFormat>,
        sizes: HashMap<usize, f64>,
        axis: Axis,
        sheet: &Rc<Sheet>,
    ) {
        sheet.grid.clear_all_slider_overrides(); // no drag survives a structural edit
        sheet.grid.load_cell_names(names);
        *sheet.formats.borrow_mut() = formats;
        match axis {
            Axis::Row => *sheet.row_heights.borrow_mut() = sizes,
            Axis::Column => *sheet.column_widths.borrow_mut() = sizes,
        }
        sheet.grid.load(raws); // reparse + rebuild definitions + recalculate
        self.recalculate();
    }
}

fn axis_bound(axis: Axis) -> usize {
    match axis {
        Axis::Row => Spreadsheet::ROW_COUNT,
        Axis::Column => Spreadsheet::COLUMN_COUNT,
    }
}

fn position(address: &CellAddress, axis: Axis) -> usize {
    match axis {
        Axis::Row => address.row,
        Axis::Column => address.column,
    }
}

/// Where an address lands after the move; `None` = inside the dropped band.
fn moved(
    address: &CellAddress,
    axis: Axis,
    from: usize,
    delta: i64,
    band: Option<&Range<usize>>,
) -> Option<CellAddress> {
    let pos = position(address, axis);
    if let Some(band) = band {
        if band.contains(&pos) {
            return None;
        }
    }
    let threshold = band.map(|b| b.end).unwrap_or(from);
    if pos < threshold {
        return Some(*address);
    }
    let shifted = (pos as i64 + delta) as usize;
    Some(match axis {
        Axis::Row => CellAddress::new(address.column, shifted),
        Axis::Column => CellAddress::new(shifted, address.row),
    })
}

fn sizes_of(sheet: &Sheet, axis: Axis) -> HashMap<usize, f64> {
    match axis {
        Axis::Row => sheet.row_heights.borrow().clone(),
        Axis::Column => sheet.column_widths.borrow().clone(),
    }
}

fn moved_sizes(
    sizes: &HashMap<usize, f64>,
    from: usize,
    delta: i64,
    band: Option<&Range<usize>>,
) -> HashMap<usize, f64> {
    let threshold = band.map(|b| b.end).unwrap_or(from);
    let mut out: HashMap<usize, f64> = HashMap::new();
    for (&key, &size) in sizes {
        if let Some(band) = band {
            if band.contains(&key) {
                continue;
            }
        }
        let new_key = if key >= threshold {
            (key as i64 + delta) as usize
        } else {
            key
        };
        out.insert(new_key, size);
    }
    out
}

#[cfg(test)]
mod tests {
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
}
