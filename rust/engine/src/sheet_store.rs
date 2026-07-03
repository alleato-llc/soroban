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
use crate::spreadsheet::{CellDisplay, Spreadsheet};
use anzan::{Calculator, EngineError};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// One worksheet: a calculation grid plus its name and layout. (Data sheets
/// — DataStore-backed tables — arrive with the persistence pass.)
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
        })
    }

    pub fn name(&self) -> String {
        self.name.borrow().clone()
    }
}

pub(crate) struct StoreInner {
    pub(crate) sheets: RefCell<Vec<Rc<Sheet>>>,
    pub(crate) active_index: std::cell::Cell<usize>,
    pub(crate) context: Rc<ResolutionContext>,
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
}

pub struct SheetStore {
    pub(crate) inner: Rc<StoreInner>,
    calculator: Rc<RefCell<Calculator>>,
}

impl SheetStore {
    pub const MAX_SHEETS: usize = 256;
    pub const MAX_NAME_LENGTH: usize = 128;

    pub fn new(calculator: Rc<RefCell<Calculator>>) -> Self {
        let context = ResolutionContext::new();
        let inner = Rc::new(StoreInner {
            sheets: RefCell::new(Vec::new()),
            active_index: std::cell::Cell::new(0),
            context: Rc::clone(&context),
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
        calculator.resolvers.host_value = Some(Box::new(move |name, _in_log| {
            let inner = weak.upgrade()?;
            match name {
                "Workbook" => Some(anzan::Value::Host(Rc::new(
                    crate::reflection::WorkbookObject {
                        store: Rc::downgrade(&inner),
                    },
                ))),
                // `History` is log-only and arrives with the LogSource seam.
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
    }

    // MARK: Host conveniences (the outermost-borrow entry points)

    /// What the grid shows at `address` on the active sheet — the harness's
    /// and UI's read path (borrows the calculator once, here).
    pub fn display_value(&self, address: CellAddress) -> CellDisplay {
        let sheet = self.inner.active_sheet();
        self.calculator
            .borrow_mut()
            .host_eval(|evaluator, environment| {
                sheet.grid.display_value((evaluator, environment), address)
            })
    }

    pub fn display_value_on(&self, sheet: &Rc<Sheet>, address: CellAddress) -> CellDisplay {
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
        self.check_capacity()?;
        let mut n = self.inner.sheets.borrow().len() + 1;
        while self.sheet_named(&format!("Sheet {n}")).is_some() {
            n += 1;
        }
        let sheet = self.make_sheet(&format!("Sheet {n}"));
        self.inner.sheets.borrow_mut().push(Rc::clone(&sheet));
        Ok(sheet)
    }

    /// Adds an empty grid sheet with a specific (validated) name — the
    /// mutation API's `addWorksheet(name)`.
    pub fn add_sheet_named(&self, name: &str) -> Result<Rc<Sheet>, EngineError> {
        self.check_capacity()?;
        let validated = self.validated_name(name, None)?;
        let sheet = self.make_sheet(&validated);
        self.inner.sheets.borrow_mut().push(Rc::clone(&sheet));
        Ok(sheet)
    }

    fn check_capacity(&self) -> Result<(), EngineError> {
        if self.inner.sheets.borrow().len() >= Self::MAX_SHEETS {
            return Err(EngineError::domain(format!(
                "a workbook holds at most {} sheets",
                Self::MAX_SHEETS
            )));
        }
        Ok(())
    }

    pub fn remove_sheet(&self, index: usize) -> Result<(), EngineError> {
        {
            let mut sheets = self.inner.sheets.borrow_mut();
            if sheets.len() <= 1 {
                return Err(EngineError::domain("a workbook needs at least one sheet"));
            }
            if index >= sheets.len() {
                return Ok(());
            }
            sheets.remove(index);
            let count = sheets.len();
            self.inner
                .active_index
                .set(self.inner.active_index.get().min(count - 1));
        }
        self.recalculate(); // formulas referencing the removed sheet error
        Ok(())
    }

    pub fn rename(&self, index: usize, new_name: &str) -> Result<(), EngineError> {
        {
            let sheets = self.inner.sheets.borrow();
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
        let sheets = self.inner.sheets.borrow();
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
        if trimmed.chars().count() > Self::MAX_NAME_LENGTH {
            return Err(EngineError::domain(format!(
                "sheet names are limited to {} characters",
                Self::MAX_NAME_LENGTH
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

    /// Drops every sheet's memo — a log variable changed, a sheet was
    /// renamed/removed, or a workbook loaded.
    pub fn recalculate(&self) {
        for sheet in self.inner.sheets.borrow().iter() {
            sheet.grid.recalculate();
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
        Sheet::new(name, Spreadsheet::new(Rc::clone(&self.inner.context)))
    }
}
