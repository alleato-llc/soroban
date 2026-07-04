//! The calculator session — the engine-facing half of the app, kept free of
//! any iced/rendering concern. It owns the shared [`Calculator`] (variables,
//! `ans`, user functions), the [`SheetStore`] wired to it (so log lines can
//! reference cells and mutate the grid), the log tape, and the ↑/↓ input
//! history. The Rust counterpart to the Swift app's `CalculatorSession`; the
//! iced `State` in `main.rs` is a thin shell over it.

use soroban_engine::{
    Calculator, CellAddress, CellDisplay, EvalOutcome, Sheet, SheetStore, Spreadsheet,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The grid's fixed logical size (the engine's sheet bounds).
pub const GRID_ROWS: usize = Spreadsheet::ROW_COUNT;
pub const GRID_COLS: usize = Spreadsheet::COLUMN_COUNT;

/// One line of the log: what was typed and what it produced.
pub struct LogEntry {
    pub input: String,
    pub outcome: Outcome,
}

/// The displayable result of one submission — the log renders each kind
/// differently (a value, a definition, a note, a documentation block, an
/// error with an optional caret column).
pub enum Outcome {
    /// `= 42` — a computed value, at full precision.
    Value(String),
    /// `λ f(x)` — a function was defined.
    Function(String),
    /// `𝑫 Point` — a data type was declared.
    Data(String),
    /// `# note` — a standalone comment line.
    Comment(String),
    /// A multi-line display block (pretty JSON, `man` output) — shown raw.
    Info(String),
    /// A failed evaluation: the message and, when the engine gives one, the
    /// 0-based column for a caret under the input.
    Error {
        message: String,
        position: Option<usize>,
    },
}

/// One cell's raw content before and after an edit — the unit of undo. An
/// empty string means the cell was (or becomes) blank.
#[derive(Clone)]
pub struct CellChange {
    pub address: CellAddress,
    pub old: String,
    pub new: String,
}

/// The undo/redo stacks are capped like the Swift `SheetModel` (grid content
/// only — the log is history, not document state).
const MAX_UNDO: usize = 100;

pub struct Session {
    calculator: Rc<RefCell<Calculator>>,
    store: SheetStore,
    entries: Vec<LogEntry>,
    input: String,
    /// Submitted lines, oldest first — the ↑/↓ recall tape.
    history: Vec<String>,
    /// Where ↑/↓ recall currently sits, or `None` when the field holds live
    /// typing rather than a recalled line.
    history_cursor: Option<usize>,
    /// Grouped cell edits, most recent last; each entry is one undoable step.
    undo_stack: Vec<Vec<CellChange>>,
    redo_stack: Vec<Vec<CellChange>>,
}

