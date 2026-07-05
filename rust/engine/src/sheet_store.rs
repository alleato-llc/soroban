//! An ordered collection of named worksheets sharing one Calculator.
//! Owns the resolver wiring: qualified references (`Budget!A:1`) route by
//! name; unqualified ones go to the formula's owning sheet, falling back to
//! the active sheet (the log's perspective).
//!
//! Ownership: the store holds the Calculator in an `Rc<RefCell<…>>`; the
//! resolver closures installed INTO the calculator capture a `Weak` of the
//! store's internals and receive the evaluator + environment as call
//! arguments, so they never touch the Calculator RefCell — the only borrow
//! of it is the host's outermost call.

use crate::cell_address::CellAddress;
use crate::context::ResolutionContext;
use crate::data_store::DataSheet;
use crate::reference_rewriter::ReferenceRewriter;
use crate::reflection::{CellObject, WorksheetObject};
use crate::spreadsheet::{CellDisplay, Spreadsheet};
use anzan::{BigDecimal, Calculator, EngineError, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// One worksheet: a calculation grid plus its name and layout — or, when
/// `data` is set, a DataStore-backed table (CSV-imported, SQLite-persisted).
/// A data sheet still carries a `grid` (for the display name / shared context)
/// but its cell reads route to the table instead; it owns no formulas.
pub struct Sheet {
    pub(crate) name: RefCell<String>,
    pub grid: Rc<Spreadsheet>,
    /// Sparse non-default sizes, in points (the app clamps).
    pub column_widths: RefCell<HashMap<usize, f64>>,
    pub row_heights: RefCell<HashMap<usize, f64>>,
    /// Sparse per-cell presentation (display-only — never touches the
    /// dependency graph). Defaults are pruned, not stored; empty cells may
    /// be formatted (fill a region before its data arrives).
    pub formats: RefCell<HashMap<CellAddress, crate::cell_format::CellFormat>>,
    /// When set, this is a data sheet: reads resolve against the table (bounded
    /// by the table, not the 1000-row grid), and it owns no formulas.
    pub data: RefCell<Option<DataSheet>>,
}

impl Sheet {
    fn new(name: &str, grid: Rc<Spreadsheet>) -> Rc<Self> {
        grid.set_display_name(Some(name.to_string()));
        Rc::new(Self {
            name: RefCell::new(name.to_string()),
            grid,
            column_widths: RefCell::new(HashMap::new()),
            row_heights: RefCell::new(HashMap::new()),
            formats: RefCell::new(HashMap::new()),
            data: RefCell::new(None),
        })
    }

    pub fn name(&self) -> String {
        self.name.borrow().clone()
    }

    /// True when this sheet is backed by a DataStore table rather than a grid.
    pub fn is_data(&self) -> bool {
        self.data.borrow().is_some()
    }
}

pub(crate) struct StoreInner {
    pub(crate) sheets: RefCell<Vec<Rc<Sheet>>>,
    pub(crate) active_index: std::cell::Cell<usize>,
    pub(crate) context: Rc<ResolutionContext>,
    /// The calculation log, for the `History` reflection API (log-only).
    /// Set by the host; `None` in the CLI/tests without a log, where
    /// `History` is simply unknown.
    pub(crate) log_source: RefCell<Option<Rc<dyn crate::history_reflection::LogSource>>>,
}

impl StoreInner {
    pub(crate) fn active_sheet(&self) -> Rc<Sheet> {
        let sheets = self.sheets.borrow();
        let index = self.active_index.get().min(sheets.len() - 1);
        Rc::clone(&sheets[index])
    }

    pub(crate) fn sheet_named(&self, name: &str) -> Option<Rc<Sheet>> {
        let needle = name.to_lowercase();
        self.sheets
            .borrow()
            .iter()
            .find(|s| s.name().to_lowercase() == needle)
            .cloned()
    }

    /// Where a reference points: named sheet, else the active one (log
    /// input).
    pub(crate) fn sheet_for_reference(
        &self,
        sheet_name: Option<&str>,
    ) -> Result<Rc<Sheet>, EngineError> {
        let Some(sheet_name) = sheet_name else {
            return Ok(self.active_sheet());
        };
        self.sheet_named(sheet_name)
            .ok_or_else(|| EngineError::domain(format!("unknown sheet '{sheet_name}'")))
    }

    /// The grid whose definitions are in scope right now (owning sheet
    /// mid-evaluation, active tab from the log).
    pub(crate) fn scope_sheet(&self) -> Rc<Spreadsheet> {
        self.context
            .current_sheet()
            .unwrap_or_else(|| Rc::clone(&self.active_sheet().grid))
    }

    /// The Sheet whose definitions/grid are in scope right now — the
    /// Sheet-level companion to `scope_sheet`, which returns its grid.
    pub(crate) fn scope_sheet_item(&self) -> Rc<Sheet> {
        if let Some(current) = self.context.current_sheet() {
            let sheets = self.sheets.borrow();
            if let Some(sheet) = sheets.iter().find(|s| Rc::ptr_eq(&s.grid, &current)) {
                return Rc::clone(sheet);
            }
        }
        self.active_sheet()
    }

    // MARK: Sheet mechanics
    //
    // These live on StoreInner (not SheetStore) so the resolver closures —
    // which capture only a `Weak<StoreInner>` — can mutate the store. The
    // SheetStore methods of the same names delegate here.

    /// Drops every sheet's memo — a log variable changed, a sheet was
    /// renamed/removed, or a workbook loaded.
    pub(crate) fn recalculate(&self) {
        for sheet in self.sheets.borrow().iter() {
            sheet.grid.recalculate();
        }
    }

    /// A fresh empty sheet built against this store's shared context.
    pub(crate) fn make_sheet(&self, name: &str) -> Rc<Sheet> {
        Sheet::new(name, Spreadsheet::new(Rc::clone(&self.context)))
    }

    /// A data sheet: an ordinary sheet shell whose reads route to `data`.
    pub(crate) fn make_data_sheet(&self, name: &str, data: DataSheet) -> Rc<Sheet> {
        let sheet = self.make_sheet(name);
        *sheet.data.borrow_mut() = Some(data);
        sheet
    }

    fn check_capacity(&self) -> Result<(), EngineError> {
        if self.sheets.borrow().len() >= SheetStore::MAX_SHEETS {
            return Err(EngineError::domain(format!(
                "a workbook holds at most {} sheets",
                SheetStore::MAX_SHEETS
            )));
        }
        Ok(())
    }

    /// Adds an empty grid sheet with a specific (validated) name — the
    /// mutation API's `addWorksheet(name)`.
    pub(crate) fn add_sheet_named(&self, name: &str) -> Result<Rc<Sheet>, EngineError> {
        self.check_capacity()?;
        let validated = self.validated_name(name, None)?;
        let sheet = self.make_sheet(&validated);
        self.sheets.borrow_mut().push(Rc::clone(&sheet));
        Ok(sheet)
    }

    pub(crate) fn remove_sheet(&self, index: usize) -> Result<(), EngineError> {
        {
            let mut sheets = self.sheets.borrow_mut();
            if sheets.len() <= 1 {
                return Err(EngineError::domain("a workbook needs at least one sheet"));
            }
            if index >= sheets.len() {
                return Ok(());
            }
            sheets.remove(index);
            let count = sheets.len();
            self.active_index
                .set(self.active_index.get().min(count - 1));
        }
        self.recalculate(); // formulas referencing the removed sheet error
        Ok(())
    }

    pub(crate) fn rename(&self, index: usize, new_name: &str) -> Result<(), EngineError> {
        {
            let sheets = self.sheets.borrow();
            if index >= sheets.len() {
                return Ok(());
            }
            let name = self.validated_name_against(new_name, &sheets, Some(index))?;
            *sheets[index].name.borrow_mut() = name.clone();
            sheets[index].grid.set_display_name(Some(name));
        }
        self.recalculate(); // references resolve by name
        Ok(())
    }

    fn validated_name(&self, name: &str, except: Option<usize>) -> Result<String, EngineError> {
        let sheets = self.sheets.borrow();
        self.validated_name_against(name, &sheets, except)
    }

    /// Trimmed, non-empty, ≤128 chars, unique (case-insensitive), and free
    /// of the characters that would break the `Sheet!A:1` syntax.
    fn validated_name_against(
        &self,
        name: &str,
        existing: &[Rc<Sheet>],
        except: Option<usize>,
    ) -> Result<String, EngineError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(EngineError::domain("sheet names can't be empty"));
        }
        if trimmed.chars().count() > SheetStore::MAX_NAME_LENGTH {
            return Err(EngineError::domain(format!(
                "sheet names are limited to {} characters",
                SheetStore::MAX_NAME_LENGTH
            )));
        }
        if trimmed.contains('!') || trimmed.contains('\'') {
            return Err(EngineError::domain("sheet names can't contain ! or '"));
        }
        for (i, sheet) in existing.iter().enumerate() {
            if Some(i) == except {
                continue;
            }
            if sheet.name().to_lowercase() == trimmed.to_lowercase() {
                return Err(EngineError::domain(format!(
                    "a sheet named '{trimmed}' already exists"
                )));
            }
        }
        Ok(trimmed.to_string())
    }

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
    fn mutate_update_cell(&self, arguments: &[Value]) -> Result<Value, EngineError> {
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
    fn mutate_add_worksheet(&self, arguments: &[Value]) -> Result<Value, EngineError> {
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
    fn mutate_rename_worksheet(&self, arguments: &[Value]) -> Result<Value, EngineError> {
        if arguments.len() != 2 {
            return Err(EngineError::domain(
                "renameWorksheet(sheet, newName) takes a worksheet (or name) and the new name",
            ));
        }
        let index = self.sheet_index_for_target(&arguments[0])?;
        let Value::String(new_name) = &arguments[1] else {
            return Err(EngineError::domain("renameWorksheet()'s new name is text"));
        };
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
        Ok(self.worksheet_handle(index))
    }

    /// Removes a worksheet (refuses the last one) and returns the new
    /// count. Formulas referencing the removed sheet become "unknown sheet"
    /// errors, exactly as when a tab is removed in the UI.
    fn mutate_delete_worksheet(&self, arguments: &[Value]) -> Result<Value, EngineError> {
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

pub struct SheetStore {
    pub(crate) inner: Rc<StoreInner>,
    calculator: Rc<RefCell<Calculator>>,
}

impl SheetStore {
    pub const MAX_SHEETS: usize = 256;
    pub const MAX_NAME_LENGTH: usize = 128;

    /// The mutation command names — the log-only gated set, shared so any
    /// host override gates exactly the same commands.
    pub const MUTATION_NAMES: [&'static str; 4] = [
        "updateCell",
        "addWorksheet",
        "renameWorksheet",
        "deleteWorksheet",
    ];

    pub fn new(calculator: Rc<RefCell<Calculator>>) -> Self {
        let context = ResolutionContext::new();
        let inner = Rc::new(StoreInner {
            sheets: RefCell::new(Vec::new()),
            active_index: std::cell::Cell::new(0),
            context: Rc::clone(&context),
            log_source: RefCell::new(None),
        });
        inner
            .sheets
            .borrow_mut()
            .push(Sheet::new("Sheet 1", Spreadsheet::new(Rc::clone(&context))));

        let store = Self {
            inner: Rc::clone(&inner),
            calculator: Rc::clone(&calculator),
        };
        store.install_resolvers();
        store
    }

    pub fn calculator(&self) -> &Rc<RefCell<Calculator>> {
        &self.calculator
    }

    /// Attaches the host's log for `History` reflection (log-only).
    pub fn set_log_source(&self, source: Rc<dyn crate::history_reflection::LogSource>) {
        *self.inner.log_source.borrow_mut() = Some(source);
    }

    fn install_resolvers(&self) {
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

    // MARK: Host conveniences (the outermost-borrow entry points)

    /// What the grid shows at `address` on the active sheet — the harness's
    /// and UI's read path (borrows the calculator once, here).
    pub fn display_value(&self, address: CellAddress) -> CellDisplay {
        let sheet = self.inner.active_sheet();
        self.display_value_on(&sheet, address)
    }

    pub fn display_value_on(&self, sheet: &Rc<Sheet>, address: CellAddress) -> CellDisplay {
        // A data sheet renders straight from the table — no calculator, no
        // formulas: empty → blank, parseable → a number, else the raw text
        // (headers and labels). Mirrors Swift's `SheetModel.display(at:)`.
        if let Some(data) = &*sheet.data.borrow() {
            let raw = data.raw_value(address.row, address.column);
            if raw.is_empty() {
                return CellDisplay::Empty;
            }
            return match BigDecimal::parse(&raw) {
                Some(value) => CellDisplay::Value(value),
                None => CellDisplay::Text(raw),
            };
        }
        self.calculator
            .borrow_mut()
            .host_eval(|evaluator, environment| {
                sheet.grid.display_value((evaluator, environment), address)
            })
    }

    // MARK: Sheets

    pub fn sheets(&self) -> Vec<Rc<Sheet>> {
        self.inner.sheets.borrow().clone()
    }

    pub fn active_sheet(&self) -> Rc<Sheet> {
        self.inner.active_sheet()
    }

    pub fn set_active_index(&self, index: usize) {
        let count = self.inner.sheets.borrow().len();
        self.inner
            .active_index
            .set(index.min(count.saturating_sub(1)));
    }

    pub fn sheet_named(&self, name: &str) -> Option<Rc<Sheet>> {
        self.inner.sheet_named(name)
    }

    /// Adds an auto-named empty grid sheet (the UI's +-button path).
    pub fn add_sheet(&self) -> Result<Rc<Sheet>, EngineError> {
        let mut n = self.inner.sheets.borrow().len() + 1;
        while self.sheet_named(&format!("Sheet {n}")).is_some() {
            n += 1;
        }
        self.inner.add_sheet_named(&format!("Sheet {n}"))
    }

    /// Adds an empty grid sheet with a specific (validated) name — the
    /// mutation API's `addWorksheet(name)`.
    pub fn add_sheet_named(&self, name: &str) -> Result<Rc<Sheet>, EngineError> {
        self.inner.add_sheet_named(name)
    }

    pub fn remove_sheet(&self, index: usize) -> Result<(), EngineError> {
        self.inner.remove_sheet(index)
    }

    pub fn rename(&self, index: usize, new_name: &str) -> Result<(), EngineError> {
        self.inner.rename(index, new_name)
    }

    /// Drops every sheet's memo — a log variable changed, a sheet was
    /// renamed/removed, or a workbook loaded.
    pub fn recalculate(&self) {
        self.inner.recalculate();
    }

    // MARK: Mutation seams (public — a host's undoable override reuses them)

    /// A `Worksheet` handle for the sheet at `index` — the value the
    /// mutation commands return, built identically by the engine default
    /// and any host override.
    pub fn worksheet_handle(&self, index: usize) -> Value {
        self.inner.worksheet_handle(index)
    }

    /// Resolves a CELL handle (`cell("A", 1)` / `…cell("A", 1)`) to the
    /// index of the sheet it lives on and its address — so a host can write
    /// it undoably.
    pub fn cell_target(&self, value: &Value) -> Result<(usize, CellAddress), EngineError> {
        self.inner.cell_target(value)
    }

    /// Resolves a worksheet TARGET — a `Worksheet` handle or a name string —
    /// to its current index in the workbook.
    pub fn sheet_index_for_target(&self, value: &Value) -> Result<usize, EngineError> {
        self.inner.sheet_index_for_target(value)
    }

    /// A value as a cell's raw text: numbers become digits, strings are
    /// verbatim. Structures/functions/handles can't live in a cell.
    pub fn raw_text_from(value: &Value) -> Result<String, EngineError> {
        match value {
            Value::Number(number) => Ok(number.to_string()),
            Value::String(text) => Ok(text.clone()),
            other => Err(EngineError::domain(format!(
                "a cell holds a number or text, not {}",
                other.kind_name()
            ))),
        }
    }

    /// Replaces everything (workbook open / new).
    pub fn replace_sheets(&self, new_sheets: Vec<Rc<Sheet>>, active_name: Option<&str>) {
        assert!(!new_sheets.is_empty());
        let active = new_sheets
            .iter()
            .position(|s| active_name.is_some_and(|n| s.name().to_lowercase() == n.to_lowercase()))
            .unwrap_or(0);
        *self.inner.sheets.borrow_mut() = new_sheets;
        self.inner.active_index.set(active);
        self.recalculate();
    }

    /// A fresh empty sheet built against this store's shared context — for
    /// workbook loading.
    pub fn make_sheet(&self, name: &str) -> Rc<Sheet> {
        self.inner.make_sheet(name)
    }

    /// A DataStore-backed sheet whose reads route to `data` — for CSV import
    /// and loading a workbook that carries data sheets.
    pub fn make_data_sheet(&self, name: &str, data: DataSheet) -> Rc<Sheet> {
        self.inner.make_data_sheet(name, data)
    }
}
