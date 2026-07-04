//! The calculator session — the engine-facing half of the app, kept free of
//! any iced/rendering concern. It owns the shared [`Calculator`] (variables,
//! `ans`, user functions), the [`SheetStore`] wired to it (so log lines can
//! reference cells and mutate the grid), the log tape, and the ↑/↓ input
//! history. The Rust counterpart to the Swift app's `CalculatorSession`; the
//! iced `State` in `main.rs` is a thin shell over it.

use soroban_engine::named_cells::NamedCells;
use soroban_engine::spreadsheet::SheetDefinitionKind;
use soroban_engine::{
    BigDecimal, BinaryView, BinaryViewUnavailable, Calculator, CellAddress, CellDisplay,
    CellFormat, Control, EvalOutcome, Sheet, SheetStore, Spreadsheet, Value,
};
use std::cell::RefCell;
use std::rc::Rc;

/// One inspector row: a name or signature, and a short detail (a value, or the
/// definition's kind and cell). The gui renders these grouped into sections.
pub struct InspectorRow {
    pub label: String,
    pub detail: String,
}

/// One reference-window entry: a function/operator signature and its summary.
pub struct DocEntry {
    pub signature: String,
    pub summary: String,
}

/// A titled group of reference entries (Special Forms, a registry category,
/// Operators, Constants, or the user's own functions/types).
pub struct DocGroup {
    pub title: String,
    pub entries: Vec<DocEntry>,
}

/// The binary bit-editor's state for the current draft: an editable bit grid,
/// or a reason the last result can't be edited (a decimal, a negative, …).
pub enum BinaryStatus {
    Editable {
        /// LSB-first (bit 0 is `bits[0]`).
        bits: Vec<bool>,
        /// The value's re-parseable text (`42`, `Int32(255)`).
        value: String,
        width: u32,
        signed: bool,
    },
    Unavailable(String),
}

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

/// One undoable step: a group of cell-content changes, or a single cell's
/// format change (display-only — no recalc). The Swift `SheetEdit.Kind`
/// analogue (`.cells` / `.formats`).
enum Edit {
    Cells(Vec<CellChange>),
    Format {
        address: CellAddress,
        old: CellFormat,
        new: CellFormat,
    },
    /// A cell's name changed, plus any reference rewrites a rename triggered
    /// (empty for a plain set/clear). Undo restores the old name and raws.
    Name {
        address: CellAddress,
        old_name: Option<String>,
        new_name: Option<String>,
        cell_changes: Vec<CellChange>,
    },
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
    /// Undoable steps, most recent last (cell content or cell format).
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,
    /// The binary bit-editor's current draft (a flip stages a new one); `None`
    /// when closed or when `ans` isn't editable.
    binary: Option<BinaryView>,
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
            binary: None,
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
    ///
    /// The live display of one cell by address (values, errors, controls).
    pub fn display_at(&self, address: CellAddress) -> CellDisplay {
        let sheet: Rc<Sheet> = self.store.active_sheet();
        self.store.display_value_on(&sheet, address)
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
        self.push_edit(Edit::Cells(changes));
    }

