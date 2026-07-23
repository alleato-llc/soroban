//! The application `Message` type — every event the shell reacts to. Kept in
//! its own module so the update/view logic (see `update`/`view`) reads without
//! scrolling past ~140 variants first.

use iced::{Color, Vector};
use soroban_engine::{CellAddress, FormatBuilderFieldKind, LanguageMode};

use crate::shot;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    InputChanged(String),
    Submit,
    /// An arrow key: (Δrow, Δcol, extend-selection). Routes by view — history
    /// recall in the log, cell-selection movement in the grid.
    ArrowKey(i32, i32, bool),
    ToggleView,
    /// Open / close the Settings window, or switch its section.
    OpenSettings,
    CloseSettings,
    SelectSettingsSection(usize),
    /// Pick a named theme (or `"Custom"`) from the Settings appearance section.
    SelectTheme(String),
    /// Set the base content font size (points).
    SetFontSize(f32),
    /// Nudge the font size by ±1 pt (⌘+ / ⌘-), clamped like the slider.
    ZoomFont(f32),
    /// Reset the font size to the default (⌘0).
    ResetFontSize,
    /// Pick the monospace font family (by display name).
    SelectFont(String),
    /// Pick the calculator language mode (Normal / Programmer / Scientific).
    SelectMode(LanguageMode),
    /// Cycle the language mode from the input-bar affordance.
    CycleMode,
    /// Edit one token of the custom palette: (token key, new color).
    SetCustomColor(String, Color),
    GridScrolled(Vector),
    GridSelected(usize, usize, bool),
    /// A column-header border was dragged: (column, new width in px).
    ColumnResized(usize, f32),
    EditChanged(String),
    EditSubmitted,
    EditCanceled,
    EditActivated(usize, usize),
    /// Enter pressed in the grid with no editor open — start editing the cell.
    GridEnter,
    /// A printable character typed in the grid with no editor open — start
    /// editing the cell seeded with that character (type-to-edit).
    GridType(String),
    Undo,
    Redo,
    SliderChanged(CellAddress, f32),
    StepperStepped(CellAddress, bool),
    CheckboxToggled(CellAddress),
    DropdownPicked(CellAddress, usize),
    SetNumberFormat(usize),
    SetAlignment(usize),
    SetTextColor(usize),
    SetFillColor(usize),
    NameChanged(String),
    NameCommitted,
    ToggleInspector,
    ToggleReference,
    ReferenceQueryChanged(String),
    ToggleBinary,
    BitToggled(usize),
    UseBinary,
    /// Change the bit-editor's register width (8…256).
    SetBinaryWidth(u32),
    /// Pick a bit-format by name, or `None` (the "None" entry) for a plain
    /// register.
    SetBinaryFormat(Option<String>),
    /// Typing in a numeric bit-field's input: (field name, draft text).
    BinaryFieldInput(String, String),
    /// Commit a bit-field to a value: (field name, text) — a numeric field's
    /// submitted draft, or an enum field's picked index.
    SetBinaryField(String, String),
    /// Open the format builder — `true` seeds it from the active format
    /// (Edit current…), `false` starts empty (Build new…).
    BeginBuildFormat(bool),
    CancelBuildFormat,
    /// Claim `bits` for the pending field in the builder.
    BuilderClaim(u32),
    BuilderDraftName(String),
    BuilderDraftKind(FormatBuilderFieldKind),
    BuilderDraftLabels(String),
    BuilderDraftBase(u32),
    BuilderAddField,
    /// Remove the committed builder field with this id.
    BuilderRemoveField(usize),
    /// Apply the builder's fields as the active format without saving.
    ApplyBuiltFormat,
    /// Typing in the builder's save-name box.
    BuilderSaveName(String),
    /// Save the builder's fields as a named custom format.
    SaveFormat,
    NewWorkbook,
    OpenWorkbook,
    SaveWorkbook,
    /// Open a CSV file as a new, EDITABLE data sheet (SQLite-backed) in the
    /// workbook. Edits are written to the working copy and saved into the
    /// `.soroban` file — the original `.csv` is never modified.
    OpenCsv,
    /// Append a new auto-named grid sheet and switch to it (the "+" tab button).
    AddSheet,
    /// Switch the active sheet to this tab index (a click on a tab).
    ActivateSheet(usize),
    /// Begin renaming the tab at this index (a double-click) — activates it and
    /// opens the inline rename bar seeded with its current name.
    BeginRenameSheet(usize),
    /// The inline sheet-rename field changed.
    SheetRenameChanged(String),
    /// Commit the inline sheet rename (Enter) — rewrites cross-sheet references.
    /// (Escape cancels it via `EditCanceled`, which closes the bar first.)
    SheetRenameCommitted,
    /// Delete the active sheet (refuses the last one).
    DeleteSheet,
    /// Open a top-level menu (`Some(i)`) or close any open one (`None`).
    ToggleMenu(Option<usize>),
    /// Copy / cut / paste the selection as TSV via the system clipboard.
    Copy,
    Cut,
    Paste,
    /// The clipboard contents arrived (from a Paste) — write them at the anchor.
    Pasted(Option<String>),
    /// Jump to a cell from an inspector provenance tag (select it, show the grid).
    JumpTo(CellAddress),
    /// Insert a sample expression from the empty-state into the log input.
    SampleClicked(String),
    /// An Examples-menu pick: show the log and fill the input bar with the
    /// expression (not evaluated — the user presses Enter). Mirrors the Swift
    /// app's `useExample`; the entries are `examples::CATEGORIES` statics.
    UseExample(&'static str),
    /// Fly out the Examples-menu category at this index (`Some(i)`, emitted on
    /// row hover) or retract any open one (`None`).
    HoverExampleCategory(Option<usize>),
    /// Accept the autocomplete row at this index (a click on a popup row).
    SuggestionPicked(usize),
    /// Accept the highlighted autocomplete row, or the first if none is
    /// highlighted (the Tab key); a no-op when the popup is closed.
    AcceptSuggestion,
    /// The cursor moved to this window-relative (x, y) — reveals/hides the auto-
    /// hiding menu bar as it nears / leaves the top edge, and records where a
    /// right-click would anchor the cell context menu.
    PointerMoved(f32, f32),
    /// A right-click on the grid — open the cell context menu at the cursor
    /// (formatting + clipboard verbs for the selected cell).
    OpenCellMenu,
    /// Dismiss the cell context menu (a backdrop click).
    CloseCellMenu,
    /// Drill the cell menu into a category (`Some(i)`) or back to the top
    /// level (`None`). The rime context menu is a flat panel — no flyouts — so
    /// categories navigate in place rather than flying out.
    ExpandCellSubmenu(Option<usize>),
    /// Clear the selected cells' contents (the context menu's Delete).
    DeleteSelection,
    /// Review-screenshot harness lifecycle (see [`shot`]); inert unless armed.
    Shot(shot::Event),
}