impl Session {
    pub fn new() -> Self {
        // The log and the grid share one calculator: variables defined in the
        // log are visible in cells, and cell references resolve from the log.
        let calculator = Rc::new(RefCell::new(Calculator::new()));
        let store = SheetStore::new(Rc::clone(&calculator));
        Self {
            calculator,
            store,
            entries: Vec::new(),
            input: String::new(),
            history: Vec::new(),
            history_cursor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    // MARK: Log

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    /// Live typing — breaks any in-progress history recall.
    pub fn set_input(&mut self, text: String) {
        self.input = text;
        self.history_cursor = None;
    }

    /// Evaluate the current line, append it to the log, and record it in the
    /// history tape. A blank line is ignored.
    pub fn submit(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
        let outcome = self.evaluate(&line);
        self.entries.push(LogEntry {
            input: line.clone(),
            outcome,
        });
        // Don't stack consecutive duplicates in the recall tape.
        if self.history.last() != Some(&line) {
            self.history.push(line);
        }
        self.input.clear();
        self.history_cursor = None;
    }

    fn evaluate(&self, line: &str) -> Outcome {
        let result = self.calculator.borrow_mut().evaluate(line);
        match result {
            Ok(outcome) => {
                // Multi-line results (pretty JSON, man pages) render raw.
                if let Some(block) = outcome.raw_block() {
                    return Outcome::Info(block.to_string());
                }
                match &outcome {
                    EvalOutcome::Value(value) => Outcome::Value(value.display_description()),
                    EvalOutcome::FunctionDefined { signature } => {
                        Outcome::Function(signature.clone())
                    }
                    EvalOutcome::DataDefined { declaration } => Outcome::Data(declaration.clone()),
                    EvalOutcome::Comment(text) => Outcome::Comment(text.clone()),
                    EvalOutcome::Documentation(_) => Outcome::Info(format!("{outcome}")),
                }
            }
            Err(error) => Outcome::Error {
                message: error.to_string(),
                position: error.position(),
            },
        }
    }

    /// ↑ — recall an older line (or the newest, on first press).
    pub fn recall_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = match self.history_cursor {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(current) => current - 1,
        };
        self.history_cursor = Some(index);
        self.input = self.history[index].clone();
    }

    /// ↓ — walk back toward the newest line, then to an empty field.
    pub fn recall_next(&mut self) {
        match self.history_cursor {
            Some(current) if current + 1 < self.history.len() => {
                self.history_cursor = Some(current + 1);
                self.input = self.history[current + 1].clone();
            }
            Some(_) => {
                // Past the newest recalled line — return to an empty field.
                self.history_cursor = None;
                self.input.clear();
            }
            None => {}
        }
    }

    // MARK: Grid (read-only in slice ②)

    /// The active sheet's name — shown on the grid tab.
    pub fn active_sheet_name(&self) -> String {
        self.store.active_sheet().name()
    }

    /// How one cell computes right now. Reads route through the ordinary
    /// dependency-tracked path, so this reflects the live values. Uses
    /// interior mutability, hence `&self`.
    pub fn cell_display(&self, row: usize, col: usize) -> CellDisplay {
        let sheet: Rc<Sheet> = self.store.active_sheet();
        self.store
            .display_value_on(&sheet, CellAddress::new(col, row))
    }

    // MARK: Editing (slice ③)

    /// The raw (unevaluated) text stored in a cell — what the edit bar shows.
    pub fn cell_raw(&self, address: CellAddress) -> String {
        self.store.active_sheet().grid.raw(address)
    }

    /// Would a leading operator complete this draft? True means the draft
    /// "expects an operand", so a cell click inserts a reference (point mode)
    /// rather than committing. Mirrors the Swift `Calculator.expectsOperand`.
    pub fn expects_operand(&self, draft: &str) -> bool {
        Calculator::expects_operand(draft)
    }

    /// Commit one cell's raw content as an undoable edit, then recalculate.
    /// A no-op when the content is unchanged.
    pub fn set_cell_raw(&mut self, address: CellAddress, raw: &str) {
        let old = self.cell_raw(address);
        if old == raw {
            return;
        }
        self.apply_edit(vec![CellChange {
            address,
            old,
            new: raw.to_string(),
        }]);
    }

    /// Apply a group of cell changes as one undo step (route every mutation
    /// through here so it stays undoable — the Swift `applyEdit` rule).
    fn apply_edit(&mut self, changes: Vec<CellChange>) {
        for change in &changes {
            self.write_raw(change.address, &change.new);
        }
        self.store.recalculate();
        self.undo_stack.push(changes);
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Low-level cell write (empty string clears the cell); no undo bookkeeping.
    fn write_raw(&self, address: CellAddress, raw: &str) {
        let grid = self.store.active_sheet().grid.clone();
        grid.set_cell(if raw.is_empty() { None } else { Some(raw) }, address);
    }

    /// Undo the most recent edit group, restoring each cell's prior raw.
    pub fn undo(&mut self) {
        if let Some(changes) = self.undo_stack.pop() {
            for change in &changes {
                self.write_raw(change.address, &change.old);
            }
            self.store.recalculate();
            self.redo_stack.push(changes);
        }
    }

    /// Redo the most recently undone edit group.
    pub fn redo(&mut self) {
        if let Some(changes) = self.redo_stack.pop() {
            for change in &changes {
                self.write_raw(change.address, &change.new);
            }
            self.store.recalculate();
            self.undo_stack.push(changes);
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