    /// Record one undoable step, capping the stack and clearing redo.
    fn push_edit(&mut self, edit: Edit) {
        self.undo_stack.push(edit);
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Apply one side of an edit — `forward` for the "new" state (do/redo),
    /// else the "old" state (undo). Cell content recalculates; a format change
    /// is display-only.
    fn apply_side(&self, edit: &Edit, forward: bool) {
        match edit {
            Edit::Cells(changes) => {
                for change in changes {
                    let raw = if forward { &change.new } else { &change.old };
                    self.write_raw(change.address, raw);
                }
                self.store.recalculate();
            }
            Edit::Format { address, old, new } => {
                self.write_format(*address, if forward { new } else { old });
            }
            Edit::Name {
                address,
                old_name,
                new_name,
                cell_changes,
            } => {
                let grid = self.store.active_sheet().grid.clone();
                let name = if forward { new_name } else { old_name };
                // A later edit may have claimed the name; skip on failure
                // rather than crash (the Swift `try? setCellName` rule).
                let _ = grid.set_cell_name(name.as_deref(), *address);
                for change in cell_changes {
                    let raw = if forward { &change.new } else { &change.old };
                    self.write_raw(change.address, raw);
                }
                self.store.recalculate();
            }
        }
    }

    /// Low-level cell write (empty string clears the cell); no undo bookkeeping.
    fn write_raw(&self, address: CellAddress, raw: &str) {
        let grid = self.store.active_sheet().grid.clone();
        grid.set_cell(if raw.is_empty() { None } else { Some(raw) }, address);
    }

    /// Undo the most recent step.
    pub fn undo(&mut self) {
        if let Some(edit) = self.undo_stack.pop() {
            self.apply_side(&edit, false);
            self.redo_stack.push(edit);
        }
    }

    /// Redo the most recently undone step.
    pub fn redo(&mut self) {
        if let Some(edit) = self.redo_stack.pop() {
            self.apply_side(&edit, true);
            self.undo_stack.push(edit);
        }
    }

    // MARK: Formats (slice ④)

    /// The format applied to a cell (the default when none is set).
    pub fn cell_format(&self, address: CellAddress) -> CellFormat {
        self.store
            .active_sheet()
            .formats
            .borrow()
            .get(&address)
            .cloned()
            .unwrap_or_default()
    }

    /// Set a cell's format as an undoable step. Formats are display-only, so
    /// there's no recalc; a default format is pruned from the sparse map.
    pub fn apply_format(&mut self, address: CellAddress, new: CellFormat) {
        let old = self.cell_format(address);
        if old == new {
            return;
        }
        self.write_format(address, &new);
        self.push_edit(Edit::Format { address, old, new });
    }

    /// Low-level format write; default formats are removed (the sparse-map rule).
    fn write_format(&self, address: CellAddress, format: &CellFormat) {
        let sheet = self.store.active_sheet();
        let mut formats = sheet.formats.borrow_mut();
        if format.is_default() {
            formats.remove(&address);
        } else {
            formats.insert(address, format.clone());
        }
    }

    // MARK: Named cells (slice ④)

    /// The name given to a cell location, if any (`'Projected Rate'`).
    pub fn cell_name(&self, address: CellAddress) -> Option<String> {
        self.store
            .active_sheet()
            .grid
            .cell_names()
            .into_iter()
            .find(|(a, _)| *a == address)
            .map(|(_, name)| name)
    }

    /// Name a cell (empty clears the name). A rename — replacing an existing
    /// name with a new one — rewrites every `'Old'` reference to `'New'` across
    /// the sheet, all as one undoable step. Returns the engine's validation
    /// error (duplicate/too long/illegal character) so the caller can revert.
    pub fn set_cell_name(&mut self, address: CellAddress, name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        let old_name = self.cell_name(address);
        let new_name = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if old_name == new_name {
            return Ok(());
        }

        let grid = self.store.active_sheet().grid.clone();
        // Validate + apply the name change first; a duplicate name errors here
        // before any references are touched.
        grid.set_cell_name(new_name.as_deref(), address)
            .map_err(|error| error.to_string())?;

        // On a rename, rewrite references `'Old'` → `'New'` in every cell.
        let cell_changes = match (&old_name, &new_name) {
            (Some(old), Some(new)) => self.rename_references(old, new),
            _ => Vec::new(),
        };
        for change in &cell_changes {
            self.write_raw(change.address, &change.new);
        }
        self.store.recalculate();
        self.push_edit(Edit::Name {
            address,
            old_name,
            new_name,
            cell_changes,
        });
        Ok(())
    }

    // MARK: Inspector (slice ⑤)

    /// Live variables: log-defined user variables (`name = value`) and the
    /// active sheet's 𝑖 definitions, sorted case-insensitively.
    pub fn inspector_variables(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_variables()
                .iter()
                .map(|(name, value)| InspectorRow {
                    label: name.clone(),
                    detail: value.display_description(),
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::Variable) {
            rows.push(InspectorRow {
                label: definition.name,
                detail: format!("𝑖 {}", definition.address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    /// Named cell locations, each with its address and current value.
    pub fn inspector_named_cells(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = self
            .store
            .active_sheet()
            .grid
            .cell_names()
            .into_iter()
            .map(|(address, name)| {
                let detail = match self.display_at(address) {
                    CellDisplay::Value(number) => format!("{address} = {number}"),
                    _ => address.to_string(),
                };
                InspectorRow {
                    label: format!("'{name}'"),
                    detail,
                }
            })
            .collect();
        sort_rows(&mut rows);
        rows
    }

    /// User functions: log-defined signatures and the sheet's λ definitions.
    pub fn inspector_functions(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_functions()
                .values()
                .map(|function| InspectorRow {
                    label: function.signature(),
                    detail: function.documentation().unwrap_or_default(),
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::Function) {
            rows.push(InspectorRow {
                label: definition.signature(),
                detail: format!("λ {}", definition.address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    /// Declared data types: log-defined and the sheet's 𝑫 definitions.
    pub fn inspector_data_types(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_data_types()
                .values()
                .map(|data_type| InspectorRow {
                    label: data_type.name.clone(),
                    detail: String::new(),
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::DataType) {
            rows.push(InspectorRow {
                label: definition.name,
                detail: format!("𝑫 {}", definition.address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    // MARK: Reference window (slice ⑤)

    /// The reference documentation, filtered by `query` (matched against each
    /// entry's signature and summary, case-insensitively). Empty query returns
    /// everything; categories with no surviving entries are dropped. Includes
    /// the user's own functions and data types first (via `Calculator`).
    pub fn reference(&self, query: &str) -> Vec<DocGroup> {
        let needle = query.trim().to_lowercase();
        self.calculator
            .borrow()
            .documentation()
            .into_iter()
            .filter_map(|category| {
                let entries: Vec<DocEntry> = category
                    .entries
                    .into_iter()
                    .filter(|entry| {
                        needle.is_empty()
                            || entry.name.to_lowercase().contains(&needle)
                            || entry.signature.to_lowercase().contains(&needle)
                            || entry.summary.to_lowercase().contains(&needle)
                    })
                    .map(|entry| DocEntry {
                        signature: entry.signature,
                        summary: entry.summary,
                    })
                    .collect();
                (!entries.is_empty()).then_some(DocGroup {
                    title: category.title,
                    entries,
                })
            })
            .collect()
    }

    // MARK: Binary editor (slice ⑤)

    /// The last computed result — the value the bit editor edits.
    fn ans(&self) -> Value {
        self.calculator.borrow().environment().ans()
    }

    /// (Re)build the bit-editor draft from `ans`. Called when the editor opens
    /// and after each submit, so it tracks the latest result until you flip a
    /// bit (which stages a draft of its own).
    pub fn refresh_binary(&mut self) {
        self.binary = BinaryView::make(&self.ans(), 32).ok();
    }

    /// Flip bit `index` (0 = LSB) of the draft, staging a new pattern.
    pub fn flip_binary_bit(&mut self, index: usize) {
        if let Some(view) = &self.binary {
            if (index as u32) < view.width {
                self.binary = Some(view.flipping_bit(index as u32));
            }
        }
    }

    /// The editor's current state: the editable grid, or why `ans` can't be
    /// edited as bits.
    pub fn binary_status(&self) -> BinaryStatus {
        if let Some(view) = &self.binary {
            return BinaryStatus::Editable {
                bits: view.bits(),
                value: view.value().display_description(),
                width: view.width,
                signed: view.signed(),
            };
        }
        let reason = match BinaryView::make(&self.ans(), 32) {
            Ok(_) => "Compute a value, then open the bit editor.".to_string(),
            Err(reason) => binary_reason(reason),
        };
        BinaryStatus::Unavailable(reason)
    }

    /// Drop the draft's value into the input line, ready to fold into an
    /// expression (the SpeedCrunch "Use" action).
    pub fn use_binary(&mut self) {
        if let Some(view) = &self.binary {
            self.input = view.value().display_description();
            self.history_cursor = None;
        }
    }

    /// The active sheet's definition cells of one kind (name + address, sorted
    /// later by the caller). Kept private — the gui reads the four groups.
    fn active_definitions(
        &self,
        kind: SheetDefinitionKind,
    ) -> Vec<soroban_engine::spreadsheet::SheetDefinition> {
        self.store
            .active_sheet()
            .grid
            .definitions()
            .into_values()
            .filter(|definition| definition.kind() == kind)
            .collect()
    }

    /// The reference rewrites a rename triggers: every cell whose raw mentions
    /// `'old'` gets it respelled to `'new'` (token-precise, spacing preserved).
    fn rename_references(&self, old: &str, new: &str) -> Vec<CellChange> {
        let sheet = self.store.active_sheet();
        let sheet_name = sheet.name();
        let replacement = format!("'{new}'");
        let mut changes = Vec::new();
        for (address, raw) in sheet.grid.raws() {
            if let Some(new_raw) =
                NamedCells::rewriting(&raw, old, Some(&sheet_name), true, &replacement)
            {
                changes.push(CellChange {
                    address,
                    old: raw,
                    new: new_raw,
                });
            }
        }
        changes
    }

    // MARK: Controls (slice ④)

    /// Rewrite a control cell's storage literal in place and commit it as an
    /// undoable edit. `Control::rewriting` preserves spacing, the 𝑖 name, and
    /// any trailing `# comment`. A no-op when the cell isn't a control.
    fn rewrite_control(&mut self, address: CellAddress, literal: &str) {
        let raw = self.cell_raw(address);
        if let Some(new_raw) = Control::rewriting(&raw, literal) {
            self.set_cell_raw(address, &new_raw);
        }
    }

    /// Flip a checkbox cell's stored `true`/`false`.
    pub fn toggle_checkbox(&mut self, address: CellAddress) {
        if let CellDisplay::Checkbox(info) = self.display_at(address) {
            self.rewrite_control(address, if info.is_on { "false" } else { "true" });
        }
    }

    /// Select a dropdown option by index, rewriting to its literal source.
    pub fn set_dropdown_index(&mut self, address: CellAddress, index: usize) {
        if let CellDisplay::Dropdown(info) = self.display_at(address) {
            if let Some(option) = info.options.get(index) {
                let literal = option_literal(option);
                self.rewrite_control(address, &literal);
            }
        }
    }

    /// Set a slider to the value nearest `target` on its step lattice, exactly
    /// (the position comes from the drag as `f64`; the stored value is snapped
    /// in `BigDecimal` so it stays a clean multiple of the step).
    pub fn set_slider(&mut self, address: CellAddress, target: f64) {
        if let CellDisplay::Slider(info) = self.display_at(address) {
            let minimum = info.minimum.to_f64();
            let step = info.step.to_f64();
            let value = if step > 0.0 {
                let steps = ((target - minimum) / step).round().max(0.0);
                let count = BigDecimal::from_f64(steps).unwrap_or_else(BigDecimal::zero);
                &info.minimum + &(&info.step * &count)
            } else {
                info.value.clone()
            };
            let value = clamp(value, &info.minimum, &info.maximum);
            self.rewrite_control(address, &value.to_string());
        }
    }

    /// Nudge a stepper (or slider) by one step, clamped to its range.
    pub fn step_control(&mut self, address: CellAddress, up: bool) {
        let info = match self.display_at(address) {
            CellDisplay::Stepper(info) | CellDisplay::Slider(info) => info,
            _ => return,
        };
        let delta = if up { info.step.clone() } else { -&info.step };
        let next = clamp(&info.value + &delta, &info.minimum, &info.maximum);
        self.rewrite_control(address, &next.to_string());
    }
}

/// A dropdown option's re-parseable literal source: numbers as-is, strings
/// quoted with the language's `\" \\ \n \t` escapes.
fn option_literal(value: &Value) -> String {
    match value {
        Value::String(text) => {
            let mut out = String::with_capacity(text.len() + 2);
            out.push('"');
            for character in text.chars() {
                match character {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    other => out.push(other),
                }
            }
            out.push('"');
            out
        }
        other => other.to_string(),
    }
}

/// Clamp a value into `[minimum, maximum]` (exact, `BigDecimal` ordering).
/// A human explanation of why a value can't be edited as bits.
fn binary_reason(reason: BinaryViewUnavailable) -> String {
    match reason {
        BinaryViewUnavailable::NotAnInteger => "The bit editor needs a whole number.".to_string(),
        BinaryViewUnavailable::Negative => {
            "Negative — wrap it in a signed Int type (e.g. Int32).".to_string()
        }
        BinaryViewUnavailable::TooWide => "Too wide — over 256 bits.".to_string(),
    }
}

/// Sort inspector rows case-insensitively by label (the reading order the
/// Swift inspector uses).
fn sort_rows(rows: &mut [InspectorRow]) {
    rows.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
}

fn clamp(value: BigDecimal, minimum: &BigDecimal, maximum: &BigDecimal) -> BigDecimal {
    if value < *minimum {
        minimum.clone()
    } else if value > *maximum {
        maximum.clone()
    } else {
        value
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
