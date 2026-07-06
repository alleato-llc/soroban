//! The calculator session — the engine-facing half of the app, kept free of
//! any iced/rendering concern. It owns the shared [`Calculator`] (variables,
//! `ans`, user functions), the [`SheetStore`] wired to it (so log lines can
//! reference cells and mutate the grid), the log tape, and the ↑/↓ input
//! history. The Rust counterpart to the Swift app's `CalculatorSession`; the
//! iced `State` in `main.rs` is a thin shell over it.

use serde::{Deserialize, Serialize};
use soroban_engine::named_cells::NamedCells;
use soroban_engine::spreadsheet::SheetDefinitionKind;
use soroban_engine::workbook::{restore_session, SheetPayload, Workbook};
use soroban_engine::{
    csv, package, BigDecimal, BinaryEditorPresets, BinaryFieldSpec, BinaryView,
    BinaryViewUnavailable, Calculator, CellAddress, CellDisplay, CellFormat, Completion,
    CompletionKind, Control, DataSheet, DataStore, EvalOutcome, FormatBuilder, LanguageMode, Sheet,
    SheetStore, Spreadsheet, UserFunction, Value, BINARY_EDITABLE_WIDTHS,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Where an inspector entry comes from — the calculation log, or a cell (which
/// the gui renders as a clickable `B:2 ↗` tag that jumps to it).
#[derive(Clone, Copy)]
pub enum Origin {
    Log,
    Cell(CellAddress),
}

/// One inspector row: a name or signature, a short detail (its value or doc),
/// and its provenance. The gui renders these grouped into sections.
pub struct InspectorRow {
    pub label: String,
    pub detail: String,
    pub origin: Origin,
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
        /// LSB-first — `bits[0]` is bit 0 (the LSB), matching the rime
        /// `bit_grid` widget's contract and `flip_binary_bit`'s indexing.
        bits: Vec<bool>,
        /// The value's re-parseable text (`42`, `Int32(255)`).
        value: String,
        /// The register's bit pattern in hex (`0x1F4`), the header annotation.
        hex: String,
        width: u32,
        signed: bool,
        /// A fixed-width int is locked to its own width — the shell hides the
        /// width picker (a plain register is free to change width).
        locked: bool,
    },
    Unavailable(String),
}

/// One selectable register width in the bit-editor's width picker. `enabled`
/// is false for a width too small to hold the current value (or the active
/// format); `active` marks the width in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryWidth {
    pub bits: u32,
    pub enabled: bool,
    pub active: bool,
}

/// How a bit-format field is edited — the shell picks its widget from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFieldKind {
    /// A plain integer field — edited by typing its value (in its base).
    Numeric,
    /// Per-bit named flags (`r w x`) — edited by toggling individual bits.
    Flags,
    /// An unsigned value indexing a label list — edited with a picker.
    Enum,
    /// A locked, must-be-zero gap — not editable.
    Reserved,
    /// A don't-care gap — editable bit-by-bit, but unlabeled.
    Unused,
}

/// One bit of a flags field, flattened for the shell: its name (`r`), its
/// absolute register bit (so a click routes to `flip_binary_bit`), and whether
/// it's set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryFlagBit {
    pub name: String,
    /// Absolute bit index in the register (0 = LSB).
    pub bit: u32,
    pub set: bool,
}

/// One decoded field of the active bit-format, flattened for the shell (no
/// `BigInt` leaks): the named range, its palette color name, the decoded
/// readout, and everything the shell needs to render the right editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryFieldView {
    pub name: String,
    /// 0 = LSB — where the field sits in the register.
    pub low_bit: u32,
    pub width: u32,
    /// The field's palette color NAME (`blue`…`teal`), or `None` for auto —
    /// the shell maps it to a real, theme-adapting color.
    pub color: Option<String>,
    /// The human-readable decode: a flag string (`r-x`), an enum label, or the
    /// numeric value spelled in its base.
    pub label: String,
    /// Which editor the field takes.
    pub kind: BinaryFieldKind,
    /// The field's numeric value spelled in its display base (`0x1b`, `755`) —
    /// the editable text for a numeric field.
    pub value_text: String,
    /// Enum labels for a picker (empty unless `kind == Enum`).
    pub options: Vec<String>,
    /// The selected enum index, when the value is in range (else `None` — an
    /// out-of-range enum shows its raw number and no selection).
    pub selected: Option<usize>,
    /// A flags field's per-bit detail, high→low (empty unless `kind == Flags`).
    pub flags: Vec<BinaryFlagBit>,
    /// A locked, must-be-zero gap (display only).
    pub reserved: bool,
    /// A don't-care gap (unlabeled but editable).
    pub unused: bool,
}

