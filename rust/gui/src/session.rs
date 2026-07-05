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

    // MARK: Log + input-history persistence (mirrors the Swift LogStore)

    /// The newest entries kept on disk — matches Swift's `LogStore.limit`.
    const LOG_LIMIT: usize = 500;

    /// The per-user data directory (`…/Application Support/Soroban` on macOS,
    /// `%APPDATA%\Soroban` on Windows, `~/.local/share/soroban` on Linux),
    /// created on demand. `None` if the platform has no data dir. The
    /// `SOROBAN_DATA_DIR` env var overrides it (an escape hatch, and the seam
    /// the persistence round-trip test points at a temp dir).
    fn data_dir() -> Option<PathBuf> {
        let dir = match std::env::var_os("SOROBAN_DATA_DIR") {
            Some(custom) => PathBuf::from(custom),
            None => dirs::data_dir()?.join("Soroban"),
        };
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn log_path() -> Option<PathBuf> {
        Some(Self::data_dir()?.join("log.json"))
    }

    fn input_history_path() -> Option<PathBuf> {
        Some(Self::data_dir()?.join("input_history.json"))
    }

    /// Reload the tape + ↑/↓ history from disk (best-effort — a missing or
    /// corrupt file just leaves the vec empty, like `LogStore::load`).
    fn load_persisted(&mut self) {
        if let Some(entries) = Self::log_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<LogEntry>>(&bytes).ok())
        {
            *self.entries.borrow_mut() = entries;
        }
        if let Some(history) = Self::input_history_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        {
            self.history = history;
        }
    }

    /// Snapshot the whole (small) tape + input history to disk, capped to the
    /// newest [`LOG_LIMIT`](Self::LOG_LIMIT). A no-op for an ephemeral session.
    fn save_persisted(&self) {
        if !self.persists {
            return;
        }
        let entries = self.entries.borrow();
        let tape = &entries[entries.len().saturating_sub(Self::LOG_LIMIT)..];
        if let (Some(path), Ok(bytes)) = (Self::log_path(), serde_json::to_vec(tape)) {
            let _ = std::fs::write(path, bytes);
        }
        let history = &self.history[self.history.len().saturating_sub(Self::LOG_LIMIT)..];
        if let (Some(path), Ok(bytes)) = (Self::input_history_path(), serde_json::to_vec(history)) {
            let _ = std::fs::write(path, bytes);
        }
    }

    // MARK: Data sheets

    /// True when the active sheet is a DataStore-backed table (not a grid).
    pub fn active_is_data(&self) -> bool {
        self.store.active_sheet().is_data()
    }

    /// Rows the grid should render for the active sheet — the whole grid for a
    /// calculation sheet, or the table's height (capped at 10,000) for a data
    /// sheet. Mirrors Swift's `visibleRowCount`.
    pub fn visible_row_count(&self) -> usize {
        let sheet = self.store.active_sheet();
        let count = match &*sheet.data.borrow() {
            Some(data) => data.row_count().clamp(1, 10_000),
            None => Spreadsheet::ROW_COUNT,
        };
        count
    }

    /// Columns to render for the active sheet (the table's width for a data
    /// sheet, else the grid's 26).
    pub fn visible_column_count(&self) -> usize {
        let sheet = self.store.active_sheet();
        let count = match &*sheet.data.borrow() {
            Some(data) => data.column_count().max(1),
            None => Spreadsheet::COLUMN_COUNT,
        };
        count
    }

    /// The working store, opened lazily at `working_db` on first need.
    fn ensure_data_store(&mut self) -> Result<Rc<DataStore>, String> {
        if let Some(store) = &self.data_store {
            return Ok(Rc::clone(store));
        }
        let store = Rc::new(DataStore::new(&self.working_db).map_err(|error| error.to_string())?);
        self.data_store = Some(Rc::clone(&store));
        Ok(store)
    }

    /// The working-database path to fold into a save — `Some` iff the document
    /// has any data sheet, so `data.sqlite` exists in a package iff it's needed.
    fn working_database_url(&self) -> Option<PathBuf> {
        self.store
            .sheets()
            .iter()
            .any(|sheet| sheet.is_data())
            .then(|| self.working_db.clone())
    }

    /// Reset the working database to `copy_from` (a package's `data.sqlite`) or
    /// to empty (`None`): drop the connection, clear the file + WAL/SHM, then
    /// copy the source in. Mirrors Swift's `prepareWorkingDatabase`.
    fn prepare_working_database(&mut self, copy_from: Option<&Path>) {
        self.data_store = None; // close the connection before touching the file
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.working_db.clone().into_os_string();
            path.push(suffix);
            let _ = std::fs::remove_file(path);
        }
        if let Some(source) = copy_from {
            let _ = std::fs::copy(source, &self.working_db);
        }
    }

    /// Import a CSV file as a new data sheet (a SQLite-backed table). Returns an
    /// optional note (e.g. that columns past the 26th were dropped). Mirrors
    /// Swift's `SheetModel.importCSV`.
    pub fn import_csv(&mut self, path: &Path) -> Result<Option<String>, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        // UTF-8, falling back to a byte-as-char (Latin-1) read like the Swift app.
        let text = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
        let mut rows = csv::parse(&text);
        if rows.iter().all(|row| row.is_empty()) {
            return Err("the CSV file is empty".into());
        }
        // Cap at the grid's column count (extra columns are dropped).
        let mut truncated = false;
        for row in &mut rows {
            if row.len() > Spreadsheet::COLUMN_COUNT {
                row.truncate(Spreadsheet::COLUMN_COUNT);
                truncated = true;
            }
        }
        let name = self.unique_sheet_name(Self::sanitized_name(path));
        let store = self.ensure_data_store()?;
        store
            .create_table(&name, &rows)
            .map_err(|error| error.to_string())?;
        let data = DataSheet::new(&name, Rc::clone(&store))
            .ok_or_else(|| "the imported table could not be opened".to_string())?;
        let sheet = self.store.make_data_sheet(&name, data);
        let mut sheets = self.store.sheets();
        sheets.push(Rc::clone(&sheet));
        self.store.replace_sheets(sheets, Some(&name));
        self.revision += 1;
        Ok(
            truncated
                .then(|| format!("Columns beyond {} were dropped.", Spreadsheet::COLUMN_COUNT)),
        )
    }

    /// A sanitized base table/sheet name from a file: the stem with `!`/`'`
    /// (reference-syntax breakers) blanked, trimmed, defaulting to "Data", and
    /// truncated to leave room for a de-dup " <n>" suffix.
    fn sanitized_name(path: &Path) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Data");
        let cleaned: String = stem
            .chars()
            .map(|c| if c == '!' || c == '\'' { ' ' } else { c })
            .collect();
        let trimmed = cleaned.trim();
        let base = if trimmed.is_empty() { "Data" } else { trimmed };
        base.chars()
            .take(SheetStore::MAX_NAME_LENGTH.saturating_sub(4))
            .collect()
    }

    /// `base`, or `base 2` / `base 3` / … until it's unused (case-insensitive).
    fn unique_sheet_name(&self, base: String) -> String {
        if self.store.sheet_named(&base).is_none() {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} {n}");
            if self.store.sheet_named(&candidate).is_none() {
                return candidate;
            }
            n += 1;
        }
    }

    /// A fresh calculator and a sheet store wired to it. The log and the grid
    /// share one calculator: variables defined in the log are visible in cells,
    /// and cell references resolve from the log.
    fn fresh_engine() -> (Rc<RefCell<Calculator>>, SheetStore) {
        let calculator = Rc::new(RefCell::new(Calculator::new()));
        let store = SheetStore::new(Rc::clone(&calculator));
        (calculator, store)
    }

    /// Wire the live log tape into the current store so a log-line `History`
    /// expression reflects it. Re-called whenever the store is replaced
    /// (New/Open), since the tape (`entries`) outlives the engine.
    fn install_log_source(&self) {
        self.store
            .set_log_source(Rc::new(LogTape(Rc::clone(&self.entries))));
    }

    // MARK: Log

    pub fn entries(&self) -> std::cell::Ref<'_, Vec<LogEntry>> {
        self.entries.borrow()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    /// Live typing — breaks any in-progress history recall.
    pub fn set_input(&mut self, text: String) {
        self.input = text;
        self.history_cursor = None;
    }

    /// Autocomplete candidates for the trailing identifier of `draft` — the
    /// engine's completion pass over the live environment (user variables and
    /// functions, constants, every built-in). Empty when the trailing word is
    /// blank or already a unique full match, so an empty result closes the
    /// popup for free.
    pub fn suggestions(&self, draft: &str) -> Vec<Completion> {
        let prefix = Calculator::trailing_identifier(draft);
        if prefix.is_empty() {
            return Vec::new();
        }
        self.calculator.borrow().completions(&prefix)
    }

    /// Splice `completion` over the trailing identifier of `draft`, returning
    /// the new text (cursor implicitly at the end). A function or type
    /// constructor also gets its opening `(` — you complete `fac` to `fact(`,
    /// ready for arguments, matching the CLI and the AppKit original.
    pub fn apply_completion(draft: &str, completion: &Completion) -> String {
        let trailing = Calculator::trailing_identifier(draft);
        // `trailing` is a suffix of `draft`, so the byte split is exact.
        let head = &draft[..draft.len() - trailing.len()];
        let mut out = String::from(head);
        out.push_str(&completion.name);
        if completion.kind == CompletionKind::Function {
            out.push('(');
        }
        out
    }

    /// Evaluate the current line, append it to the log, and record it in the
    /// history tape. A blank line is ignored.
    /// The active calculator dialect (drives how the LOG parses/renders; cells
    /// are always Normal). Programmer reads `^ & | << >> ~ %` as bitwise/modulo.
    pub fn language_mode(&self) -> LanguageMode {
        self.calculator.borrow().mode
    }

    /// Switch the log's dialect. Canonical storage is unchanged — only which
    /// glyphs you type and read differ.
    pub fn set_language_mode(&mut self, mode: LanguageMode) {
        self.calculator.borrow_mut().mode = mode;
        self.revision += 1;
    }

    /// Intercept the host-level `:mode [name]` command (like the CLI). Returns
    /// the log outcome to record, or `None` if the line isn't a mode command.
    fn mode_command(&mut self, line: &str) -> Option<Outcome> {
        let rest = line.strip_prefix(":mode")?;
        let arg = rest.trim();
        if arg.is_empty() {
            return Some(Outcome::Info(format!(
                "mode: {}",
                self.language_mode().name()
            )));
        }
        match LanguageMode::from_name(arg) {
            Some(mode) => {
                self.set_language_mode(mode);
                Some(Outcome::Info(format!("mode: {}", mode.name())))
            }
            None => Some(Outcome::Error {
                message: format!("unknown mode '{arg}' — normal, programmer, or finance"),
                position: None,
            }),
        }
    }

    pub fn submit(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
        let outcome = self
            .mode_command(&line)
            .unwrap_or_else(|| self.evaluate(&line));
        self.entries.borrow_mut().push(LogEntry {
            input: line.clone(),
            outcome,
        });
        // Don't stack consecutive duplicates in the recall tape.
        if self.history.last() != Some(&line) {
            self.history.push(line);
        }
        self.input.clear();
        self.history_cursor = None;
        // A log line may define a variable/function/type — mark the doc dirty.
        self.revision += 1;
        // The tape + recall history survive a relaunch (mirrors LogStore).
        self.save_persisted();
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

    /// The active sheet's control cells (slider / stepper / checkbox / dropdown),
    /// each with its live display — the gui hosts an interactive widget over each.
    /// Scans only the occupied cells (sparse), so it's cheap per frame.
    pub fn control_cells(&self) -> Vec<(CellAddress, CellDisplay)> {
        let mut controls: Vec<(CellAddress, CellDisplay)> = self
            .store
            .active_sheet()
            .grid
            .raws()
            .into_keys()
            .filter_map(|address| {
                let display = self.display_at(address);
                matches!(
                    display,
                    CellDisplay::Slider(_)
                        | CellDisplay::Stepper(_)
                        | CellDisplay::Checkbox(_)
                        | CellDisplay::Dropdown(_)
                )
                .then_some((address, display))
            })
            .collect();
        // Stable order (HashMap iteration isn't) so overlays don't reshuffle.
        controls.sort_by_key(|(address, _)| (address.column, address.row));
        controls
    }

    // MARK: Editing (slice ③)

    /// The raw (unevaluated) text stored in a cell — what the edit bar shows.
    /// A data sheet reads the stored table value; a grid sheet its cell raw.
    pub fn cell_raw(&self, address: CellAddress) -> String {
        let sheet = self.store.active_sheet();
        if let Some(data) = &*sheet.data.borrow() {
            return data.raw_value(address.row, address.column);
        }
        sheet.grid.raw(address)
    }

    /// Per-column widths for the active sheet, as a full `GRID_COLS`-length
    /// vector the grid indexes directly. Unset columns report `0.0`, which the
    /// grid reads as "use the default width".
    pub fn column_widths(&self) -> Vec<f32> {
        let sheet = self.store.active_sheet();
        let widths = sheet.column_widths.borrow();
        (0..GRID_COLS)
            .map(|col| widths.get(&col).copied().unwrap_or(0.0) as f32)
            .collect()
    }

    /// Set a column's width on the active sheet. Display-only (it never touches
    /// the dependency graph), but it dirties the document so the size is saved.
    pub fn set_column_width(&mut self, col: usize, width: f32) {
        self.store
            .active_sheet()
            .column_widths
            .borrow_mut()
            .insert(col, width as f64);
        self.revision += 1;
    }

    /// Would a leading operator complete this draft? True means the draft
    /// "expects an operand", so a cell click inserts a reference (point mode)
    /// rather than committing. Mirrors the Swift `Calculator.expectsOperand`.
    pub fn expects_operand(&self, draft: &str) -> bool {
        Calculator::expects_operand(draft)
    }

    /// Excel point mode: a click on `address` while editing `draft`, with
    /// `extend` set for a shift-click. When the draft ends expecting an operand
    /// (after `=`, an operator, `(`, `,`, `..`, …), the clicked cell's reference
    /// is spliced onto the draft and editing continues ([`PointClick::Inserted`]);
    /// otherwise the click means "I'm done here" and the caller commits
    /// ([`PointClick::Commit`]). The inserted reference is the cell's **name**
    /// when it has one (`'Rate'`), else its `A:1` address — names read more
    /// naturally, like Excel's defined names.
    ///
    /// Two continuations reuse the last splice (its memory lives in
    /// `point_anchor`, cleared by [`clear_point_anchor`] as an edit begins or
    /// ends): if the draft still equals what the last splice left, a plain
    /// **re-click replaces** that reference and a **shift-click extends** it into
    /// a `first..this` range (addresses, since ranges don't carry names). Once
    /// it's already a range, a further shift-click replaces it with the single
    /// clicked cell — matching the Swift `SheetModel`.
    ///
    /// [`clear_point_anchor`]: Session::clear_point_anchor
    pub fn point_click(&mut self, draft: &str, address: CellAddress, extend: bool) -> PointClick {
        if !self.wants_reference_insertion(draft) {
            self.point_anchor = None;
            return PointClick::Commit;
        }
        // Reuse the previous splice only when the draft is untouched since it.
        let anchor = self
            .point_anchor
            .as_ref()
            .filter(|a| a.draft == draft)
            .cloned();
        let (new_draft, reference) = match anchor {
            Some(anchor) if extend && !anchor.reference.contains("..") => {
                // Widen the just-inserted reference into a range: B:1 → B:1..B:4.
                let base = &draft[..draft.len() - anchor.reference.len()];
                let range = format!("{}..{}", anchor.address, address);
                (format!("{base}{range}"), range)
            }
            Some(anchor) => {
                // Re-click (or shift-click past a range) replaces the reference.
                let base = &draft[..draft.len() - anchor.reference.len()];
                let reference = self.reference_text(address);
                (format!("{base}{reference}"), reference)
            }
            None => {
                // Fresh insert: append onto the operand-expecting draft.
                let reference = self.reference_text(address);
                (format!("{draft}{reference}"), reference)
            }
        };
        self.point_anchor = Some(PointAnchor {
            draft: new_draft.clone(),
            reference,
            address,
        });
        PointClick::Inserted(new_draft)
    }

    /// Should a click insert a reference (vs. commit)? Yes when the draft still
    /// expects an operand, OR when it's exactly what our last splice left — that
    /// second case is how a re-click or shift-click keeps editing even though a
    /// complete `=B:1` no longer "expects an operand". Mirrors the Swift
    /// `wantsReferenceInsertion`.
    fn wants_reference_insertion(&self, draft: &str) -> bool {
        Calculator::expects_operand(draft)
            || self.point_anchor.as_ref().is_some_and(|a| a.draft == draft)
    }

    /// Forget the last point-mode splice — the shell calls this as an edit
    /// begins or ends so a stale anchor can't hijack a later click (the Swift
    /// `beginEditing`/`endEditing` reset).
    pub fn clear_point_anchor(&mut self) {
        self.point_anchor = None;
    }

    /// The text a point-mode click inserts for `address`: a quoted name if the
    /// cell is named on its sheet, else the bare `A:1` address.
    fn reference_text(&self, address: CellAddress) -> String {
        match self.cell_name(address) {
            Some(name) => format!("'{name}'"),
            None => address.to_string(),
        }
    }

    /// Commit one cell's raw content as an undoable edit, then recalculate.
    /// A no-op when the content is unchanged.
    pub fn set_cell_raw(&mut self, address: CellAddress, raw: &str) {
        let old = self.cell_raw(address);
        if old == raw {
            return;
        }
        // Data-sheet edits write through to SQLite (bounds-checked against the
        // table), not the dependency graph — no undo step, like the Swift app.
        let sheet = self.store.active_sheet();
        if let Some(data) = &*sheet.data.borrow() {
            if data.set_raw_value(raw, address.row, address.column).is_ok() {
                self.store.recalculate();
                self.revision += 1;
            }
            return;
        }
        self.apply_edit(vec![CellChange {
            address,
            old,
            new: raw.to_string(),
        }]);
    }

    /// TSV of the raw cell contents in the inclusive `(r0..=r1, c0..=c1)` rect —
    /// rows on `\n`, cells on `\t` (Excel/Numbers interchange). For copy/cut.
    pub fn selection_tsv(&self, r0: usize, r1: usize, c0: usize, c1: usize) -> String {
        (r0..=r1)
            .map(|row| {
                (c0..=c1)
                    .map(|col| self.cell_raw(CellAddress::new(col, row)))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear every cell in the inclusive rect as one undoable edit (cut).
    pub fn clear_range(&mut self, r0: usize, r1: usize, c0: usize, c1: usize) {
        let mut changes = Vec::new();
        for row in r0..=r1 {
            for col in c0..=c1 {
                let address = CellAddress::new(col, row);
                let old = self.cell_raw(address);
                if !old.is_empty() {
                    changes.push(CellChange {
                        address,
                        old,
                        new: String::new(),
                    });
                }
            }
        }
        if !changes.is_empty() {
            self.apply_edit(changes);
        }
    }

    /// Write a TSV block with its top-left at `anchor`, clipped to the grid, as
    /// one undoable edit. Rows split on `\n` (trailing `\r` tolerated), cells on
    /// `\t` — the inverse of [`Self::selection_tsv`], and Excel/Numbers-pasteable.
    pub fn paste_tsv(&mut self, anchor: CellAddress, tsv: &str) {
        let mut changes = Vec::new();
        for (drow, line) in tsv.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for (dcol, field) in line.split('\t').enumerate() {
                let row = anchor.row + drow;
                let col = anchor.column + dcol;
                if row >= GRID_ROWS || col >= GRID_COLS {
                    continue;
                }
                let address = CellAddress::new(col, row);
                let old = self.cell_raw(address);
                if old != field {
                    changes.push(CellChange {
                        address,
                        old,
                        new: field.to_string(),
                    });
                }
            }
        }
        if !changes.is_empty() {
            self.apply_edit(changes);
        }
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
        self.revision += 1;
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
            self.revision += 1;
        }
    }

    /// Redo the most recently undone step.
    pub fn redo(&mut self) {
        if let Some(edit) = self.redo_stack.pop() {
            self.apply_side(&edit, true);
            self.undo_stack.push(edit);
            self.revision += 1;
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
                    origin: Origin::Log,
                })
                .collect()
        };
        // Sheet-scoped 𝑖 definitions (name a value in a cell).
        for definition in self.active_definitions(SheetDefinitionKind::Variable) {
            let detail = match self.display_at(definition.address) {
                CellDisplay::Value(number) => number.to_string(),
                _ => String::new(),
            };
            rows.push(InspectorRow {
                label: definition.name,
                detail,
                origin: Origin::Cell(definition.address),
            });
        }
        // Named cell locations (name a place; value is the cell's).
        for (address, name) in self.store.active_sheet().grid.cell_names() {
            let detail = match self.display_at(address) {
                CellDisplay::Value(number) => number.to_string(),
                _ => String::new(),
            };
            rows.push(InspectorRow {
                label: name,
                detail,
                origin: Origin::Cell(address),
            });
        }
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
                    origin: Origin::Log,
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::Function) {
            rows.push(InspectorRow {
                label: definition.signature(),
                detail: String::new(),
                origin: Origin::Cell(definition.address),
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
                    origin: Origin::Log,
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::DataType) {
            rows.push(InspectorRow {
                label: definition.name,
                detail: String::new(),
                origin: Origin::Cell(definition.address),
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

    /// (Re)build the bit-editor draft from `ans` at the preferred width. Called
    /// when the editor opens and after each submit, so it tracks the latest
    /// result until you flip a bit (which stages a draft of its own). The active
    /// format layout carries over.
    pub fn refresh_binary(&mut self) {
        self.binary = BinaryView::make(&self.ans(), self.binary_width).ok();
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
    /// edited as bits. `bits` is **LSB-first** (`bits[0]` = bit 0), matching the
    /// widget and `flip_binary_bit` — `BinaryView::bits()` is MSB-first, so we
    /// reverse it here.
    pub fn binary_status(&self) -> BinaryStatus {
        if let Some(view) = &self.binary {
            let mut bits = view.bits(); // MSB-first from the engine…
            bits.reverse(); // …flipped to the widget's LSB-first contract.
            return BinaryStatus::Editable {
                bits,
                value: view.value().display_description(),
                hex: BigDecimal::new(view.pattern.clone(), 0)
                    .hex_text()
                    .unwrap_or_default(),
                width: view.width,
                signed: view.signed(),
                locked: view.width_locked(),
            };
        }
        let reason = match BinaryView::make(&self.ans(), self.binary_width) {
            Ok(_) => "Compute a value, then open the bit editor.".to_string(),
            Err(reason) => binary_reason(reason),
        };
        BinaryStatus::Unavailable(reason)
    }

    /// The register widths offered in the picker (empty when the editor is
    /// closed or the value is locked to a fixed width). A width too small to
    /// hold the current value or the active format is `enabled: false`.
    pub fn binary_widths(&self) -> Vec<BinaryWidth> {
        let Some(view) = &self.binary else {
            return Vec::new();
        };
        if view.width_locked() {
            return Vec::new();
        }
        let floor = view.minimum_width().max(self.layout_min_width());
        BINARY_EDITABLE_WIDTHS
            .into_iter()
            .map(|bits| BinaryWidth {
                bits,
                enabled: bits >= floor,
                active: bits == view.width,
            })
            .collect()
    }

    /// Re-open the draft at `width` (keeping the current value and format).
    /// Ignored when the value can't be represented, or is locked to its width.
    pub fn set_binary_width(&mut self, width: u32) {
        let Some(view) = &self.binary else { return };
        if view.width_locked() || width < view.minimum_width().max(self.layout_min_width()) {
            return;
        }
        if let Ok(rebuilt) = BinaryView::make(&view.value(), width) {
            self.binary_width = width;
            self.binary = Some(rebuilt);
        }
    }

    /// The names of the built-in format presets, in menu order (always
    /// available — the picker offers them whenever the editor is open).
    pub fn binary_preset_names(&self) -> Vec<String> {
        BinaryEditorPresets::standard()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// The active bit-format's name, or `None` for a plain register.
    pub fn binary_format_name(&self) -> Option<String> {
        self.binary_format_name.clone()
    }

    /// Apply a named format (or `None` to clear back to a plain register): a
    /// built-in preset, else a saved custom format (a user variable holding a
    /// layout-shaped map). Bumps the register width up if the layout needs more
    /// bits. An unknown name is a no-op.
    pub fn apply_binary_format(&mut self, name: Option<&str>) {
        let Some(name) = name else {
            self.binary_layout = None;
            self.binary_format_name = None;
            return;
        };
        let layout = BinaryEditorPresets::standard()
            .into_iter()
            .find(|(preset, _)| *preset == name)
            .and_then(|(_, value)| BinaryView::layout(&value))
            .or_else(|| {
                let calc = self.calculator.borrow();
                calc.environment()
                    .user_variables()
                    .get(name)
                    .and_then(BinaryView::layout)
            });
        if let Some(layout) = layout {
            self.install_layout(name, layout);
        }
    }

    /// Make `layout` the active format under `name`, widening the register to
    /// fit if it's currently too narrow.
    fn install_layout(&mut self, name: &str, layout: Vec<BinaryFieldSpec>) {
        let needed = BinaryView::layout_width(&layout);
        self.binary_format_name = Some(name.to_string());
        self.binary_layout = Some(layout);
        if let Some(view) = &self.binary {
            if view.width < needed && !view.width_locked() {
                if let Some(fit) = BINARY_EDITABLE_WIDTHS.into_iter().find(|&w| w >= needed) {
                    self.set_binary_width(fit);
                }
            }
        }
    }

    /// The names of saved custom formats — user variables whose value decodes
    /// as a bit-format layout (the same "any map/record `layout` accepts" rule
    /// as the AppKit app). Sorted; offered in the picker after the presets.
    pub fn saved_format_names(&self) -> Vec<String> {
        let calc = self.calculator.borrow();
        let mut names: Vec<String> = calc
            .environment()
            .user_variables()
            .iter()
            .filter(|(_, value)| BinaryView::layout(value).is_some())
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        names
    }

    // MARK: Format builder (Build new… / Edit current… / Save current…)

    /// Open the visual builder. With `seed_active`, it starts from the fields
    /// of the current format (Edit current…), else empty (Build new…).
    pub fn begin_format_build(&mut self, seed_active: bool) {
        let mut builder = FormatBuilder::new(&BINARY_PALETTE);
        if seed_active {
            if let Some(layout) = &self.binary_layout {
                builder.seed(layout);
            }
        }
        self.format_builder = Some(builder);
    }

    /// Close the builder without applying.
    pub fn cancel_format_build(&mut self) {
        self.format_builder = None;
    }

    /// The live builder, for the shell to render (fields, drafts, free bits).
    pub fn format_builder(&self) -> Option<&FormatBuilder> {
        self.format_builder.as_ref()
    }

    /// The live builder, for message handlers to drive (claim, add, remove,
    /// draft inputs).
    pub fn format_builder_mut(&mut self) -> Option<&mut FormatBuilder> {
        self.format_builder.as_mut()
    }

    /// Apply the builder's fields as the active format without saving —
    /// SpeedCrunch's transient "Apply" (the builder stays open).
    pub fn apply_built_format(&mut self) {
        let Some(builder) = &self.format_builder else {
            return;
        };
        if builder.is_empty() {
            return;
        }
        let layout = builder.layout();
        self.install_layout("Custom", layout);
    }

    /// Persist the builder's fields as a saved format named `name` (a user
    /// variable, so it rides the workbook), apply it, and close the builder.
    /// Returns false when the name is blank or no fields were built.
    pub fn save_format(&mut self, name: &str) -> bool {
        let name = name.trim().to_string();
        let Some(builder) = &self.format_builder else {
            return false;
        };
        if name.is_empty() || builder.is_empty() {
            return false;
        }
        let layout = builder.layout();
        // A loose-map value round-trips through `layout` and persists as a
        // workbook variable — set off-log so it never disturbs `ans`.
        let value = BinaryView::format_value(&layout);
        self.calculator.borrow_mut().set_user_variable(&name, value);
        self.format_builder = None;
        self.install_layout(&name, layout);
        self.revision += 1;
        true
    }

    /// Delete a saved format (removing its backing user variable). Clears the
    /// active format when it was the one deleted.
    pub fn delete_saved_format(&mut self, name: &str) {
        self.calculator.borrow_mut().remove_user_variable(name);
        if self.binary_format_name.as_deref() == Some(name) {
            self.binary_layout = None;
            self.binary_format_name = None;
        }
        self.revision += 1;
    }

    /// The active format's fields, decoded from the current value (empty for a
    /// plain register) — named ranges with their color, decoded readout, and
    /// everything the shell needs to render the right editor (a numeric input,
    /// an enum picker, or per-bit flag cells).
    pub fn binary_fields(&self) -> Vec<BinaryFieldView> {
        let (Some(view), Some(layout)) = (&self.binary, &self.binary_layout) else {
            return Vec::new();
        };
        let palette = BINARY_PALETTE;
        layout
            .iter()
            .zip(view.fields(layout))
            .enumerate()
            .map(|(index, (spec, field))| {
                let kind = if field.reserved {
                    BinaryFieldKind::Reserved
                } else if field.unused {
                    BinaryFieldKind::Unused
                } else if field.flags.is_some() {
                    BinaryFieldKind::Flags
                } else if field.values.is_some() {
                    BinaryFieldKind::Enum
                } else {
                    BinaryFieldKind::Numeric
                };
                // Enum selection: the value indexes the labels when in range.
                let (options, selected) = match &field.values {
                    Some(values) => {
                        let index = field.value.to_string().parse::<usize>().ok();
                        (values.clone(), index.filter(|&i| i < values.len()))
                    }
                    None => (Vec::new(), None),
                };
                // Flag bits, high→low, each with its absolute register bit.
                let flags = field
                    .flags
                    .as_ref()
                    .map(|names| {
                        names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| BinaryFlagBit {
                                name: name.clone(),
                                // flag i is the field's high bit minus i.
                                bit: field.low_bit + field.width - 1 - i as u32,
                                set: field.is_set_from_top(i),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                BinaryFieldView {
                    name: field.name.clone(),
                    low_bit: field.low_bit,
                    width: field.width,
                    color: spec
                        .color
                        .clone()
                        .or_else(|| Some(palette[index % palette.len()].to_string())),
                    label: field.label(),
                    kind,
                    value_text: field.value_text(),
                    options,
                    selected,
                    flags,
                    reserved: field.reserved,
                    unused: field.unused,
                }
            })
            .collect()
    }

    /// Set the field named `name` to `text`, parsed in the field's display base
    /// (a numeric field's `0x1b`/`755`, or an enum's selected index as a plain
    /// number). Clamped to the field's width by the engine. Returns false when
    /// there's no active format, no such field, or the text won't parse.
    pub fn set_binary_field(&mut self, name: &str, text: &str) -> bool {
        let (Some(view), Some(layout)) = (&self.binary, &self.binary_layout) else {
            return false;
        };
        let Some(spec) = layout.iter().find(|f| f.name == name) else {
            return false;
        };
        // Enum/flags carry no base; a numeric field reads in its own.
        let base = spec.base.unwrap_or(10);
        let Some(value) = BinaryView::parse(text, base) else {
            return false;
        };
        self.binary = Some(view.setting_field(name, &value, layout));
        true
    }

    /// The active format's total bit width (0 when none) — the register can't
    /// be narrower than this.
    fn layout_min_width(&self) -> u32 {
        self.binary_layout
            .as_ref()
            .map(|layout| BinaryView::layout_width(layout))
            .unwrap_or(0)
    }

    /// Drop the draft's value into the input line, ready to fold into an
    /// expression (the SpeedCrunch "Use" action).
    pub fn use_binary(&mut self) {
        if let Some(view) = &self.binary {
            self.input = view.value().display_description();
            self.history_cursor = None;
        }
    }

    // MARK: Workbook (slice ⑥)

    /// A monotonic mutation counter — the shell compares it to a saved baseline
    /// to show the dirty indicator.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Snapshot the document into a `Workbook`: every sheet's raw cells, named
    /// cells, and formats, plus the log's variables, functions, and data types.
    /// Data sheets carry only a `kind`/`table` marker — their rows live in the
    /// package's `data.sqlite`, folded in by `save_to`.
    fn build_workbook(&self) -> Workbook {
        let payloads: Vec<SheetPayload> = self
            .store
            .sheets()
            .iter()
            .map(|sheet| {
                let mut payload = SheetPayload::new(
                    sheet.name(),
                    sheet
                        .grid
                        .raws()
                        .into_iter()
                        .map(|(address, raw)| (address.to_string(), raw))
                        .collect::<HashMap<String, String>>(),
                );
                payload.names = sheet
                    .grid
                    .cell_names()
                    .into_iter()
                    .map(|(address, name)| (address.to_string(), name))
                    .collect();
                payload.column_widths = sheet
                    .column_widths
                    .borrow()
                    .iter()
                    .map(|(col, width)| (CellAddress::column_name_for(*col), *width))
                    .collect();
                payload.formats = sheet
                    .formats
                    .borrow()
                    .iter()
                    .map(|(address, format)| (address.to_string(), format.clone()))
                    .collect();
                if let Some(data) = &*sheet.data.borrow() {
                    payload.kind = Some("data".to_string());
                    payload.table = Some(data.table().to_string());
                }
                payload
            })
            .collect();

        let calculator = self.calculator.borrow();
        let environment = calculator.environment();
        let functions: Vec<UserFunction> = environment
            .all_user_functions()
            .into_iter()
            .cloned()
            .collect();
        Workbook::new(
            payloads,
            None,
            environment.user_variables(),
            &functions,
            environment.user_data_types(),
            environment.namespace_sources().to_vec(),
            environment.imported_namespaces().to_vec(),
        )
    }

    /// Write the document to a `.soroban` package, folding in the working
    /// `data.sqlite` when the document has any data sheets.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let workbook = self.build_workbook();
        let database = self.working_database_url();
        // Flush the WAL so the byte copy of the working DB captures every row.
        if database.is_some() {
            if let Some(store) = &self.data_store {
                store.checkpoint().map_err(|error| error.to_string())?;
            }
        }
        package::write(&workbook, path, database.as_deref()).map_err(|error| error.to_string())
    }

    /// Open a `.soroban` (package or legacy flat file), replacing the current
    /// document. The package's `data.sqlite` (if any) is copied into the working
    /// store first; restore order is types → functions → variables (via
    /// `restore_session`), then the sheets.
    pub fn open_from(&mut self, path: &Path) -> Result<(), String> {
        let workbook = package::read(path).map_err(|error| error.to_string())?;
        self.prepare_working_database(package::database_path(path).as_deref());
        self.load_workbook(workbook);
        Ok(())
    }

    /// Reset to an empty single-sheet document (New).
    pub fn new_workbook(&mut self) {
        self.prepare_working_database(None); // discard any data sheets' working db
        let (calculator, store) = Self::fresh_engine();
        self.calculator = calculator;
        self.store = store;
        self.install_log_source(); // the new store needs the tape rewired
        self.reset_document_state();
    }

    /// Rebuild the engine from a decoded workbook and swap it in.
    fn load_workbook(&mut self, workbook: Workbook) {
        let (calculator, store) = Self::fresh_engine();
        restore_session(&mut calculator.borrow_mut(), &workbook);
        let mut sheets = Vec::new();
        for payload in &workbook.sheets {
            // A data sheet reattaches to its table in the (already-copied)
            // working database; a corrupt/missing table degrades to an empty
            // grid sheet rather than failing the whole open.
            if payload.is_data() {
                let data = payload.table.as_deref().and_then(|table| {
                    self.ensure_data_store()
                        .ok()
                        .and_then(|store| DataSheet::new(table, store))
                });
                match data {
                    Some(data) => sheets.push(store.make_data_sheet(&payload.name, data)),
                    None => sheets.push(store.make_sheet(&payload.name)),
                }
                continue;
            }
            let sheet = store.make_sheet(&payload.name);
            let contents: HashMap<CellAddress, String> = payload
                .cells
                .iter()
                .filter_map(|(key, raw)| CellAddress::from_key(key).map(|a| (a, raw.clone())))
                .collect();
            let names: HashMap<CellAddress, String> = payload
                .names
                .iter()
                .filter_map(|(key, name)| CellAddress::from_key(key).map(|a| (a, name.clone())))
                .collect();
            sheet.grid.load(&contents);
            sheet.grid.load_cell_names(names);
            *sheet.column_widths.borrow_mut() = payload
                .column_widths
                .iter()
                .filter_map(|(name, width)| {
                    CellAddress::column_index(name).map(|col| (col, *width))
                })
                .collect();
            *sheet.formats.borrow_mut() = payload
                .formats
                .iter()
                .filter_map(|(key, format)| {
                    CellAddress::from_key(key).map(|address| (address, format.clone()))
                })
                .collect();
            sheets.push(sheet);
        }
        let first = workbook.sheets.first().map(|payload| payload.name.clone());
        store.replace_sheets(sheets, first.as_deref());
        self.calculator = calculator;
        self.store = store;
        self.install_log_source(); // the new store needs the tape rewired
        self.reset_document_state();
    }

    /// Clear the per-document transient state after New/Open (the log tape is a
    /// global running history, so it's kept).
    fn reset_document_state(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.binary = None;
        self.input.clear();
        self.history_cursor = None;
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
    rows.sort_by_key(|a| a.label.to_lowercase());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A persisting session reloads its log tape and ↑/↓ input history from
    /// disk on the next launch — the parity fix for the Swift `LogStore`.
    #[test]
    fn log_tape_and_input_history_survive_a_relaunch() {
        // Point persistence at a unique temp dir, never the real data dir.
        let dir = std::env::temp_dir().join(format!(
            "soroban-persist-test-{}-{:p}",
            std::process::id(),
            &() as *const ()
        ));
        std::env::set_var("SOROBAN_DATA_DIR", &dir);

        // First launch: type two lines, which persist on each submit.
        {
            let mut session = Session::new();
            session.set_input("1 + 1".to_string());
            session.submit();
            session.set_input("2 + 3".to_string());
            session.submit();
        }

        // Second launch: the tape and recall history come back.
        {
            let mut session = Session::new();
            let entries = session.entries();
            assert_eq!(entries.len(), 2, "the log tape reloaded");
            assert_eq!(entries[0].input, "1 + 1");
            assert!(matches!(&entries[1].outcome, Outcome::Value(v) if v == "5"));
            drop(entries);
            // ↑ recalls the newest submitted line.
            session.recall_previous();
            assert_eq!(session.input(), "2 + 3", "the ↑/↓ history reloaded");
        }

        // An ephemeral session ignores the same dir entirely.
        {
            let session = Session::ephemeral();
            assert!(session.entries().is_empty(), "ephemeral loads nothing");
        }

        std::env::remove_var("SOROBAN_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
