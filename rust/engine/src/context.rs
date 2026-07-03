//! Shared by every sheet in a store. Two jobs:
//!  1. The owning-sheet stack — while a formula on sheet X evaluates, its
//!     unqualified `A:1` references must resolve against X, not whichever
//!     tab is active in the UI.
//!  2. Cross-sheet cycle detection — `Sheet1!A:1 → Sheet2!B:1 → Sheet1!A:1`
//!     must report a circular reference, not recurse forever, so the
//!     in-flight set is keyed by (sheet identity, address).
//!
//! Interior-mutable throughout (RefCell with short borrows) — evaluation
//! re-enters these structures recursively.

use crate::cell_address::CellAddress;
use crate::spreadsheet::Spreadsheet;
use std::cell::{Cell as StdCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use std::rc::{Rc, Weak};

/// The Swift side keys on ObjectIdentifier; here each registered sheet gets
/// a store-unique id.
pub(crate) type SheetId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CellKey {
    pub sheet: SheetId,
    pub address: CellAddress,
}

/// Range reads, per source sheet: rect + reader.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RangeRead {
    rows: RangeInclusive<usize>,
    columns: RangeInclusive<usize>,
    reader: CellKey,
}

#[derive(Default)]
pub struct ResolutionContext {
    next_id: StdCell<SheetId>,
    pub(crate) resolving: RefCell<HashSet<CellKey>>,
    sheet_stack: RefCell<Vec<Weak<Spreadsheet>>>,
    key_stack: RefCell<Vec<CellKey>>,
    /// source cell → cells whose formulas read it.
    dependents: RefCell<HashMap<CellKey, HashSet<CellKey>>>,
    range_dependents: RefCell<HashMap<SheetId, HashSet<RangeRead>>>,
    /// Registered sheets, weakly — invalidation needs to reach their memos.
    sheets: RefCell<HashMap<SheetId, Weak<Spreadsheet>>>,
}

impl ResolutionContext {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// Identity comes from the shared context (the Swift side keys on
    /// ObjectIdentifier; Rc has no stable public id, so the context assigns
    /// one before construction and attaches the weak ref after).
    pub(crate) fn allocate_id(&self) -> SheetId {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    pub(crate) fn attach(&self, id: SheetId, sheet: &Rc<Spreadsheet>) {
        self.sheets.borrow_mut().insert(id, Rc::downgrade(sheet));
    }

    /// The sheet that owns the formula being evaluated right now.
    pub(crate) fn current_sheet(&self) -> Option<Rc<Spreadsheet>> {
        self.sheet_stack.borrow().last().and_then(Weak::upgrade)
    }

    /// The cell whose formula is evaluating right now — dependency edges
    /// point at it.
    pub(crate) fn current_key(&self) -> Option<CellKey> {
        self.key_stack.borrow().last().copied()
    }

    pub(crate) fn push(&self, sheet: &Rc<Spreadsheet>, key: CellKey) {
        self.sheet_stack.borrow_mut().push(Rc::downgrade(sheet));
        self.key_stack.borrow_mut().push(key);
    }

    pub(crate) fn pop(&self) {
        self.sheet_stack.borrow_mut().pop();
        self.key_stack.borrow_mut().pop();
    }

    // MARK: Dependency graph (edit → invalidate only the affected closure)

    /// Called when a formula reads one cell of `source`.
    pub(crate) fn record_cell_read(&self, source: CellKey) {
        let Some(reader) = self.current_key() else {
            return;
        };
        if reader == source {
            return;
        }
        self.dependents
            .borrow_mut()
            .entry(source)
            .or_default()
            .insert(reader);
    }

    /// Called when a formula reads a rectangle of `sheet`.
    pub(crate) fn record_range_read(
        &self,
        sheet: SheetId,
        rows: RangeInclusive<usize>,
        columns: RangeInclusive<usize>,
    ) {
        let Some(reader) = self.current_key() else {
            return;
        };
        self.range_dependents
            .borrow_mut()
            .entry(sheet)
            .or_default()
            .insert(RangeRead {
                rows,
                columns,
                reader,
            });
    }

    /// A cell changed: drop its memo and, transitively, every reader's —
    /// across sheets. Edges may be stale (a reader's formula changed since
    /// recording); that only over-invalidates, which is correctness-safe.
    pub(crate) fn invalidate(&self, start: CellKey) {
        let mut queue = vec![start];
        let mut visited: HashSet<CellKey> = HashSet::new();
        while let Some(key) = queue.pop() {
            if !visited.insert(key) {
                continue;
            }
            if let Some(sheet) = self.sheets.borrow().get(&key.sheet).and_then(Weak::upgrade) {
                sheet.clear_memo(key.address);
            }
            if let Some(direct) = self.dependents.borrow().get(&key) {
                queue.extend(direct.iter().copied());
            }
            if let Some(ranges) = self.range_dependents.borrow().get(&key.sheet) {
                for read in ranges {
                    if read.rows.contains(&key.address.row)
                        && read.columns.contains(&key.address.column)
                    {
                        queue.push(read.reader);
                    }
                }
            }
        }
    }

    /// Everything is suspect (variables changed, sheets renamed/removed,
    /// workbook loaded): clear all memos and start the graph fresh.
    pub(crate) fn invalidate_everything(&self) {
        self.dependents.borrow_mut().clear();
        self.range_dependents.borrow_mut().clear();
        for sheet in self.sheets.borrow().values() {
            if let Some(sheet) = sheet.upgrade() {
                sheet.clear_all_memo();
            }
        }
    }
}