/// The grid's fixed logical size (the engine's sheet bounds).
pub const GRID_ROWS: usize = Spreadsheet::ROW_COUNT;
pub const GRID_COLS: usize = Spreadsheet::COLUMN_COUNT;

/// One line of the log: what was typed and what it produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub input: String,
    pub outcome: Outcome,
}

/// The displayable result of one submission — the log renders each kind
/// differently (a value, a definition, a note, a documentation block, an
/// error with an optional caret column).
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Feeds the gui's live log tape to the engine's `History` reflection (so a
/// log-line `last(History).value` / `len(History)` reflects what came before).
/// Holds the shared tape and converts each [`LogEntry`] to a host-neutral
/// `LogRecord` on demand — the engine derives `kind`/`referencesCells` from the
/// input parse itself.
struct LogTape(Rc<RefCell<Vec<LogEntry>>>);

impl soroban_engine::history_reflection::LogSource for LogTape {
    fn records(&self) -> Vec<soroban_engine::history_reflection::LogRecord> {
        self.0.borrow().iter().map(log_record).collect()
    }
}

/// Convert one log entry to the engine's host-neutral record. `value` is the
/// re-parsed typed result for value lines (lossless for numbers/strings, like
/// the Swift `Value(parsing:)`); the flag trio classifies error/comment/info.
fn log_record(entry: &LogEntry) -> soroban_engine::history_reflection::LogRecord {
    use soroban_engine::history_reflection::LogRecord;
    let (text, value, is_error, is_comment, is_info) = match &entry.outcome {
        Outcome::Value(s) => (s.clone(), Value::parsing(s), false, false, false),
        Outcome::Function(sig) => (sig.clone(), None, false, false, false),
        Outcome::Data(decl) => (decl.clone(), None, false, false, false),
        Outcome::Comment(note) => (note.clone(), None, false, true, false),
        Outcome::Info(block) => (block.clone(), None, false, false, true),
        Outcome::Error { message, .. } => (message.clone(), None, true, false, false),
    };
    LogRecord {
        input: entry.input.clone(),
        text,
        value,
        is_error,
        is_comment,
        is_info,
        note: String::new(),
    }
}

/// What a grid click means while a cell editor is open — Excel's point mode.
#[derive(Debug, PartialEq, Eq)]
pub enum PointClick {
    /// The draft ended "expecting an operand", so the clicked cell's reference
    /// was spliced in; the caller keeps editing with this new draft.
    Inserted(String),
    /// The draft was already a complete value — the caller should commit the
    /// edit and move the selection to the clicked cell (Excel behavior).
    Commit,
}

/// Point mode's memory of its last reference splice — the state behind Excel's
/// re-click-replaces and shift-click-extends-to-a-range. Mirrors the Swift
/// `SheetModel`'s `pointModeExpectedDraft` / `lastInsertedReference` /
/// `lastInsertedAddress` trio.
#[derive(Clone)]
struct PointAnchor {
    /// The draft exactly as our last splice left it. If the live draft still
    /// equals this, the user hasn't typed since — so the next click replaces
    /// (or, with shift, extends) the reference instead of appending another.
    draft: String,
    /// The exact text we last spliced in (`B:1`, `'Rate'`, or a `B:1..B:4`
    /// range) — what a re-click peels back off before writing the new one.
    reference: String,
    /// That reference's cell, so a shift-click can widen it into a range.
    address: CellAddress,
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
pub(crate) enum Edit {
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

/// The bit-field band palette, by name — a host maps each to a real,
/// theme-adapting color; the builder cycles it for successive fields.
const BINARY_PALETTE: [&str; 6] = ["blue", "green", "orange", "purple", "pink", "teal"];

pub struct Session {
    calculator: Rc<RefCell<Calculator>>,
    store: SheetStore,
    /// The log tape, shared so the engine's `History` reflection can read it
    /// live (via [`LogTape`]); a global running history, never cleared.
    entries: Rc<RefCell<Vec<LogEntry>>>,
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
    /// The preferred register width the bit editor opens a plain integer at
    /// (auto-bumped to fit a larger value or format). A fixed-width int ignores
    /// it — it edits at its own width.
    binary_width: u32,
    /// The active bit-format layout (a preset or custom), or `None` for a plain
    /// register. Paired with `binary_format_name` for the picker label.
    binary_layout: Option<Vec<BinaryFieldSpec>>,
    /// The active format's display name (the picker's current selection).
    binary_format_name: Option<String>,
    /// The visual format builder, present only while building/editing a custom
    /// bit-format (`Build new…` / `Edit current…`).
    format_builder: Option<FormatBuilder>,
    /// Bumped on every document mutation; the shell compares it against a saved
    /// baseline for the dirty indicator.
    revision: u64,
    /// Point mode's last-splice memory (re-click-replace / shift-extend), or
    /// `None` when no reference has been inserted into the current edit. The
    /// shell clears it as an edit begins or ends via [`clear_point_anchor`].
    ///
    /// [`clear_point_anchor`]: Session::clear_point_anchor
    point_anchor: Option<PointAnchor>,
    /// The working SQLite store backing this document's data sheets, opened
    /// lazily on the first import/open-with-data (`None` when the document has
    /// none). Each `DataSheet` shares it by `Rc`. The live editing target:
    /// edits write through to it, and save copies it into the `.soroban`
    /// package (open copies the package's `data.sqlite` back out).
    data_store: Option<Rc<DataStore>>,
    /// This session's working-database file (a per-session temp path).
    working_db: PathBuf,
    /// Whether the log tape and ↑/↓ input history persist to disk across
    /// launches. True for the app (`Session::new`), false for tests
    /// (`Session::ephemeral`) so they never touch the real data-dir files —
    /// the same discipline as Swift's `LogStore(persists:)`.
    persists: bool,
}

mod binary;
mod cells;
mod controls;
mod document;
mod formats;
mod inspector;
mod log;
mod persistence;
mod worksheets;

impl Session {
    /// The app session — its log tape and ↑/↓ input history persist to the
    /// user data dir and are reloaded here, so a relaunch restores them.
    pub fn new() -> Self {
        Self::build(true)
    }

    /// A disk-isolated session for tests: nothing is loaded or saved (mirrors
    /// Swift's `LogStore(persists: false)` and the working-DB temp path).
    pub fn ephemeral() -> Self {
        Self::build(false)
    }

    fn build(persists: bool) -> Self {
        let (calculator, store) = Self::fresh_engine();
        let mut session = Self {
            calculator,
            store,
            entries: Rc::new(RefCell::new(Vec::new())),
            input: String::new(),
            history: Vec::new(),
            history_cursor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            binary: None,
            binary_width: 32,
            binary_layout: None,
            binary_format_name: None,
            format_builder: None,
            revision: 0,
            point_anchor: None,
            data_store: None,
            working_db: Self::working_db_path(),
            persists,
        };
        session.install_log_source();
        if persists {
            session.load_persisted();
        }
        session
    }

    /// A per-session working-database path under the temp dir. Unique per
    /// process so concurrent windows/tests don't share one SQLite file.
    fn working_db_path() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("soroban-working-{}-{n}.sqlite", std::process::id()))
    }

    /// A fresh calculator and a sheet store wired to it. The log and the grid
    /// share one calculator: variables defined in the log are visible in cells,
    /// and cell references resolve from the log.
    pub(crate) fn fresh_engine() -> (Rc<RefCell<Calculator>>, SheetStore) {
        let calculator = Rc::new(RefCell::new(Calculator::new()));
        let store = SheetStore::new(Rc::clone(&calculator));
        (calculator, store)
    }

    /// Wire the live log tape into the current store so a log-line `History`
    /// expression reflects it. Re-called whenever the store is replaced
    /// (New/Open), since the tape (`entries`) outlives the engine.
    pub(crate) fn install_log_source(&self) {
        self.store
            .set_log_source(Rc::new(LogTape(Rc::clone(&self.entries))));
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
