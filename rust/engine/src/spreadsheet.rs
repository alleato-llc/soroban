//! The spreadsheet's calculation model: sparse raw contents plus memoized
//! evaluation with formula auto-detection and cycle detection.
//!
//! Explicit markers override auto-detection:
//!  - `=…` is always a formula; every failure (even unknown names) is an
//!    error
//!  - `"…"` is always text, shown without the quotes (`"123"` stays a label)
//!
//! Auto-detect rules for everything else:
//!  1. blank → empty
//!  2. doesn't parse → text
//!  3. parses and references a cell → always a formula (errors surface)
//!  4. parses without cell refs → formula if it evaluates; on failure the
//!     error kind decides: unknown variable/function means it's a label
//!     ("Q1 revenue" parses as `Q1 * revenue`), anything else (division by
//!     zero, domain error, arity) is a formula mistake and shows the error
//!
//! Interior mutability discipline: every RefCell borrow is SHORT — never
//! held across an inner evaluation, which can re-enter this sheet's maps.
//! Evaluation methods take `(&Evaluator, &mut EvaluationEnvironment)` — the
//! re-entry context the resolvers thread through (the Rust answer to the
//! Swift side's shared-class Calculator re-entrancy).

use crate::cell::{Cell, Content, Definition, DefinitionKind};
use crate::cell_address::CellAddress;
use crate::context::{CellKey, ResolutionContext, SheetId};
use crate::controls::SliderInfo;
use anzan::{BigDecimal, EvaluationEnvironment, Evaluator};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// What the grid shows for a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellDisplay {
    Empty,
    Text(String),
    Value(BigDecimal),
    Error(String),
    /// A sheet-scoped definition — "λ tax(x)" or "𝑖 rate" (user design:
    /// definitions show a glyph, not a value; the editor shows the source).
    Definition(String),
    /// A comment-only cell (`# a note`) — the host renders it dim; it holds
    /// no value (skipped in ranges, errors on direct reference).
    Note(String),
    /// Control expressions: `slider(…)` / `rate = slider(…)` etc. — the
    /// grid draws the control; interaction rewrites the storage literal in
    /// place.
    Slider(SliderInfo),
    Stepper(SliderInfo),
    Checkbox(crate::controls::CheckboxInfo),
    Dropdown(crate::controls::DropdownInfo),
}

/// One name claimed by a definition cell on this sheet.
#[derive(Debug, Clone)]
pub struct SheetDefinition {
    /// As typed.
    pub name: String,
    pub address: CellAddress,
    pub(crate) definition: Definition,
}

/// Which of the three a definition cell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetDefinitionKind {
    Variable,
    Function,
    DataType,
}

impl SheetDefinition {
    pub fn kind(&self) -> SheetDefinitionKind {
        match self.definition.kind {
            DefinitionKind::Variable(_) => SheetDefinitionKind::Variable,
            DefinitionKind::Function { .. } => SheetDefinitionKind::Function,
            DefinitionKind::DataType { .. } => SheetDefinitionKind::DataType,
        }
    }

    /// "f(x, y)" for a λ cell, the bare name otherwise.
    pub fn signature(&self) -> String {
        if let DefinitionKind::Function { parameters, .. } = &self.definition.kind {
            return format!("{}({})", self.name, parameters.join(", "));
        }
        self.name.clone()
    }
}

