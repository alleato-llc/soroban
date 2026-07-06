//! Workbook mutation — the log-only commands' DIRECT (no-undo) default.
//! `updateCell` / `addWorksheet` / `renameWorksheet` / `deleteWorksheet`
//! mutate the store; worksheet targets resolve to an index here too.

use super::{SheetStore, StoreInner};
use crate::cell_address::CellAddress;
use crate::reference_rewriter::ReferenceRewriter;
use crate::reflection::{CellObject, WorksheetObject};
use anzan::{BigDecimal, EngineError, Value};
use std::rc::Rc;

impl StoreInner {
    // MARK: Workbook mutation (the log-only commands' DIRECT default)
    //
    // `updateCell` / `addWorksheet` / `renameWorksheet` / `deleteWorksheet`
    // mutate the store directly — no undo, no journal — which is what the
    // CLI and headless tests want. An app host may replace the resolver to
    // make the SAME commands undoable; these are the reference semantics
    // that override must match. A worksheet TARGET is either a `Worksheet`
    // handle (`Workbook.worksheets[0]`) or a sheet-name string — both
    // resolve to an index via `sheet_index_for_target`.

    /// A `Worksheet` handle for the sheet at `index` — the value the
    /// mutation commands return, built identically by the engine default
    /// and any host override.
    pub(crate) fn worksheet_handle(&self, index: usize) -> Value {
        Value::Host(Rc::new(WorksheetObject {
            sheet: Rc::downgrade(&self.sheets.borrow()[index]),
        }))
    }

    /// The concrete cell handle behind `updateCell`'s first argument.
    fn cell_object(value: &Value) -> Result<&CellObject, EngineError> {
        let not_a_cell =
            || EngineError::domain("updateCell()'s first argument is a cell — e.g. cell(\"A\", 1)");
        let Value::Host(object) = value else {
            return Err(not_a_cell());
        };
        let any: &dyn std::any::Any = object.as_ref();
        any.downcast_ref::<CellObject>().ok_or_else(not_a_cell)
    }

    /// Resolves a CELL handle (`cell("A", 1)` / `…cell("A", 1)`) to the
    /// index of the sheet it lives on and its address — so a host can write
    /// it undoably.
    pub(crate) fn cell_target(&self, value: &Value) -> Result<(usize, CellAddress), EngineError> {
        let cell = Self::cell_object(value)?;
        let stale = || EngineError::domain("that cell's sheet is no longer in the workbook");
        let grid = cell.grid.upgrade().ok_or_else(stale)?;
        let sheets = self.sheets.borrow();
        let index = sheets
            .iter()
            .position(|s| Rc::ptr_eq(&s.grid, &grid))
            .ok_or_else(stale)?;
        Ok((index, cell.address))
    }

    /// Resolves a worksheet TARGET — a `Worksheet` handle or a name string —
    /// to its current index in the workbook.
    pub(crate) fn sheet_index_for_target(&self, value: &Value) -> Result<usize, EngineError> {
        match value {
            Value::String(name) => {
                let needle = name.to_lowercase();
                self.sheets
                    .borrow()
                    .iter()
                    .position(|s| s.name().to_lowercase() == needle)
                    .ok_or_else(|| EngineError::domain(format!("unknown sheet '{name}'")))
            }
            Value::Host(object) => {
                let stale = || EngineError::domain("that worksheet is no longer in the workbook");
                let any: &dyn std::any::Any = object.as_ref();
                let worksheet = any.downcast_ref::<WorksheetObject>().ok_or_else(stale)?;
                let sheet = worksheet.sheet.upgrade().ok_or_else(stale)?;
                self.sheets
                    .borrow()
                    .iter()
                    .position(|s| Rc::ptr_eq(s, &sheet))
                    .ok_or_else(stale)
            }
            other => Err(EngineError::domain(format!(
                "expected a worksheet or a sheet name, got {}",
                other.kind_name()
            ))),
        }
    }

