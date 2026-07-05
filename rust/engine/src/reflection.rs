//! The read-only Workbook reflection graph. A `Workbook` global plus the
//! flat `cell()`/`sheetNames()`/… functions hand the language opaque `.host`
//! handles it navigates uniformly (`.member`, `[…]`, `.method(…)`).
//! Everything here is READ-ONLY — mutation is the separate, log-only API.
//!
//! Handles hold the store/sheet/grid WEAKLY: a stored handle
//! (`w = Workbook.worksheets[0]`) never keeps a removed sheet alive, and
//! reads after teardown throw cleanly. Cell reads route through the ordinary
//! `numeric_value`/`display_value` path (via the re-entry pair the trait
//! now threads), so dependency edges and cycle detection come for free.

use crate::cell_address::CellAddress;
use crate::sheet_store::{Sheet, StoreInner};
use crate::spreadsheet::{CellDisplay, Spreadsheet};
use anzan::eval::evaluator::Reentry;
use anzan::{BigDecimal, EngineError, HostObject, Value};
use std::rc::{Rc, Weak};

/// `Workbook` — the root handle. `.worksheets` is the collection,
/// `.sheetNames` a quick array, `.count` the number of sheets.
pub(crate) struct WorkbookObject {
    pub(crate) store: Weak<StoreInner>,
}

impl HostObject for WorkbookObject {
    fn type_name(&self) -> String {
        "Workbook".to_string()
    }

    fn description(&self) -> String {
        let count = self
            .store
            .upgrade()
            .map(|s| s.sheets.borrow().len())
            .unwrap_or(0);
        format!(
            "Workbook({count} sheet{})",
            if count == 1 { "" } else { "s" }
        )
    }

    fn member(&self, _host: Reentry<'_, '_>, name: &str) -> Option<Value> {
        let store = self.store.upgrade()?;
        match name {
            "worksheets" | "sheets" => Some(Value::Host(Rc::new(WorksheetCollection {
                store: self.store.clone(),
            }))),
            "sheetNames" => Some(Value::Array(
                store
                    .sheets
                    .borrow()
                    .iter()
                    .map(|s| Value::String(s.name()))
                    .collect(),
            )),
            "count" => Some(Value::Number(BigDecimal::from_int(
                store.sheets.borrow().len() as i64,
            ))),
            "activeSheet" => Some(Value::Host(Rc::new(WorksheetObject {
                sheet: Rc::downgrade(&store.active_sheet()),
            }))),
            _ => None,
        }
    }
}

/// `Workbook.worksheets` — index by position (`[0]`, `[-1]` from the end) or
/// by name (`["Budget"]`). `.count` is the number of sheets.
pub(crate) struct WorksheetCollection {
    store: Weak<StoreInner>,
}

impl HostObject for WorksheetCollection {
    fn type_name(&self) -> String {
        "Worksheets".to_string()
    }

    fn description(&self) -> String {
        format!(
            "Worksheets({})",
            self.store
                .upgrade()
                .map(|s| s.sheets.borrow().len())
                .unwrap_or(0)
        )
    }

    fn member(&self, _host: Reentry<'_, '_>, name: &str) -> Option<Value> {
        let store = self.store.upgrade()?;
        if name == "count" {
            return Some(Value::Number(BigDecimal::from_int(
                store.sheets.borrow().len() as i64,
            )));
        }
        None
    }

    fn index(&self, _host: Reentry<'_, '_>, key: &Value) -> Option<Value> {
        let store = self.store.upgrade()?;
        match key {
            Value::Number(position) => {
                let raw = position.int_value()?;
                let count = store.sheets.borrow().len() as i64;
                // Negative indices count from the end (-1 is the last sheet).
                let resolved = if raw < 0 { count + raw } else { raw };
                if !(0..count).contains(&resolved) {
                    return None;
                }
                let sheet = Rc::clone(&store.sheets.borrow()[resolved as usize]);
                Some(Value::Host(Rc::new(WorksheetObject {
                    sheet: Rc::downgrade(&sheet),
                })))
            }
            Value::String(name) => {
                let sheet = store.sheet_named(name)?;
                Some(Value::Host(Rc::new(WorksheetObject {
                    sheet: Rc::downgrade(&sheet),
                })))
            }
            _ => None,
        }
    }
}

/// One worksheet — `.name`, `.rowCount`/`.columnCount`, `.isData`, and the
/// `.cell("A", 2)` method returning a `Cell` handle.
pub(crate) struct WorksheetObject {
    pub(crate) sheet: Weak<Sheet>,
}

impl HostObject for WorksheetObject {
    fn type_name(&self) -> String {
        "Worksheet".to_string()
    }

    fn description(&self) -> String {
        format!(
            "Worksheet({})",
            self.sheet
                .upgrade()
                .map(|s| s.name())
                .unwrap_or_else(|| "—".to_string())
        )
    }

    fn is_equal(&self, other: &dyn HostObject) -> bool {
        // Identity compare — same live sheet. Type check rides the display
        // (Rust trait objects can't downcast without Any; the description
        // includes the type name and the sheet name, and both handles being
        // Worksheets with the same name in one store means the same sheet —
        // names are unique per store).
        other.type_name() == "Worksheet" && other.description() == self.description()
    }