/// The re-entry pair evaluation methods receive.
pub type Host<'a, 'b> = (&'a Evaluator<'a>, &'b mut EvaluationEnvironment);

pub struct Spreadsheet {
    pub(crate) id: SheetId,
    /// Cells, parsed and statically classified at commit time (see `Cell`).
    cells: RefCell<HashMap<CellAddress, Cell>>,
    /// Shared with every sheet of a SheetStore: tracks which sheet owns the
    /// formula being evaluated and detects cycles that span sheets.
    pub(crate) context: Rc<ResolutionContext>,
    /// For error messages ("circular reference involving Budget!A:1") — set
    /// by SheetStore; `None` for a standalone single sheet.
    pub(crate) display_name: RefCell<Option<String>>,
    /// Memo for the current generation; cleared by `recalculate()`.
    cache: RefCell<HashMap<CellAddress, CellDisplay>>,
    /// One name per cell, ≤64 chars, unique per sheet (case-insensitive,
    /// like sheet names). Distinct from 𝑖 definitions: a definition names a
    /// VALUE; a cell name names the cell itself, whatever it holds.
    cell_names: RefCell<HashMap<CellAddress, String>>,
    /// Name (lowercased — one case-insensitive namespace per sheet) → its
    /// canonical definition. Earliest address (row, then column) wins; the
    /// others display errors.
    definitions: RefCell<HashMap<String, SheetDefinition>>,
    /// Guards `rate = rate + 1`-style self-reference during lazy evaluation.
    resolving_definitions: RefCell<HashSet<String>>,
    /// Live drag values for slider cells: mid-drag the UI previews here and
    /// only rewrites the cell's raw on release.
    slider_overrides: RefCell<HashMap<CellAddress, BigDecimal>>,
}

impl Spreadsheet {
    pub const COLUMN_COUNT: usize = 26;
    pub const ROW_COUNT: usize = 1000;

    pub const MAX_NAME_LENGTH: usize = 64;

    pub fn new(context: Rc<ResolutionContext>) -> Rc<Self> {
        let id = context.allocate_id();
        let sheet = Rc::new(Spreadsheet {
            id,
            cells: RefCell::new(HashMap::new()),
            context: Rc::clone(&context),
            display_name: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
            cell_names: RefCell::new(HashMap::new()),
            definitions: RefCell::new(HashMap::new()),
            resolving_definitions: RefCell::new(HashSet::new()),
            slider_overrides: RefCell::new(HashMap::new()),
        });
        context.attach(id, &sheet);
        sheet
    }

    fn key(&self, address: CellAddress) -> CellKey {
        CellKey {
            sheet: self.id,
            address,
        }
    }

    pub fn display_name(&self) -> Option<String> {
        self.display_name.borrow().clone()
    }

    pub fn set_display_name(&self, name: Option<String>) {
        *self.display_name.borrow_mut() = name;
    }

    // MARK: Editing

    /// Sets (or clears, with `None`/blank) a cell's raw content. Only this
    /// cell and the formulas that (transitively) read it are recomputed —
    /// across sheets — via the dependency graph. Definition cells are MOSTLY
    /// the exception (λ/𝑫 calls leave no graph edges → invalidate
    /// everything, like a log variable change); the carve-out is a 𝑖 cell
    /// redefining the SAME variable (a slider drag commit): `defined_value`
    /// records a read edge per consumer, so its readers are exactly known —
    /// that's what keeps controls responsive on big workbooks.
    pub fn set_cell(&self, raw: Option<&str>, address: CellAddress) {
        let new = raw.and_then(Cell::new);
        let old = {
            let mut cells = self.cells.borrow_mut();
            match new {
                Some(cell) => cells.insert(address, cell),
                None => cells.remove(&address),
            }
        };
        let cells = self.cells.borrow();
        let new_ref = cells.get(&address);
        if Self::is_same_variable_redefinition(old.as_ref(), new_ref) {
            drop(cells);
            self.rebuild_definitions(); // refresh the indexed expression
            self.context.invalidate(self.key(address));
        } else if old.as_ref().is_some_and(Cell::is_definition)
            || new_ref.is_some_and(Cell::is_definition)
        {
            drop(cells);
            self.rebuild_definitions();
            self.context.invalidate_everything();
        } else {
            drop(cells);
            self.context.invalidate(self.key(address));
        }
    }

    /// Both sides define a VARIABLE with the same (case-insensitive) name.
    /// Only 𝑖 qualifies: function/data-type calls have no dependency edges,
    /// and a name change orphans readers the graph can't see.
    fn is_same_variable_redefinition(old: Option<&Cell>, new: Option<&Cell>) -> bool {
        let (Some(old), Some(new)) = (old, new) else {
            return false;
        };
        let (Content::Definition(before), Content::Definition(after)) =
            (&old.content, &new.content)
        else {
            return false;
        };
        matches!(before.kind, DefinitionKind::Variable(_))
            && matches!(after.kind, DefinitionKind::Variable(_))
            && before.name.to_lowercase() == after.name.to_lowercase()
    }

    /// Raw contents view — what persistence stores.
    pub fn raws(&self) -> HashMap<CellAddress, String> {
        self.cells
            .borrow()
            .iter()
            .map(|(a, c)| (*a, c.raw.clone()))
            .collect()
    }

    pub fn raw(&self, address: CellAddress) -> String {
        self.cells
            .borrow()
            .get(&address)
            .map(|c| c.raw.clone())
            .unwrap_or_default()
    }

    /// Replaces all contents (used when loading persisted state).
    pub fn load(&self, contents: &HashMap<CellAddress, String>) {
        *self.cells.borrow_mut() = contents
            .iter()
            .filter_map(|(a, raw)| Cell::new(raw).map(|c| (*a, c)))
            .collect();
        self.rebuild_definitions();
        self.recalculate();
    }

    /// Drops ALL memoized results, everywhere this sheet's context reaches —
    /// for changes the dependency graph can't see (variables, functions,
    /// sheet renames, workbook loads).
    pub fn recalculate(&self) {
        self.context.invalidate_everything();
    }

    pub(crate) fn clear_memo(&self, address: CellAddress) {
        self.cache.borrow_mut().remove(&address);
    }

    pub(crate) fn clear_all_memo(&self) {
        self.cache.borrow_mut().clear();
    }
}

/// Named cells, sheet-scoped definitions, and slider previews.
mod definitions;
/// Cell evaluation and the numeric read paths.
mod evaluation;