    /// Sets a cell's raw contents from a value: a number becomes its
    /// digits, a string is taken verbatim (so `updateCell(c, "=B:1*2")`
    /// writes a formula and `updateCell(c, "Total")` a label). An empty
    /// string clears the cell.
    pub(super) fn mutate_update_cell(&self, arguments: &[Value]) -> Result<Value, EngineError> {
        if arguments.len() != 2 {
            return Err(EngineError::domain(
                "updateCell(cell, value) takes a cell and a value",
            ));
        }
        let cell = Self::cell_object(&arguments[0])?;
        let Some(grid) = cell.grid.upgrade() else {
            return Err(EngineError::domain(
                "that cell's sheet is no longer in the workbook",
            ));
        };
        let raw = SheetStore::raw_text_from(&arguments[1])?;
        grid.set_cell(if raw.is_empty() { None } else { Some(&raw) }, cell.address);
        self.recalculate(); // set_cell invalidates readers; recalc keeps cross-sheet fresh
        Ok(arguments[1].clone())
    }

    /// Adds an empty grid sheet and returns its handle.
    pub(super) fn mutate_add_worksheet(&self, arguments: &[Value]) -> Result<Value, EngineError> {
        let [Value::String(name)] = arguments else {
            return Err(EngineError::domain("addWorksheet(name) takes a sheet name"));
        };
        let sheet = self.add_sheet_named(name)?;
        self.recalculate(); // a new sheet may satisfy a previously-unknown qualifier
        Ok(Value::Host(Rc::new(WorksheetObject {
            sheet: Rc::downgrade(&sheet),
        })))
    }

    /// Renames a worksheet AND rewrites every `Old!A:1` / `'Old'!A:1`
    /// qualifier across all grid sheets — the same auto-rewrite the UI
    /// rename performs (references are by name; that's why you rename).
    /// Returns the handle.
    pub(super) fn mutate_rename_worksheet(
        &self,
        arguments: &[Value],
    ) -> Result<Value, EngineError> {
        if arguments.len() != 2 {
            return Err(EngineError::domain(
                "renameWorksheet(sheet, newName) takes a worksheet (or name) and the new name",
            ));
        }
        let index = self.sheet_index_for_target(&arguments[0])?;
        let Value::String(new_name) = &arguments[1] else {
            return Err(EngineError::domain("renameWorksheet()'s new name is text"));
        };
        self.rename_worksheet(index, new_name)?;
        Ok(self.worksheet_handle(index))
    }

    /// Renames the sheet at `index` AND rewrites every `Old!A:1` / `'Old'!A:1`
    /// qualifier across all grid sheets — the same auto-rewrite the UI rename
    /// performs (references are by name; that's why you rename). Shared by the
    /// `renameWorksheet` mutation command and the GUI's tab rename.
    pub(crate) fn rename_worksheet(&self, index: usize, new_name: &str) -> Result<(), EngineError> {
        let old_name = self.sheets.borrow()[index].name();
        self.rename(index, new_name)?; // validates + recalculates
        let resolved = self.sheets.borrow()[index].name();
        if old_name != resolved {
            for sheet in self.sheets.borrow().clone() {
                for (address, raw) in sheet.grid.raws() {
                    if let Some(rewritten) =
                        ReferenceRewriter::renaming_sheet(&raw, &old_name, &resolved)
                    {
                        sheet.grid.set_cell(Some(&rewritten), address);
                    }
                }
            }
            self.recalculate();
        }
        Ok(())
    }

    /// Removes a worksheet (refuses the last one) and returns the new
    /// count. Formulas referencing the removed sheet become "unknown sheet"
    /// errors, exactly as when a tab is removed in the UI.
    pub(super) fn mutate_delete_worksheet(
        &self,
        arguments: &[Value],
    ) -> Result<Value, EngineError> {
        if arguments.len() != 1 {
            return Err(EngineError::domain(
                "deleteWorksheet(sheet) takes a worksheet or a sheet name",
            ));
        }
        let index = self.sheet_index_for_target(&arguments[0])?;
        self.remove_sheet(index)?; // validates (≥1 sheet) + recalculates
        Ok(Value::Number(BigDecimal::from_int(
            self.sheets.borrow().len() as i64,
        )))
    }
}