    fn member(&self, _host: Reentry<'_, '_>, name: &str) -> Option<Value> {
        let sheet = self.sheet.upgrade()?;
        match name {
            "name" => Some(Value::String(sheet.name())),
            "rowCount" => Some(Value::Number(BigDecimal::from_int(
                Spreadsheet::ROW_COUNT as i64,
            ))),
            "columnCount" => Some(Value::Number(BigDecimal::from_int(
                Spreadsheet::COLUMN_COUNT as i64,
            ))),
            "isData" => Some(Value::bool(sheet.is_data())),
            _ => None,
        }
    }

    fn call(
        &self,
        _host: Reentry<'_, '_>,
        method: &str,
        arguments: &[Value],
    ) -> Result<Value, EngineError> {
        let Some(sheet) = self.sheet.upgrade() else {
            return Err(EngineError::domain("the worksheet is no longer available"));
        };
        match method {
            "cell" => CellObject::make(&sheet.grid, arguments),
            _ => Err(EngineError::domain(format!(
                "Worksheet has no method '{method}' — try .cell(\"A\", 1)"
            ))),
        }
    }
}

/// One cell — `.value` (numeric, throws when the cell isn't a number,
/// exactly like a plain reference), `.text` (its displayed string),
/// `.raw`/`.formula` (the source), `.address`, `.isEmpty`.
pub(crate) struct CellObject {
    /// Crate-visible so the mutation API (`sheet_store`) can resolve a cell
    /// handle back to its grid + address — the `cell_target` seam.
    pub(crate) grid: Weak<Spreadsheet>,
    pub(crate) address: CellAddress,
}

impl CellObject {
    /// Builds a Cell handle from a `("A", 2)` argument pair, validating the
    /// column letters and row number into an in-range address.
    pub(crate) fn make(grid: &Rc<Spreadsheet>, arguments: &[Value]) -> Result<Value, EngineError> {
        if arguments.len() != 2 {
            return Err(EngineError::domain(
                "cell() takes a column and a row — cell(\"A\", 1)",
            ));
        }
        let Value::String(column) = &arguments[0] else {
            return Err(EngineError::domain(
                "cell()'s first argument is a column letter — cell(\"A\", 1)",
            ));
        };
        let row = match &arguments[1] {
            Value::Number(row_value) => row_value.int_value(),
            _ => None,
        };
        let Some(row) = row else {
            return Err(EngineError::domain(
                "cell()'s second argument is a row number — cell(\"A\", 1)",
            ));
        };
        let Some(address) = CellAddress::from_column_name(column, row) else {
            return Err(EngineError::domain(format!(
                "cell {column}:{row} is out of range"
            )));
        };
        Ok(Value::Host(Rc::new(CellObject {
            grid: Rc::downgrade(grid),
            address,
        })))
    }

    /// The cell's displayed string — the human-readable face of any display.
    fn text(display: CellDisplay) -> String {
        match display {
            CellDisplay::Empty => String::new(),
            CellDisplay::Text(text) => text,
            CellDisplay::Note(comment) => format!("# {comment}"),
            CellDisplay::Value(value) => value.to_string(),
            CellDisplay::Definition(glyph) => glyph,
            CellDisplay::Error(message) => message,
            CellDisplay::Slider(info) | CellDisplay::Stepper(info) => info.value.to_string(),
            CellDisplay::Checkbox(info) => if info.is_on { "true" } else { "false" }.to_string(),
            CellDisplay::Dropdown(info) => info.value.display_text(),
        }
    }
}

impl HostObject for CellObject {
    fn type_name(&self) -> String {
        "Cell".to_string()
    }

    fn description(&self) -> String {
        format!("Cell({})", self.address)
    }

    fn is_equal(&self, other: &dyn HostObject) -> bool {
        other.type_name() == "Cell" && other.description() == self.description()
    }

    fn member(&self, host: Reentry<'_, '_>, name: &str) -> Option<Value> {
        let grid = self.grid.upgrade()?;
        let (evaluator, environment) = host;
        match name {
            "value" => {
                // Routes through numeric_value → records a dependency edge,
                // so a formula reading a cell this way recalcs when that
                // cell changes. A non-numeric cell reads as its placeholder
                // text (member() can't throw); use .text when a cell may
                // hold a label.
                match grid.numeric_value(
                    (evaluator, environment),
                    &self.address.column_name(),
                    self.address.row_number() as i64,
                ) {
                    Ok(value) => Some(Value::Number(value)),
                    Err(_) => Some(Value::String(Self::text(
                        grid.display_value((evaluator, environment), self.address),
                    ))),
                }
            }
            "text" => Some(Value::String(Self::text(
                grid.display_value((evaluator, environment), self.address),
            ))),
            "raw" | "formula" => Some(Value::String(grid.raw(self.address))),
            "address" => Some(Value::String(self.address.to_string())),
            "isEmpty" => Some(Value::bool(
                grid.display_value((evaluator, environment), self.address) == CellDisplay::Empty,
            )),
            _ => None,
        }
    }
}
