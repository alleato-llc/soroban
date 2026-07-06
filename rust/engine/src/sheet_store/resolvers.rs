//! Installs the Calculator resolver closures — cell/range/name references,
//! sheet-scoped definitions, the read-only Workbook/History reflection API,
//! and the default (direct, no-undo) workbook mutation commands.

use super::{SheetStore, StoreInner};
use crate::spreadsheet::Spreadsheet;
use anzan::EngineError;
use std::rc::{Rc, Weak};

impl SheetStore {
    pub(super) fn install_resolvers(&self) {
        let mut calculator = self.calculator.borrow_mut();
        let no_sheets = || EngineError::domain("no sheets available");

        let weak: Weak<StoreInner> = Rc::downgrade(&self.inner);
        calculator.resolvers.cell = Some(Box::new(move |host, sheet_name, column, row| {
            let inner = weak.upgrade().ok_or_else(no_sheets)?;
            // Unqualified refs inside a grid formula belong to the owning
            // grid.
            if sheet_name.is_none() {
                if let Some(current) = inner.context.current_sheet() {
                    return current.numeric_value(host, column, row);
                }
            }
            let target = inner.sheet_for_reference(sheet_name)?;
            // A qualified ref into a data sheet reads the table (bounded by the
            // table, not the grid); data sheets own no formulas so an
            // unqualified ref never lands here.
            if let Some(data) = &*target.data.borrow() {
                return data.numeric_value(column, row);
            }
            target.grid.numeric_value(host, column, row)
        }));

        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.range = Some(Box::new(move |host, sheet_name, fc, fr, tc, tr| {
            let inner = weak.upgrade().ok_or_else(no_sheets)?;
            if sheet_name.is_none() {
                if let Some(current) = inner.context.current_sheet() {
                    return current.numeric_values(host, fc, fr, tc, tr);
                }
            }
            let target = inner.sheet_for_reference(sheet_name)?;
            if let Some(data) = &*target.data.borrow() {
                return data.numeric_values(fc, fr, tc, tr);
            }
            target.grid.numeric_values(host, fc, fr, tc, tr)
        }));

        // Named cells: 'Projected Rate' routes like an unqualified A:1
        // (owning sheet, active from the log); Budget!'Rate' by sheet name.
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.name = Some(Box::new(move |host, sheet_name, name| {
            let inner = weak.upgrade().ok_or_else(no_sheets)?;
            if sheet_name.is_none() {
                if let Some(current) = inner.context.current_sheet() {
                    return current.numeric_value_for_name(host, name);
                }
            }
            let target = inner.sheet_for_reference(sheet_name)?;
            if target.is_data() {
                return Err(EngineError::domain("data sheets don't have named cells"));
            }
            target.grid.numeric_value_for_name(host, name)
        }));

        // Sheet-scoped λ/𝑖/𝑫 definitions: resolved against the formula's
        // owning sheet (mid-evaluation) or the active tab (log input).
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.scoped_function = Some(Box::new(move |name| {
            weak.upgrade()?.scope_sheet().defined_function(name)
        }));
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.scoped_variable = Some(Box::new(move |host, name| {
            let Some(inner) = weak.upgrade() else {
                return Ok(None);
            };
            inner.scope_sheet().defined_value(host, name)
        }));
        let weak = Rc::downgrade(&self.inner);
        calculator.scoped_definition_owner = Some(Box::new(move |name| {
            weak.upgrade()?.scope_sheet().definition_owner(name)
        }));
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.scoped_data_type = Some(Box::new(move |name| {
            weak.upgrade()?.scope_sheet().defined_data_type(name)
        }));

        // The read-only Workbook reflection API: the `Workbook` global and
        // the flat cell()/sheetName()/sheetNames()/rowCount()/columnCount()
        // accessors. Reflection names are case-sensitive, like data types.
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.host_value = Some(Box::new(move |name, in_log| {
            let inner = weak.upgrade()?;
            match name {
                "Workbook" => Some(anzan::Value::Host(Rc::new(
                    crate::reflection::WorkbookObject {
                        store: Rc::downgrade(&inner),
                    },
                ))),
                // `History` is LOG-ONLY: in a cell it stays None and the
                // name degrades to a text label (unknownVariable → text),
                // never an error. Unknown too when no host wired a log.
                "History" if in_log => {
                    let source = inner.log_source.borrow().as_ref().map(Rc::clone)?;
                    Some(crate::history_reflection::value_from(&source))
                }
                _ => None,
            }
        }));
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.host_function = Some(Box::new(move |host, name, arguments| {
            let Some(inner) = weak.upgrade() else {
                return Ok(None);
            };
            let expect_arity = |count: usize| -> Result<(), EngineError> {
                if arguments.len() != count {
                    return Err(EngineError::domain(format!(
                        "{name}() takes {count} argument{}",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                Ok(())
            };
            let _ = host;
            match name {
                // cell(col, row) on the scope sheet; cell(sheet, col, row)
                // by name.
                "cell" => match arguments.len() {
                    2 => crate::reflection::CellObject::make(&inner.scope_sheet(), arguments)
                        .map(Some),
                    3 => {
                        let anzan::Value::String(sheet_name) = &arguments[0] else {
                            return Err(EngineError::domain(
                                "cell()'s first argument is a sheet name — cell(\"Budget\", \"A\", 1)",
                            ));
                        };
                        let Some(sheet) = inner.sheet_named(sheet_name) else {
                            return Err(EngineError::domain(format!(
                                "unknown sheet '{sheet_name}'"
                            )));
                        };
                        crate::reflection::CellObject::make(&sheet.grid, &arguments[1..]).map(Some)
                    }
                    _ => Err(EngineError::domain(
                        "cell() takes (column, row) or (sheet, column, row)",
                    )),
                },
                "sheetNames" => {
                    expect_arity(0)?;
                    Ok(Some(anzan::Value::Array(
                        inner
                            .sheets
                            .borrow()
                            .iter()
                            .map(|s| anzan::Value::String(s.name()))
                            .collect(),
                    )))
                }
                "sheetName" => {
                    expect_arity(0)?;
                    Ok(Some(anzan::Value::String(inner.scope_sheet_item().name())))
                }
                "rowCount" => {
                    expect_arity(0)?;
                    Ok(Some(anzan::Value::Number(anzan::BigDecimal::from_int(
                        Spreadsheet::ROW_COUNT as i64,
                    ))))
                }
                "columnCount" => {
                    expect_arity(0)?;
                    Ok(Some(anzan::Value::Number(anzan::BigDecimal::from_int(
                        Spreadsheet::COLUMN_COUNT as i64,
                    ))))
                }
                // Not a reflection function — fall through to unknown.
                _ => Ok(None),
            }
        }));

        // The DEFAULT (direct, no-undo) workbook mutation API: `updateCell`
        // / `addWorksheet` / `renameWorksheet` / `deleteWorksheet`. These
        // change the workbook, so they run from the LOG only — `in_log` is
        // false during cell recalc and the resolver throws then (recalc
        // stays reproducible). Resolved LAST in call resolution, so a
        // user's own `updateCell(…)` shadows it; an app host may replace
        // this resolver to make the same commands undoable — this default
        // is what the CLI and headless tests see.
        let weak = Rc::downgrade(&self.inner);
        calculator.resolvers.host_mutation =
            Some(Box::new(move |_host, name, arguments, in_log| {
                let Some(inner) = weak.upgrade() else {
                    return Ok(None);
                };
                if !Self::MUTATION_NAMES.contains(&name) {
                    return Ok(None);
                }
                if !in_log {
                    return Err(EngineError::domain(format!(
                    "'{name}' changes the workbook — it runs in the calculation log, not a cell"
                )));
                }
                match name {
                    "updateCell" => inner.mutate_update_cell(arguments).map(Some),
                    "addWorksheet" => inner.mutate_add_worksheet(arguments).map(Some),
                    "renameWorksheet" => inner.mutate_rename_worksheet(arguments).map(Some),
                    "deleteWorksheet" => inner.mutate_delete_worksheet(arguments).map(Some),
                    _ => Ok(None),
                }
            }));
    }
}
