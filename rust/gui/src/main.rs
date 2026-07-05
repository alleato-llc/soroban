//! Soroban — the Rust/iced desktop app (docs/MIGRATION.md Phase 3b).
//!
//! Slice ①–④: a log-view calculator plus an editable spreadsheet grid, with
//! ⌘\ toggling between them. The log and grid share one engine session
//! (`soroban_gui::session::Session`) — variables defined in the log are visible in cells,
//! and `updateCell(…)` from the log populates the grid. A formula/edit bar
//! commits cell edits (undoable, ⌘Z / ⇧⌘Z), point mode inserts a cell's
//! reference when you click it mid-formula, a control strip drives the
//! selected cell's slider / stepper / checkbox / dropdown, a format bar sets
//! its number format, alignment, and colors, a name box names its location
//! (`'Rate'`), a Names inspector sidebar lists every live name, a searchable
//! Reference sidebar documents every function, and a Binary bit-editor strip
//! edits the last result's bits. This file is the iced shell (state → message
//! → update → view) and the rime-styled rendering; the last slice adds workbook
//! save/open.

mod shot;
mod themes;

use iced::widget::{column, container, mouse_area, operation, row, scrollable, text, Id};
use iced::{
    event, keyboard, Color, Element, Event, Font, Length, Subscription, Task, Theme, Vector,
};
use rime::icons::{self, glyph};
use rime::theme;
use rime::widgets::menu;
use rime::widgets::{
    bit_grid, button, caption, card, color_field, grid, menu_bar_with_trailing, section, select,
    settings, slider, stepper, suggestion_list, text_field, toggle, BitBand, CellAlign, GridCell,
    GridSelection, Menu, MenuItem, Suggestion,
};
use soroban_engine::{
    CellAddress, CellAlignment, CellDisplay, CellFormat, Completion, FormatBuilderFieldKind,
    LanguageMode, NumberFormat, PaletteColor,
};
use soroban_gui::session::{
    BinaryFieldKind, BinaryStatus, Origin, Outcome, PointClick, Session, GRID_COLS, GRID_ROWS,
};
use std::collections::HashMap;
use std::path::PathBuf;

const MONO: Font = Font::MONOSPACE;

/// The edit bar's widget id, used to refocus it after a point-mode reference
/// insertion (a grid click steals focus, so we grab it back).
fn edit_bar_id() -> Id {
    Id::new("soroban-edit-bar")
}

/// The log input's widget id, so clicking an empty-state sample can focus it.
fn log_input_id() -> Id {
    Id::new("soroban-log-input")
}

/// The inline cell editor's widget id (hosted inside the grid on the active
/// cell), so double-click / point-mode can focus it.
fn grid_editor_id() -> Id {
    Id::new("soroban-grid-editor")
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum ViewMode {
    #[default]
    Log,
    Grid,
}

/// Capitalize the first letter (`"programmer"` → `"Programmer"`) — for the
/// mode picker's display labels over the engine's lowercase mode names.
fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[derive(Default)]
struct App {
    session: Session,
    /// The selected theme's catalog name, or `"Custom"` when the palette is
    /// hand-edited (then `custom_palette` holds it). See [`themes`].
    theme_name: String,
    /// The hand-edited palette, live only while `theme_name == "Custom"`.
    custom_palette: Option<theme::Palette>,
    /// The base font size for the log/grid content (points); the Settings
    /// window's slider drives it.
    font_size: f32,
    /// Whether the Settings window is open, and which section (0 Appearance,
    /// 1 Calculator).
    settings_open: bool,
    settings_section: usize,
    mode: ViewMode,
    grid_offset: Vector,
    grid_selection: Option<GridSelection>,
    /// The edit bar's contents for the selected cell.
    edit_draft: String,
    /// The name box's contents — the selected cell's name, if any.
    name_draft: String,
    /// True while the edit bar holds uncommitted typing — the point-mode gate:
    /// a grid click on an operand-expecting draft inserts a reference instead
    /// of moving the selection.
    editing: bool,
    /// Whether the names inspector sidebar is showing.
    inspector_visible: bool,
    /// Whether the reference (docs) sidebar is showing, and its search query.
    reference_visible: bool,
    reference_query: String,
    /// Whether the binary bit-editor strip is showing.
    binary_visible: bool,
    /// Uncommitted text for a numeric bit-field editor, keyed by field name;
    /// cleared whenever the register changes so it re-syncs to the live value.
    binary_field_drafts: HashMap<String, String>,
    /// The name box for saving a custom format (the builder's Save field).
    builder_save_name: String,
    /// The saved file, if any, and the revision at which it was last saved
    /// (compared against the session's live revision for the dirty indicator).
    file_path: Option<PathBuf>,
    saved_revision: u64,
    /// Which top menu (File / Edit / View) is open, if any — the menu bar is
    /// stateless, so the host owns this. See [`Self::menus`].
    menu_open: Option<usize>,
    /// The menu bar auto-hides (iced has no system menu bar, so the in-window
    /// one is chrome): shown only while the pointer is near the top edge or a
    /// menu is open. Tracked from cursor movement.
    menu_revealed: bool,
    /// Live autocomplete candidates for the focused input (the log bar or the
    /// grid formula bar), recomputed on every keystroke; empty hides the popup.
    suggestions: Vec<Completion>,
    /// Which suggestion row is highlighted (↑/↓ move it, Enter accepts it);
    /// `None` means no row is selected, so Enter submits rather than completes.
    suggest_highlight: Option<usize>,
    /// The review-screenshot harness, present only when `SOROBAN_SHOT` is set —
    /// otherwise `None` and the whole thing is inert. See [`shot`].
    shot: Option<shot::Shot>,
}

#[derive(Debug, Clone)]
enum Message {
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
    /// Pick the calculator language mode (Normal / Programmer / Finance).
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
    /// Accept the autocomplete row at this index (a click on a popup row).
    SuggestionPicked(usize),
    /// Accept the highlighted autocomplete row, or the first if none is
    /// highlighted (the Tab key); a no-op when the popup is closed.
    AcceptSuggestion,
    /// The cursor moved to this window-relative Y — reveals/hides the auto-
    /// hiding menu bar as it nears / leaves the top edge.
    PointerMoved(f32),
    /// Review-screenshot harness lifecycle (see [`shot`]); inert unless armed.
    Shot(shot::Event),
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        // Any real action closes an open menu (the backdrop only closes on an
        // outside click); the toggle itself opens/switches menus, the screenshot
        // harness's background frames must leave it be, and — crucially now that
        // the menu bar auto-hides — plain cursor movement must NOT close it, or
        // reaching for a submenu item would dismiss the menu before you got there.
        if self.menu_open.is_some()
            && !matches!(
                message,
                Message::ToggleMenu(_) | Message::Shot(_) | Message::PointerMoved(_)
            )
        {
            self.menu_open = None;
        }
        match message {
            Message::InputChanged(text) => {
                self.session.set_input(text);
                self.refresh_suggestions();
            }
            Message::Submit => {
                // Enter accepts a highlighted suggestion instead of submitting;
                // with none highlighted it submits the line (the popup, if any,
                // clears on the fresh empty input).
                if let Some(index) = self.suggest_highlight {
                    return self.accept_suggestion(index);
                }
                self.session.submit();
                self.clear_suggestions();
                // The bit editor tracks the newest result until you flip a bit.
                if self.binary_visible {
                    self.session.refresh_binary();
                    self.sync_binary_field_drafts();
                }
            }
            // Arrows: an open autocomplete popup claims ↑/↓ first (move the
            // highlight — the dual-role the rime widget documents); otherwise
            // history recall in the log, cell navigation in the grid (when not
            // editing — an open editor owns its own arrows).
            Message::ArrowKey(drow, _, _) if drow != 0 && !self.suggestions.is_empty() => {
                self.move_highlight(drow);
            }
            Message::ArrowKey(drow, dcol, extend) => match self.mode {
                ViewMode::Log => {
                    if drow < 0 {
                        self.session.recall_previous();
                    } else if drow > 0 {
                        self.session.recall_next();
                    }
                }
                ViewMode::Grid => {
                    if !self.editing {
                        self.move_selection(drow, dcol, extend);
                    }
                }
            },
            Message::OpenSettings => {
                self.settings_open = true;
                self.menu_open = None;
            }
            Message::CloseSettings => self.settings_open = false,
            Message::SelectSettingsSection(index) => self.settings_section = index,
            Message::SelectTheme(name) => {
                // Entering "Custom" seeds the editable palette from the current
                // theme, so the color rows start where the eye already is.
                if name == "Custom" && self.custom_palette.is_none() {
                    self.custom_palette = Some(self.active_palette());
                }
                self.theme_name = name;
            }
            Message::SetFontSize(size) => self.font_size = size.clamp(9.0, 28.0),
            Message::SelectMode(mode) => self.session.set_language_mode(mode),
            Message::CycleMode => {
                let next = match self.session.language_mode() {
                    LanguageMode::Normal => LanguageMode::Programmer,
                    LanguageMode::Programmer => LanguageMode::Finance,
                    LanguageMode::Finance => LanguageMode::Normal,
                };
                self.session.set_language_mode(next);
                // Keep typing where it was — a toolbar click would otherwise
                // steal focus from the input.
                return operation::focus(log_input_id());
            }
            Message::SetCustomColor(key, color) => {
                let mut palette = self.custom_palette.unwrap_or_else(|| self.active_palette());
                palette.set(&key, color);
                self.custom_palette = Some(palette);
                self.theme_name = "Custom".to_string();
            }
            Message::ToggleView => {
                self.mode = match self.mode {
                    ViewMode::Log => ViewMode::Grid,
                    ViewMode::Grid => ViewMode::Log,
                }
            }
            Message::GridScrolled(offset) => self.grid_offset = offset,
            Message::GridSelected(row, col, extend) => {
                return self.select_or_point(row, col, extend)
            }
            Message::ColumnResized(col, width) => self.session.set_column_width(col, width),
            Message::EditChanged(text) => {
                self.edit_draft = text;
                self.editing = true;
                self.refresh_suggestions();
            }
            // Enter in the editor: accept a highlighted suggestion, else commit
            // and advance the selection down (Excel).
            Message::EditSubmitted => {
                if let Some(index) = self.suggest_highlight {
                    return self.accept_suggestion(index);
                }
                self.commit_edit();
                self.move_selection(1, 0, false);
            }
            Message::EditCanceled => {
                // Escape dismisses the Settings window first, if it's open.
                if self.settings_open {
                    self.settings_open = false;
                    return Task::none();
                }
                self.editing = false;
                self.clear_suggestions();
                self.load_draft();
            }
            // Double-click a cell → edit it in place: select it, load its raw,
            // open the inline editor and focus it.
            Message::EditActivated(row, col) => {
                self.grid_selection = Some(GridSelection::cell(row, col));
                self.load_draft();
                self.editing = true;
                return operation::focus(grid_editor_id());
            }
            // Enter with no editor open → start editing the selected cell.
            Message::GridEnter => {
                if self.mode == ViewMode::Grid && !self.editing && self.active_cell().is_some() {
                    self.editing = true;
                    return operation::focus(grid_editor_id());
                }
            }
            // Type-to-edit: a character with no editor open seeds a fresh edit.
            Message::GridType(text) => {
                if self.mode == ViewMode::Grid && !self.editing && self.active_cell().is_some() {
                    self.edit_draft = text;
                    self.editing = true;
                    return operation::focus(grid_editor_id());
                }
            }
            // Undo/redo are GRID-document operations (the log is history, not
            // document state), so they're inert in the log — matching copy/cut/
            // paste, which already gate on the grid view.
            Message::Undo => {
                if self.mode == ViewMode::Grid {
                    self.session.undo();
                    self.editing = false;
                    self.load_draft();
                }
            }
            Message::Redo => {
                if self.mode == ViewMode::Grid {
                    self.session.redo();
                    self.editing = false;
                    self.load_draft();
                }
            }
            // Inline control interactions rewrite the cell's storage literal (the
            // address rides the message, since many controls are live at once);
            // reload the draft so an open editor / the formula bar stays in sync.
            Message::SliderChanged(address, value) => {
                self.session.set_slider(address, value as f64);
                self.load_draft();
            }
            Message::StepperStepped(address, up) => {
                self.session.step_control(address, up);
                self.load_draft();
            }
            Message::CheckboxToggled(address) => {
                self.session.toggle_checkbox(address);
                self.load_draft();
            }
            Message::DropdownPicked(address, index) => {
                self.session.set_dropdown_index(address, index);
                self.load_draft();
            }
            // Format edits mutate one field of the active cell's format and
            // commit it (display-only, undoable).
            Message::SetNumberFormat(index) => {
                self.apply_format(|format| format.number_format = number_format_at(index));
            }
            Message::SetAlignment(index) => {
                self.apply_format(|format| format.alignment = CellAlignment::ALL[index]);
            }
            Message::SetTextColor(index) => {
                self.apply_format(|format| format.text_color = color_choice(index));
            }
            Message::SetFillColor(index) => {
                self.apply_format(|format| format.fill_color = color_choice(index));
            }
            Message::NameChanged(text) => self.name_draft = text,
            Message::NameCommitted => {
                if let Some(address) = self.active_cell() {
                    // On a validation error (duplicate/illegal), reload reverts
                    // the box to the stored name.
                    let _ = self.session.set_cell_name(address, &self.name_draft);
                    self.load_draft();
                }
            }
            Message::ToggleInspector => self.inspector_visible = !self.inspector_visible,
            Message::ToggleReference => self.reference_visible = !self.reference_visible,
            Message::ReferenceQueryChanged(text) => self.reference_query = text,
            Message::ToggleBinary => {
                self.binary_visible = !self.binary_visible;
                if self.binary_visible {
                    self.session.refresh_binary();
                    self.sync_binary_field_drafts();
                }
            }
            Message::BitToggled(index) => {
                self.session.flip_binary_bit(index);
                self.sync_binary_field_drafts();
            }
            Message::SetBinaryWidth(width) => {
                self.session.set_binary_width(width);
                self.sync_binary_field_drafts();
            }
            Message::SetBinaryFormat(name) => {
                self.session.apply_binary_format(name.as_deref());
                self.sync_binary_field_drafts();
            }
            Message::BinaryFieldInput(name, text) => {
                self.binary_field_drafts.insert(name, text);
            }
            Message::SetBinaryField(name, text) => {
                self.session.set_binary_field(&name, &text);
                self.sync_binary_field_drafts();
            }
            Message::BeginBuildFormat(seed) => {
                self.session.begin_format_build(seed);
                self.builder_save_name.clear();
            }
            Message::CancelBuildFormat => {
                self.session.cancel_format_build();
                self.builder_save_name.clear();
            }
            Message::BuilderClaim(bits) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.claim(bits);
                }
            }
            Message::BuilderDraftName(text) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.draft_name = text;
                }
            }
            Message::BuilderDraftKind(kind) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.draft_kind = kind;
                }
            }
            Message::BuilderDraftLabels(text) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.draft_labels = text;
                }
            }
            Message::BuilderDraftBase(base) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.draft_base = base;
                }
            }
            Message::BuilderAddField => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.add_field();
                }
            }
            Message::BuilderRemoveField(id) => {
                if let Some(b) = self.session.format_builder_mut() {
                    b.remove(id);
                }
            }
            Message::ApplyBuiltFormat => {
                self.session.apply_built_format();
                self.sync_binary_field_drafts();
            }
            Message::BuilderSaveName(text) => self.builder_save_name = text,
            Message::SaveFormat => {
                if self.session.save_format(&self.builder_save_name) {
                    self.builder_save_name.clear();
                    self.sync_binary_field_drafts();
                }
            }
            Message::UseBinary => {
                self.session.use_binary();
                // The value lands in the log input; show it.
                self.mode = ViewMode::Log;
            }
            Message::NewWorkbook => {
                self.session.new_workbook();
                self.file_path = None;
                self.after_document_change();
            }
            Message::OpenWorkbook => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Soroban workbook", &["soroban"])
                    .pick_file()
                {
                    if self.session.open_from(&path).is_ok() {
                        self.file_path = Some(path);
                        self.after_document_change();
                    }
                }
            }
            Message::SaveWorkbook => {
                // Save to the current file, or prompt for one the first time.
                let target = self.file_path.clone().or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Soroban workbook", &["soroban"])
                        .set_file_name("Untitled.soroban")
                        .save_file()
                });
                if let Some(path) = target {
                    if self.session.save_to(&path).is_ok() {
                        self.file_path = Some(path);
                        self.saved_revision = self.session.revision();
                    }
                }
            }
            // Copy / cut / paste the grid selection as TSV (Excel-interop). Only
            // in the grid — in the log, ⌘C/⌘V fall through to normal text copy.
            Message::Copy => {
                if self.mode == ViewMode::Grid {
                    if let Some((r0, r1, c0, c1)) = self.selection_bounds() {
                        return iced::clipboard::write(self.session.selection_tsv(r0, r1, c0, c1));
                    }
                }
            }
            Message::Cut => {
                if self.mode == ViewMode::Grid {
                    if let Some((r0, r1, c0, c1)) = self.selection_bounds() {
                        let tsv = self.session.selection_tsv(r0, r1, c0, c1);
                        self.session.clear_range(r0, r1, c0, c1);
                        self.load_draft();
                        return iced::clipboard::write(tsv);
                    }
                }
            }
            Message::Paste => {
                if self.mode == ViewMode::Grid && !self.editing && self.active_cell().is_some() {
                    return iced::clipboard::read().map(Message::Pasted);
                }
            }
            Message::Pasted(contents) => {
                if let (Some(text), Some(anchor)) = (contents, self.active_cell()) {
                    self.session.paste_tsv(anchor, &text);
                    self.load_draft();
                }
            }
            Message::ToggleMenu(next) => self.menu_open = next,
            // Inspector row → jump: select the cell and show the grid.
            Message::JumpTo(address) => {
                self.grid_selection = Some(GridSelection::cell(address.row, address.column));
                self.mode = ViewMode::Grid;
                self.editing = false;
                self.load_draft();
            }
            Message::SampleClicked(sample) => {
                self.session.set_input(sample);
                return operation::focus(log_input_id());
            }
            Message::SuggestionPicked(index) => return self.accept_suggestion(index),
            Message::AcceptSuggestion => {
                // Tab accepts the highlighted row, or the top one; inert when the
                // popup is closed so Tab still behaves normally elsewhere.
                if !self.suggestions.is_empty() {
                    let index = self.suggest_highlight.unwrap_or(0);
                    return self.accept_suggestion(index);
                }
            }
            // Reveal the menu bar while the pointer hugs the top edge; a small
            // margin past the bar keeps it steady once revealed (no flicker as
            // the cursor drifts onto a menu item).
            Message::PointerMoved(y) => self.menu_revealed = y <= menu::BAR_HEIGHT + 8.0,
            Message::Shot(event) => return shot::handle(self, event),
        }
        Task::none()
    }

    /// After a New/Open replaces the document, drop the transient view state and
    /// mark the session clean at its current revision.
    fn after_document_change(&mut self) {
        self.grid_selection = None;
        self.editing = false;
        self.saved_revision = self.session.revision();
        self.load_draft();
    }

    /// True when the document has unsaved changes (the live revision has moved
    /// past the one last written).
    fn is_dirty(&self) -> bool {
        self.session.revision() != self.saved_revision
    }

    /// Mutate the active cell's format via `edit` and commit it undoably.
    fn apply_format(&mut self, edit: impl FnOnce(&mut CellFormat)) {
        if let Some(address) = self.active_cell() {
            let mut format = self.session.cell_format(address);
            edit(&mut format);
            self.session.apply_format(address, format);
        }
    }

    /// The selection's inclusive `(r0, r1, c0, c1)` rect, corners normalized.
    fn selection_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        self.grid_selection.map(|selection| selection.bounds())
    }

    /// The active (anchor) cell — where the edit bar reads and writes.
    fn active_cell(&self) -> Option<CellAddress> {
        self.grid_selection.map(|selection| {
            let (row, col) = selection.anchor;
            CellAddress::new(col, row)
        })
    }

    /// Reload the edit bar and name box from the active cell. Also forgets any
    /// point-mode anchor — every fresh edit or navigation flows through here, so
    /// a stale reference-splice can't hijack the next click.
    fn load_draft(&mut self) {
        self.session.clear_point_anchor();
        self.clear_suggestions();
        match self.active_cell() {
            Some(address) => {
                self.edit_draft = self.session.cell_raw(address);
                self.name_draft = self.session.cell_name(address).unwrap_or_default();
            }
            None => {
                self.edit_draft.clear();
                self.name_draft.clear();
            }
        }
    }

    /// Recompute autocomplete candidates for whichever input is focused (the
    /// log bar in Log view, the formula bar in Grid view) and reset the
    /// highlight. Called on every keystroke; an empty result hides the popup.
    fn refresh_suggestions(&mut self) {
        let draft = match self.mode {
            ViewMode::Log => self.session.input().to_string(),
            ViewMode::Grid => self.edit_draft.clone(),
        };
        self.suggestions = self.session.suggestions(&draft);
        self.suggest_highlight = None;
    }

    /// The autocomplete popup for the focused input, or `None` when there's
    /// nothing to complete. Each row shows the candidate plus a dim kind badge
    /// (`ƒ` / `var` / `const`); a click accepts it.
    fn suggestion_popup(&self) -> Option<Element<'_, Message>> {
        let rows: Vec<Suggestion> = self
            .suggestions
            .iter()
            .map(|completion| {
                Suggestion::with_hint(completion.name.clone(), completion.kind.badge())
            })
            .collect();
        suggestion_list(rows, self.suggest_highlight, Message::SuggestionPicked)
    }

    /// Drop any open autocomplete popup (on submit, cancel, or navigation).
    fn clear_suggestions(&mut self) {
        self.suggestions.clear();
        self.suggest_highlight = None;
    }

    /// Move the highlighted suggestion row by `delta` (±1). Down from "none"
    /// lands on the first row; up from the first row returns to "none" (so the
    /// next Enter submits again); it clamps at the last row.
    fn move_highlight(&mut self, delta: i32) {
        let count = self.suggestions.len();
        if count == 0 {
            return;
        }
        self.suggest_highlight = match self.suggest_highlight {
            None if delta > 0 => Some(0),
            None => Some(count - 1),
            Some(0) if delta < 0 => None,
            Some(index) => {
                let next = (index as i32 + delta).clamp(0, count as i32 - 1);
                Some(next as usize)
            }
        };
    }

    /// Splice the chosen completion into the focused input, then recompute
    /// (usually emptying the popup) and keep focus on that input.
    fn accept_suggestion(&mut self, index: usize) -> Task<Message> {
        let Some(completion) = self.suggestions.get(index).cloned() else {
            return Task::none();
        };
        let focus = match self.mode {
            ViewMode::Log => {
                let next = Session::apply_completion(self.session.input(), &completion);
                self.session.set_input(next);
                log_input_id()
            }
            ViewMode::Grid => {
                self.edit_draft = Session::apply_completion(&self.edit_draft, &completion);
                self.editing = true;
                edit_bar_id()
            }
        };
        self.refresh_suggestions();
        operation::focus(focus)
    }

    /// Write the edit bar back to the active cell as an undoable edit.
    fn commit_edit(&mut self) {
        if let Some(address) = self.active_cell() {
            self.session.set_cell_raw(address, &self.edit_draft);
        }
        self.editing = false;
        self.load_draft();
    }

    /// A grid click: in point mode (editing an operand-expecting draft) insert
    /// the clicked cell's reference and refocus the bar; otherwise commit any
    /// pending edit, then move the selection and load its content.
    fn select_or_point(&mut self, row: usize, col: usize, extend: bool) -> Task<Message> {
        if self.mode == ViewMode::Grid && self.editing {
            match self
                .session
                .point_click(&self.edit_draft, CellAddress::new(col, row), extend)
            {
                // Point mode: the clicked cell's reference joins the draft and
                // the inline editor keeps focus (the in-grid editor is active).
                PointClick::Inserted(draft) => {
                    self.edit_draft = draft;
                    return operation::focus(grid_editor_id());
                }
                // Navigating away commits the in-progress edit (Excel behavior).
                PointClick::Commit => self.commit_edit(),
            }
        }
        self.grid_selection = Some(match (extend, self.grid_selection) {
            (true, Some(current)) => GridSelection {
                anchor: current.anchor,
                extent: (row, col),
            },
            _ => GridSelection::cell(row, col),
        });
        self.editing = false;
        self.load_draft();
        Task::none()
    }

    /// Move the grid selection by `(drow, dcol)`, clamped to the grid. With
    /// `extend`, the anchor holds and the opposite corner moves (shift-arrow);
    /// otherwise it's a plain single-cell move that reloads the edit draft.
    fn move_selection(&mut self, drow: i32, dcol: i32, extend: bool) {
        let current = self
            .grid_selection
            .unwrap_or_else(|| GridSelection::cell(0, 0));
        self.grid_selection = Some(next_selection(current, drow, dcol, extend));
        if !extend {
            self.load_draft();
        }
    }

    fn theme(&self) -> Theme {
        let name = self.theme_display_name();
        self.active_palette().iced_theme(name)
    }

    /// The active palette: the hand-edited one under `"Custom"`, else the named
    /// catalog palette (falling back to the default for an unknown name).
    fn active_palette(&self) -> theme::Palette {
        if self.theme_name == "Custom" {
            return self
                .custom_palette
                .unwrap_or_else(|| themes::palette(themes::default_name()));
        }
        themes::palette(&self.theme_name)
    }

    /// The theme's display name — the catalog name, or `"Custom"`, defaulting to
    /// the first catalog entry when unset (a fresh `App::default`).
    fn theme_display_name(&self) -> &str {
        if self.theme_name.is_empty() {
            themes::default_name()
        } else {
            &self.theme_name
        }
    }

    /// The effective base font size in points (a fresh `App::default` reads 0).
    fn base_font_size(&self) -> f32 {
        if self.font_size <= 0.0 {
            14.0
        } else {
            self.font_size
        }
    }

    /// Arrows navigate (history in the log, cells in the grid); Enter edits the
    /// selected cell, a bare character type-to-edits; ⌘\ toggles the view; ⌘Z /
    /// ⇧⌘Z undo & redo; ⌘C/⌘X/⌘V copy/cut/paste; Escape cancels an edit. The
    /// grid-only messages are gated in `update` (they no-op while editing / in
    /// the log), so a focused editor keeps its own keys.
    fn subscription(&self) -> Subscription<Message> {
        use keyboard::key::Named;
        let keys = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => match key {
                keyboard::Key::Named(Named::ArrowUp) => {
                    Some(Message::ArrowKey(-1, 0, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowDown) => {
                    Some(Message::ArrowKey(1, 0, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowLeft) => {
                    Some(Message::ArrowKey(0, -1, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowRight) => {
                    Some(Message::ArrowKey(0, 1, modifiers.shift()))
                }
                keyboard::Key::Named(Named::Enter) => Some(Message::GridEnter),
                keyboard::Key::Named(Named::Tab) => Some(Message::AcceptSuggestion),
                keyboard::Key::Named(Named::Escape) => Some(Message::EditCanceled),
                keyboard::Key::Character(character) if modifiers.command() => {
                    match character.as_str() {
                        "\\" => Some(Message::ToggleView),
                        "z" | "Z" if modifiers.shift() => Some(Message::Redo),
                        "z" | "Z" => Some(Message::Undo),
                        "n" | "N" => Some(Message::NewWorkbook),
                        "o" | "O" => Some(Message::OpenWorkbook),
                        "s" | "S" => Some(Message::SaveWorkbook),
                        "c" | "C" => Some(Message::Copy),
                        "x" | "X" => Some(Message::Cut),
                        "v" | "V" => Some(Message::Paste),
                        "," => Some(Message::OpenSettings),
                        _ => None,
                    }
                }
                // A bare character (no ⌘/⌃/⌥) → type-to-edit in the grid.
                keyboard::Key::Character(character)
                    if !modifiers.command() && !modifiers.control() && !modifiers.alt() =>
                {
                    Some(Message::GridType(character.to_string()))
                }
                _ => None,
            },
            // Cursor position drives the auto-hiding menu bar (window-relative Y).
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::PointerMoved(position.y))
            }
            _ => None,
        });
        match shot::subscription(self) {
            Some(shot) => Subscription::batch([keys, shot]),
            None => keys,
        }
    }

    /// The window title carries the document name and unsaved-changes dot, like
    /// the AppKit original ("Soroban・算盤 — Untitled") — no in-window wordmark.
    fn window_title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        format!(
            "Soroban・算盤 — {name}{}",
            if self.is_dirty() { " •" } else { "" }
        )
    }

    /// The menu bar's File / Edit / View menus — the honest in-window stand-in
    /// for the macOS menu bar the AppKit app uses (iced has no system menu bar).
    /// Labels track state (Show Grid ↔ Show Log, Light ↔ Dark Theme).
    fn menus(&self) -> Vec<Menu<Message>> {
        let view_label = match self.mode {
            ViewMode::Log => "Show Grid",
            ViewMode::Grid => "Show Log",
        };
        vec![
            Menu::new(
                "File",
                vec![
                    MenuItem::shortcut("New", "⌘N", Message::NewWorkbook),
                    MenuItem::shortcut("Open…", "⌘O", Message::OpenWorkbook),
                    MenuItem::shortcut("Save", "⌘S", Message::SaveWorkbook),
                    MenuItem::separator(),
                    MenuItem::shortcut("Settings…", "⌘,", Message::OpenSettings),
                ],
            ),
            Menu::new(
                "Edit",
                vec![
                    MenuItem::shortcut("Undo", "⌘Z", Message::Undo),
                    MenuItem::shortcut("Redo", "⇧⌘Z", Message::Redo),
                    MenuItem::separator(),
                    MenuItem::shortcut("Copy", "⌘C", Message::Copy),
                    MenuItem::shortcut("Cut", "⌘X", Message::Cut),
                    MenuItem::shortcut("Paste", "⌘V", Message::Paste),
                ],
            ),
            Menu::new(
                "View",
                vec![
                    MenuItem::shortcut(view_label, "⌘\\", Message::ToggleView),
                    MenuItem::separator(),
                    MenuItem::action("Names", Message::ToggleInspector),
                    MenuItem::action("Reference", Message::ToggleReference),
                    MenuItem::action("Bits", Message::ToggleBinary),
                ],
            ),
        ]
    }

    fn view(&self) -> Element<'_, Message> {
        let _scope = theme::enter(self.active_palette());
        let palette = theme::tokens();

        let body = match self.mode {
            ViewMode::Log => self.log_view(&palette),
            ViewMode::Grid => self.grid_view(&palette),
        };

        // Edge-to-edge, no card — the view fills the window; the log's own input
        // bar sits at the bottom (REPL layout).
        let main = container(body)
            .padding([10, 16])
            .width(Length::Fill)
            .height(Length::Fill);

        // The main area plus any right-side sidebars (inspector / reference).
        let horizontal: Element<'_, Message> = if self.inspector_visible || self.reference_visible {
            let mut panels = row![main.width(Length::Fill)].height(Length::Fill);
            if self.inspector_visible {
                panels = panels.push(self.inspector_panel(&palette));
            }
            if self.reference_visible {
                panels = panels.push(self.reference_panel(&palette));
            }
            panels.into()
        } else {
            main.into()
        };

        // The binary bit-editor rides underneath as a full-width strip.
        let content: Element<'_, Message> = if self.binary_visible {
            column![
                container(horizontal).height(Length::Fill),
                self.binary_panel(&palette)
            ]
            .into()
        } else {
            horizontal
        };

        // The menu bar AUTO-HIDES: iced has no system menu bar, so the in-window
        // one is chrome that only earns its space while you're reaching for it.
        // Content fills the whole window; when revealed (pointer near the top, or
        // a menu open) the bar overlays the top edge rather than pushing content
        // down — so nothing jumps as it appears.
        //
        // CRUCIAL: `content` stays at a FIXED tree position (stack layer 0) whether
        // or not the bar shows — the bar is a second layer that's either the real
        // menu or a zero-size placeholder. Re-parenting `content` (wrapping it in a
        // stack only when revealed) reset the focused text field's widget state, so
        // the log input lost focus the instant the pointer neared the top edge —
        // which felt like focus being "stolen while typing".
        let bar_layer: Element<'_, Message> = if self.menu_revealed || self.menu_open.is_some() {
            let inspector_icon = button::icon(glyph::NAMES, Message::ToggleInspector);
            menu_bar_with_trailing(
                self.menus(),
                self.menu_open,
                Message::ToggleMenu,
                Some(inspector_icon.into()),
            )
        } else {
            iced::widget::Space::new()
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into()
        };
        let base: Element<'_, Message> = iced::widget::stack![content, bar_layer].into();

        // The Settings window, when open, frames itself over everything.
        if self.settings_open {
            self.settings_view(base, &palette)
        } else {
            base
        }
    }

    /// The Settings window: rime's `settings` shell (a dimmed backdrop, a section
    /// rail, and the active section's body). Appearance = theme + custom colors +
    /// font size + a live preview; Calculator = the language mode.
    fn settings_view<'a>(
        &'a self,
        base: Element<'a, Message>,
        palette: &theme::Palette,
    ) -> Element<'a, Message> {
        let content = match self.settings_section {
            1 => self.settings_calculator(palette),
            _ => self.settings_appearance(palette),
        };
        settings(
            base,
            &["Appearance", "Calculator"],
            self.settings_section,
            Message::SelectSettingsSection,
            content,
            None,
            Message::CloseSettings,
        )
    }

    /// The Appearance section: a theme picker (the ten named palettes plus a
    /// hand-editable "Custom"), the custom color rows when it's active, a font-
    /// size slider, and a live preview swatch.
    fn settings_appearance(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let mut names = themes::names();
        names.push("Custom".to_string());
        let current = self.theme_display_name().to_string();
        let theme_picker = select(names, Some(current.clone()), Message::SelectTheme);

        let mut body = column![caption("Theme"), theme_picker,].spacing(10);

        // The custom color editor — one color_field per token — shows only for
        // the "Custom" theme, seeded from the live palette.
        if current == "Custom" {
            let editable = self.active_palette();
            let mut rows = column![].spacing(8);
            for &key in theme::PALETTE_KEYS {
                if let Some(color) = editable.color(key) {
                    let owned_key = key.to_string();
                    rows = rows.push(color_field(key, color, move |c| {
                        Message::SetCustomColor(owned_key.clone(), c)
                    }));
                }
            }
            body = body.push(rows);
        }

        let size = self.base_font_size();
        body = body.push(caption("Font size"));
        body = body.push(slider(
            "",
            9.0..=28.0,
            size,
            format!("{} pt", size.round() as i32),
            Message::SetFontSize,
        ));

        body = body.push(caption("Preview"));
        body = body.push(self.settings_preview(palette));
        scrollable(body).height(Length::Fill).into()
    }

    /// A small live preview of the log: an expression echo, a result, an error,
    /// and a muted note — all in the pending palette and font size, so theme and
    /// size changes are visible without leaving Settings.
    fn settings_preview(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let size = self.base_font_size();
        let sample = column![
            text("1024 / 8").font(MONO).size(size).color(palette.accent),
            text("= 128").font(MONO).size(size).color(palette.ink),
            text("sqrt(-1)").font(MONO).size(size).color(palette.accent),
            text("domain error")
                .font(MONO)
                .size(size)
                .color(palette.danger),
            text("# a note").font(MONO).size(size).color(palette.muted),
        ]
        .spacing(6);
        card(sample)
    }

    /// The Calculator section: the language mode (input/display dialect).
    fn settings_calculator(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let modes = [
            LanguageMode::Normal,
            LanguageMode::Programmer,
            LanguageMode::Finance,
        ];
        let labels: Vec<String> = modes.iter().map(|m| title_case(m.name())).collect();
        let current = title_case(self.session.language_mode().name());
        let picker = select(labels, Some(current), move |chosen: String| {
            let mode = modes
                .iter()
                .copied()
                .find(|m| title_case(m.name()) == chosen)
                .unwrap_or(LanguageMode::Normal);
            Message::SelectMode(mode)
        });
        column![
            caption("Mode"),
            picker,
            text(
                "Normal is the everyday dialect. Programmer reads ^ & | << >> as \
                 bitwise operators; Finance tunes the display for money."
            )
            .size(12)
            .color(palette.muted),
        ]
        .spacing(10)
        .into()
    }

    /// The binary bit-editor strip: value + hex header, a width picker, a
    /// bit-format dropdown, and a clickable bit grid tinted by the active
    /// format's named fields, plus a Use button that drops the value into the
    /// input.
    fn binary_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let content: Element<'_, Message> = match self.session.binary_status() {
            BinaryStatus::Editable {
                bits,
                value,
                hex,
                width,
                signed,
                // `binary_widths()` is already empty for a locked (fixed-width)
                // value, so the width picker hides itself; nothing to do here.
                locked: _,
            } => {
                let caption = format!(
                    "{value}   {hex}   ·   {width}-bit {}",
                    if signed { "signed" } else { "unsigned" }
                );

                // The format picker: "None", every preset, then saved formats.
                let none = "None".to_string();
                let mut options = vec![none.clone()];
                options.extend(self.session.binary_preset_names());
                options.extend(self.session.saved_format_names());
                let current = self.session.binary_format_name().unwrap_or(none.clone());
                let format_picker = select(options, Some(current), move |chosen: String| {
                    if chosen == "None" {
                        Message::SetBinaryFormat(None)
                    } else {
                        Message::SetBinaryFormat(Some(chosen))
                    }
                });
                // "Build" (new) and, when a format is active, "Edit" it.
                let mut build_actions =
                    row![button::ghost("Build…", Message::BeginBuildFormat(false))].spacing(6);
                if self.session.binary_format_name().is_some() {
                    build_actions =
                        build_actions.push(button::ghost("Edit…", Message::BeginBuildFormat(true)));
                }

                let header = row![
                    text(caption).font(MONO).size(13).color(palette.accent),
                    container(row![build_actions, format_picker].spacing(8))
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                    button::secondary("Use in input", Message::UseBinary),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center);

                // The width picker (hidden when the value is locked to its
                // width): one chip per width, disabled below the minimum.
                let widths = self.session.binary_widths();
                let mut layout = column![header].spacing(12);
                if !widths.is_empty() {
                    let mut chips = row![].spacing(6);
                    for w in widths {
                        let mut chip =
                            iced::widget::button(text(w.bits.to_string()).size(12).center())
                                .padding([4, 10])
                                .style(width_chip_style(w.active, w.enabled, *palette));
                        if w.enabled && !w.active {
                            chip = chip.on_press(Message::SetBinaryWidth(w.bits));
                        }
                        chips = chips.push(chip);
                    }
                    layout = layout.push(chips);
                }

                // Decode the active format into named bands for the grid; rime
                // cycles its palette by position (owner=blue, group=green, …),
                // matching the AppKit app's field coloring.
                let bands: Vec<BitBand> = self
                    .session
                    .binary_fields()
                    .into_iter()
                    .map(|f| {
                        let label = if f.label.is_empty() {
                            f.name.clone()
                        } else {
                            format!("{} {}", f.name, f.label)
                        };
                        BitBand::new(label, f.low_bit as usize, f.width as usize)
                    })
                    .collect();

                layout = layout.push(scrollable(bit_grid(bits, bands, Message::BitToggled)));
                // Per-field editors below the grid (enum pickers, numeric
                // inputs, flag chips) — empty for a plain register.
                if let Some(fields) = self.binary_fields_view(palette) {
                    layout = layout.push(fields);
                }
                // The visual builder, when Build…/Edit… is open.
                if let Some(builder) = self.format_builder_view(width, palette) {
                    layout = layout.push(builder);
                }
                layout.into()
            }
            BinaryStatus::Unavailable(reason) => text(reason).size(13).color(palette.muted).into(),
        };

        container(card(
            column![
                Self::panel_header("Binary", Message::ToggleBinary, palette),
                content
            ]
            .spacing(12),
        ))
        .padding(iced::Padding {
            top: 0.0,
            right: 20.0,
            bottom: 20.0,
            left: 20.0,
        })
        .into()
    }

    /// Rebuild the numeric-field draft map from the register's current values,
    /// so a text input can borrow its value from `self` (living as long as the
    /// view) and re-syncs after any bit/width/format/field change.
    fn sync_binary_field_drafts(&mut self) {
        self.binary_field_drafts = self
            .session
            .binary_fields()
            .into_iter()
            .filter(|f| matches!(f.kind, BinaryFieldKind::Numeric | BinaryFieldKind::Unused))
            .map(|f| (f.name, f.value_text))
            .collect();
    }

    /// The per-field editor strip for the active bit-format: one card per field
    /// carrying the right control — a picker for an enum, a text input for a
    /// numeric field, clickable chips for flags, a dimmed lock for a reserved
    /// gap. `None` when no format is applied.
    fn binary_fields_view(&self, palette: &theme::Palette) -> Option<Element<'_, Message>> {
        // Own the palette (it's Copy) so the field-card closures don't borrow it
        // — the returned Element then borrows only `self`.
        let palette = *palette;
        let fields = self.session.binary_fields();
        if fields.is_empty() {
            return None;
        }
        let mut cards = row![].spacing(10);
        for f in fields {
            let name = f.name.clone();
            let header = text(format!(
                "{} [{}:{}]",
                f.name,
                f.low_bit + f.width - 1,
                f.low_bit
            ))
            .size(11)
            .color(palette.muted);
            let editor: Element<'_, Message> = match f.kind {
                BinaryFieldKind::Enum => {
                    let options = f.options.clone();
                    let selected = f.selected.and_then(|i| options.get(i).cloned());
                    let lookup = options.clone();
                    let field_name = name.clone();
                    select(options, selected, move |chosen: String| {
                        let index = lookup.iter().position(|o| *o == chosen).unwrap_or(0);
                        Message::SetBinaryField(field_name.clone(), index.to_string())
                    })
                    .into()
                }
                BinaryFieldKind::Numeric | BinaryFieldKind::Unused => {
                    // The value is borrowed from the drafts map (kept in sync
                    // with the register), so it lives as long as this Element.
                    let value: &str = self
                        .binary_field_drafts
                        .get(&name)
                        .map(String::as_str)
                        .unwrap_or("");
                    let submit_text = value.to_string();
                    let input_name = name.clone();
                    let submit_name = name.clone();
                    text_field("", value, move |text| {
                        Message::BinaryFieldInput(input_name.clone(), text)
                    })
                    .on_submit(Message::SetBinaryField(submit_name, submit_text))
                    .into()
                }
                BinaryFieldKind::Flags => {
                    let mut chips = row![].spacing(4);
                    for bit in f.flags {
                        chips = chips.push(
                            iced::widget::button(
                                column![
                                    text(bit.name).size(10).center(),
                                    text(if bit.set { "1" } else { "0" }).size(12).center(),
                                ]
                                .align_x(iced::Alignment::Center),
                            )
                            .padding([2, 6])
                            .on_press(Message::BitToggled(bit.bit as usize))
                            .style(width_chip_style(bit.set, true, palette)),
                        );
                    }
                    chips.into()
                }
                BinaryFieldKind::Reserved => text(format!("reserved · {}", f.value_text))
                    .size(12)
                    .color(palette.muted)
                    .into(),
            };
            cards = cards.push(
                container(column![header, editor].spacing(4))
                    .padding(8)
                    .style(move |_theme| container::background(palette.surface)),
            );
        }
        Some(scrollable(cards).into())
    }

    /// The visual format builder (Build new… / Edit current…): claim a run of
    /// the free bits, describe the pending field (name / kind / labels / base),
    /// Add it; the committed fields list with per-row remove, then Apply
    /// (transient) or Save (named). `None` unless the builder is open.
    fn format_builder_view(
        &self,
        register_width: u32,
        palette: &theme::Palette,
    ) -> Option<Element<'_, Message>> {
        let palette = *palette;
        let builder = self.session.format_builder()?;
        let free = builder.free_bits(register_width);

        // Claim buttons: 1..=free bits (capped for a sane row width).
        let mut claim = row![text("Claim").size(12).color(palette.muted)]
            .spacing(4)
            .align_y(iced::Alignment::Center);
        for bits in 1..=free.min(16) {
            let active = builder.pending_width() == bits;
            claim = claim.push(
                iced::widget::button(text(bits.to_string()).size(12).center())
                    .padding([2, 8])
                    .on_press(Message::BuilderClaim(bits))
                    .style(width_chip_style(active, true, palette)),
            );
        }

        // Draft inputs: name (unless a gap), kind picker, labels (flags/enum),
        // base (numeric), Add.
        let kinds: Vec<String> = FormatBuilderFieldKind::ALL
            .iter()
            .map(|k| k.raw_value().to_string())
            .collect();
        let kind_picker = select(
            kinds,
            Some(builder.draft_kind.raw_value().to_string()),
            |chosen: String| {
                let kind = FormatBuilderFieldKind::ALL
                    .into_iter()
                    .find(|k| k.raw_value() == chosen)
                    .unwrap_or(FormatBuilderFieldKind::Numeric);
                Message::BuilderDraftKind(kind)
            },
        );
        let mut draft = row![].spacing(6).align_y(iced::Alignment::Center);
        if !builder.is_gap_kind() {
            draft = draft.push(
                text_field("field name", &builder.draft_name, Message::BuilderDraftName)
                    .width(Length::Fixed(140.0)),
            );
        }
        draft = draft.push(kind_picker);
        if matches!(
            builder.draft_kind,
            FormatBuilderFieldKind::Flags | FormatBuilderFieldKind::Enumeration
        ) {
            draft = draft.push(
                text_field(
                    "labels, comma-separated",
                    &builder.draft_labels,
                    Message::BuilderDraftLabels,
                )
                .width(Length::Fixed(220.0)),
            );
        }
        if matches!(builder.draft_kind, FormatBuilderFieldKind::Numeric) {
            let dec = builder.draft_base == 10;
            draft = draft
                .push(
                    iced::widget::button(text("dec").size(11))
                        .padding([2, 8])
                        .on_press(Message::BuilderDraftBase(10))
                        .style(width_chip_style(dec, true, palette)),
                )
                .push(
                    iced::widget::button(text("hex").size(11))
                        .padding([2, 8])
                        .on_press(Message::BuilderDraftBase(16))
                        .style(width_chip_style(!dec, true, palette)),
                );
        }
        draft = draft.push(button::secondary("Add field", Message::BuilderAddField));

        // Committed fields, each with a remove button.
        let mut fields = column![].spacing(4);
        for f in builder.fields() {
            fields = fields.push(
                row![
                    text(format!(
                        "{} · {} bits · {}",
                        f.name,
                        f.width,
                        f.kind.raw_value()
                    ))
                    .size(12)
                    .color(palette.ink),
                    iced::widget::button(icons::icon(glyph::CLOSE).size(11))
                        .padding([1, 6])
                        .on_press(Message::BuilderRemoveField(f.id)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }

        let footer = row![
            text(format!("{free} free")).size(12).color(palette.muted),
            button::ghost("Apply", Message::ApplyBuiltFormat),
            text_field(
                "save as…",
                &self.builder_save_name,
                Message::BuilderSaveName
            )
            .width(Length::Fixed(120.0)),
            button::secondary("Save", Message::SaveFormat),
            button::ghost("Cancel", Message::CancelBuildFormat),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let panel = column![
            text("Build format").size(13).color(palette.ink),
            claim,
            draft,
            fields,
            footer,
        ]
        .spacing(8);
        Some(
            container(panel)
                .padding(10)
                .style(move |_theme| container::background(palette.surface))
                .into(),
        )
    }

    /// The reference window: every function, operator, and constant — the
    /// user's own first — with a live search filter.
    /// A sidebar panel's title row: the title on the left, a × on the right that
    /// closes the panel (fires `close`).
    fn panel_header<'a>(
        title: &'a str,
        close: Message,
        palette: &theme::Palette,
    ) -> Element<'a, Message> {
        row![
            text(title).size(15).color(palette.ink),
            container(button::icon(glyph::CLOSE, close))
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn reference_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let search = text_field(
            "Search the reference…",
            &self.reference_query,
            Message::ReferenceQueryChanged,
        );
        let mut list = column![].spacing(14);
        let groups = self.session.reference(&self.reference_query);
        if groups.is_empty() {
            list = list.push(text("No matches.").size(12).color(palette.muted));
        }
        for group in groups {
            let mut group_column = column![section(&group.title)].spacing(8);
            for entry in group.entries {
                group_column = group_column.push(
                    column![
                        text(entry.signature)
                            .font(MONO)
                            .size(12)
                            .color(palette.accent),
                        text(entry.summary).size(11).color(palette.muted),
                    ]
                    .spacing(2),
                );
            }
            list = list.push(group_column);
        }

        container(card(
            column![
                Self::panel_header("Reference", Message::ToggleReference, palette),
                search,
                scrollable(list).height(Length::Fill),
            ]
            .spacing(12),
        ))
        .width(Length::Fixed(320.0))
        .padding(20)
        .height(Length::Fill)
        .into()
    }

    /// The names inspector: every live variable (log vars, named cells, sheet 𝑖
    /// definitions), function, and data type — grouped into three sections like
    /// the original, each row tagged with its provenance (`log` or a clickable
    /// `B:2 ↗` that jumps to the cell).
    fn inspector_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let mut sections = column![Self::panel_header(
            "Environment",
            Message::ToggleInspector,
            palette
        )]
        .spacing(16);
        let groups = [
            ("VARIABLES", self.session.inspector_variables()),
            ("FUNCTIONS", self.session.inspector_functions()),
            ("DATA TYPES", self.session.inspector_data_types()),
        ];
        let mut any = false;
        for (title, rows) in groups {
            if rows.is_empty() {
                continue;
            }
            any = true;
            // A small-caps muted section heading, like the original.
            let mut group = column![text(title).size(11).color(palette.muted)].spacing(8);
            for row in rows {
                let mut line = column![row![
                    text(row.label).font(MONO).size(12).color(palette.accent),
                    container(origin_tag(row.origin, palette))
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                ]
                .align_y(iced::Alignment::Center)]
                .spacing(1);
                if !row.detail.is_empty() {
                    line = line.push(text(row.detail).font(MONO).size(11).color(palette.muted));
                }
                group = group.push(line);
            }
            sections = sections.push(group);
        }
        if !any {
            sections = sections.push(
                text("Nothing defined yet — assign a variable or name a cell.")
                    .size(12)
                    .color(palette.muted),
            );
        }

        container(card(scrollable(sections).height(Length::Fill)))
            .width(Length::Fixed(260.0))
            .padding(20)
            .height(Length::Fill)
            .into()
    }

    fn log_view(&self, palette: &theme::Palette) -> Element<'_, Message> {
        // The log fills, oldest→newest, so the freshest result sits just above
        // the input — the terminal/REPL layout of the AppKit original.
        let size = self.base_font_size();
        let entries = self.session.entries(); // Ref over the shared log tape
        // Keep this slot a `container` in BOTH states (empty vs. populated) so the
        // input below it doesn't re-parent — and lose focus — on the first submit.
        let log_inner: Element<'_, Message> = if entries.is_empty() {
            self.empty_log(palette)
        } else {
            let mut items = column![].spacing(12);
            for entry in entries.iter() {
                items = items.push(entry_view(&entry.input, &entry.outcome, palette, size));
            }
            scrollable(items.padding([4, 8]))
                .height(Length::Fill)
                .into()
        };
        let log = container(log_inner).height(Length::Fill);

        // The input is pinned to the BOTTOM, behind a `>` prompt; Enter submits
        // (no `=` button — the original has none). A mode affordance (cycles
        // Normal → Programmer → Finance) sits at the left of the corner icons
        // (docs / grid), like the AppKit app's input-bar mode control.
        let mode_label = match self.session.language_mode() {
            LanguageMode::Normal => "Normal",
            LanguageMode::Programmer => "Programmer",
            LanguageMode::Finance => "Finance",
        };
        let input_bar = row![
            text(">").font(MONO).size(size + 2.0).color(palette.muted),
            text_field("Expression", self.session.input(), Message::InputChanged)
                .id(log_input_id())
                .on_submit(Message::Submit)
                .size(size)
                .font(MONO),
            button::ghost(mode_label, Message::CycleMode),
            button::icon(glyph::REFERENCE, Message::ToggleReference),
            button::icon(glyph::GRID, Message::ToggleView),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // The popup sits just ABOVE the bottom-anchored input (a REPL wants its
        // completions rising from the prompt, not dropping off-screen). The
        // input bar stays at a FIXED position in the column — a zero-height
        // placeholder stands in when there's no popup — so the text field never
        // shifts index and thus never loses focus mid-type (an iced tree-diff
        // quirk: a widget that changes tree position is rebuilt from scratch).
        let popup: Element<'_, Message> = self.suggestion_popup().unwrap_or_else(|| {
            // A zero-height CONTAINER (not a Space) so this slot is always the same
            // widget type as the real popup — the input below it never re-parents.
            container(iced::widget::Space::new().height(Length::Fixed(0.0))).into()
        });
        let bottom = column![popup, input_bar].spacing(4);
        column![log, bottom].spacing(12).into()
    }

    /// The empty-state: an invitation plus a few sample expressions that insert
    /// themselves into the input on click (the original's "double-click one").
    fn empty_log(&self, palette: &theme::Palette) -> Element<'_, Message> {
        const SAMPLES: [&str; 3] = [
            "map(n -> n * n, filter(x -> x % 2 == 0, seq(1, 20)))",
            "fact(52) / (fact(5) * fact(47))",
            "0.1 + 0.2",
        ];
        let mut column = column![text("Type an expression below — or click one:")
            .size(14)
            .color(palette.muted)]
        .spacing(10);
        for sample in SAMPLES {
            column = column.push(
                mouse_area(text(sample).font(MONO).size(14).color(palette.accent))
                    .on_press(Message::SampleClicked(sample.to_string()))
                    .interaction(iced::mouse::Interaction::Pointer),
            );
        }
        container(column).padding(12).height(Length::Fill).into()
    }

    fn grid_view(&self, palette: &theme::Palette) -> Element<'_, Message> {
        // The formula/edit bar: the active cell's address, then its raw content.
        // Click it (or just start typing) to edit; Enter commits, Esc cancels.
        let address_label = self
            .active_cell()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "—".to_string());
        let edit_bar = row![
            container(text(address_label).font(MONO).size(13).color(palette.muted))
                .width(Length::Fixed(48.0))
                .center_y(Length::Shrink),
            // The name box (Excel-style): name the selected cell's location.
            container(
                text_field("name", &self.name_draft, Message::NameChanged)
                    .on_submit(Message::NameCommitted)
            )
            .width(Length::Fixed(150.0)),
            text_field(
                "Type a value or formula — click a cell to insert its reference",
                &self.edit_draft,
                Message::EditChanged
            )
            .id(edit_bar_id())
            .on_submit(Message::EditSubmitted)
            .font(MONO),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Controls now render inline in their cells (below); the header keeps the
        // formula/name bar, the autocomplete popup (dropping below the top-
        // anchored bar), and the format bar.
        let mut header = column![edit_bar].spacing(12);
        if let Some(popup) = self.suggestion_popup() {
            header = header.push(popup);
        }
        if let Some(bar) = self.format_bar() {
            header = header.push(bar);
        }

        let palette = *palette;
        let session = &self.session;
        let mut sheet = grid(GRID_ROWS, GRID_COLS, move |row, col| {
            let address = CellAddress::new(col, row);
            render_cell(
                session.display_at(address),
                &session.cell_format(address),
                &palette,
            )
        })
        .offset(self.grid_offset)
        .selection(self.grid_selection)
        .column_widths(self.session.column_widths())
        .on_scroll(Message::GridScrolled)
        .on_select(Message::GridSelected)
        .on_activate(Message::EditActivated)
        .on_resize_column(Message::ColumnResized);

        // Host each control (slider / stepper / checkbox / dropdown) as an
        // interactive widget inside its own cell — the AppKit behavior — except
        // the cell currently being edited (the editor takes that one).
        let editing_cell = self.editing.then(|| self.active_cell()).flatten();
        for (address, display) in self.session.control_cells() {
            if Some(address) == editing_cell {
                continue;
            }
            if let Some(widget) = control_widget(address, display) {
                sheet = sheet.overlay(address.row, address.column, widget);
            }
        }

        // While editing, host an inline text editor over the active cell — the
        // cell edits in place (the AppKit behavior), mirroring the formula bar.
        if self.editing {
            if let Some((row, col)) = self.grid_selection.map(|s| s.anchor) {
                let editor = iced::widget::text_input("", &self.edit_draft)
                    .id(grid_editor_id())
                    .on_input(Message::EditChanged)
                    .on_submit(Message::EditSubmitted)
                    .padding(2)
                    .size(13)
                    .font(MONO);
                sheet = sheet.editor(row, col, editor);
            }
        }

        // A sheet-tab strip at the bottom-left, like the original's `Mortgage +`,
        // with a log/grid view-toggle icon pinned bottom-right (the AppKit app's
        // corner affordance).
        let sheet_tab = row![
            container(
                text(self.session.active_sheet_name())
                    .font(MONO)
                    .size(13)
                    .color(palette.ink)
            )
            .padding([4, 12])
            .style(move |_| container::background(palette.surface)),
            text("+").size(15).color(palette.muted),
            container(text("").size(1)).width(Length::Fill),
            button::icon(glyph::LOG, Message::ToggleView),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        column![header, container(sheet).height(Length::Fill), sheet_tab,]
            .spacing(12)
            .into()
    }

    /// The format bar for the active cell: number format, alignment, and text /
    /// fill color. Each change commits an undoable, display-only format edit.
    fn format_bar(&self) -> Option<Element<'_, Message>> {
        let address = self.active_cell()?;
        let format = self.session.cell_format(address);
        Some(
            row![
                labeled_select(
                    "Format",
                    &NUMBER_FORMAT_LABELS,
                    number_format_index(&format.number_format),
                    Message::SetNumberFormat,
                ),
                labeled_select(
                    "Align",
                    &ALIGN_LABELS,
                    align_index(format.alignment),
                    Message::SetAlignment,
                ),
                labeled_select(
                    "Text",
                    &COLOR_LABELS,
                    color_index(format.text_color),
                    Message::SetTextColor,
                ),
                labeled_select(
                    "Fill",
                    &COLOR_LABELS,
                    color_index(format.fill_color),
                    Message::SetFillColor,
                ),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center)
            .into(),
        )
    }
}

/// A small `label [ picker ]` cluster: a dropdown over `options` showing the
/// one at `selected`, emitting `message(index)` on a pick.
fn labeled_select<'a>(
    label: &'a str,
    options: &[&'static str],
    selected: usize,
    message: impl Fn(usize) -> Message + 'a,
) -> Element<'a, Message> {
    let options: Vec<String> = options.iter().map(|option| option.to_string()).collect();
    let current = options.get(selected).cloned();
    let lookup = options.clone();
    let picker = select(options, current, move |chosen: String| {
        let index = lookup.iter().position(|o| *o == chosen).unwrap_or(0);
        message(index)
    });
    row![text(label).size(12), picker]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

/// Render a cell for the grid: the display drives the base text/alignment,
/// then the cell's format overrides the number rendering, alignment, and
/// The engine tags sheet-scoped definitions with math-alphanumeric markers
/// (`𝑫` data, `𝑖` variable) that neither the text nor the icon font renders —
/// they'd show as tofu. Swap them for plain letters for display (`λ` for
/// functions renders fine, so it's left alone). Display-only; the engine's
/// canonical marker is untouched.
fn renderable_definition(marker: String) -> String {
    marker.replace('𝑫', "D").replace('𝑖', "i")
}

/// colors on top.
fn render_cell(display: CellDisplay, format: &CellFormat, palette: &theme::Palette) -> GridCell {
    let mut cell = base_cell(display, format, palette);
    if let Some(align) = alignment_override(format.alignment) {
        cell = cell.align(align);
    }
    if let Some(color) = format.text_color {
        cell = cell.text_color(palette_color(color));
    }
    if let Some(color) = format.fill_color {
        cell = cell.background(fill_color(color));
    }
    cell
}

/// The base cell from a display: numbers right-align (rendered through the
/// cell's number format), labels left-align, errors show `#ERR`, definitions
/// and notes get their glyph text.
fn base_cell(display: CellDisplay, format: &CellFormat, palette: &theme::Palette) -> GridCell {
    match display {
        CellDisplay::Empty => GridCell::default(),
        CellDisplay::Text(label) => GridCell::new(label),
        CellDisplay::Value(number) => GridCell::right(format.number_format.rendered(&number)),
        CellDisplay::Error(_) => GridCell::new("#ERR")
            .align(CellAlign::Center)
            .text_color(palette.danger),
        CellDisplay::Definition(marker) => {
            GridCell::new(renderable_definition(marker)).text_color(palette.accent)
        }
        CellDisplay::Note(note) => GridCell::new(note).text_color(palette.muted),
        // Controls render as interactive overlay widgets in their cells (see
        // `control_widget`), so the painted cell underneath is left empty.
        CellDisplay::Slider(_)
        | CellDisplay::Stepper(_)
        | CellDisplay::Checkbox(_)
        | CellDisplay::Dropdown(_) => GridCell::default(),
    }
}

/// The number-format presets offered in the format bar, in menu order.
const NUMBER_FORMAT_LABELS: [&str; 7] = [
    "General", "Number", "Currency", "Percent", "Date", "Hex", "Binary",
];

fn number_format_at(index: usize) -> NumberFormat {
    match index {
        1 => NumberFormat::Number { decimals: 2 },
        2 => NumberFormat::Currency {
            symbol: "$".to_string(),
            decimals: 2,
        },
        3 => NumberFormat::Percent { decimals: 2 },
        4 => NumberFormat::Date,
        5 => NumberFormat::Hex,
        6 => NumberFormat::Binary,
        _ => NumberFormat::General,
    }
}

fn number_format_index(format: &NumberFormat) -> usize {
    match format {
        NumberFormat::General => 0,
        NumberFormat::Number { .. } => 1,
        NumberFormat::Currency { .. } => 2,
        NumberFormat::Percent { .. } => 3,
        NumberFormat::Date => 4,
        NumberFormat::Hex => 5,
        NumberFormat::Binary => 6,
    }
}

const ALIGN_LABELS: [&str; 4] = ["Auto", "Left", "Center", "Right"];

fn align_index(alignment: CellAlignment) -> usize {
    CellAlignment::ALL
        .iter()
        .position(|&a| a == alignment)
        .unwrap_or(0)
}

fn alignment_override(alignment: CellAlignment) -> Option<CellAlign> {
    match alignment {
        CellAlignment::Auto => None,
        CellAlignment::Left => Some(CellAlign::Left),
        CellAlignment::Center => Some(CellAlign::Center),
        CellAlignment::Right => Some(CellAlign::Right),
    }
}

const COLOR_LABELS: [&str; 8] = [
    "None", "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray",
];

fn color_choice(index: usize) -> Option<PaletteColor> {
    index
        .checked_sub(1)
        .and_then(|i| PaletteColor::ALL.get(i).copied())
}

fn color_index(color: Option<PaletteColor>) -> usize {
    match color {
        None => 0,
        Some(color) => PaletteColor::ALL
            .iter()
            .position(|&c| c == color)
            .map(|i| i + 1)
            .unwrap_or(0),
    }
}

/// A semantic palette color as an approximate display color. (The Swift app
/// maps these to theme-adaptive system colors; fixed values are a first cut.)
fn palette_color(color: PaletteColor) -> Color {
    match color {
        PaletteColor::Red => Color::from_rgb(0.90, 0.30, 0.24),
        PaletteColor::Orange => Color::from_rgb(0.93, 0.56, 0.18),
        PaletteColor::Yellow => Color::from_rgb(0.85, 0.70, 0.15),
        PaletteColor::Green => Color::from_rgb(0.30, 0.70, 0.36),
        PaletteColor::Blue => Color::from_rgb(0.28, 0.55, 0.90),
        PaletteColor::Purple => Color::from_rgb(0.60, 0.42, 0.85),
        PaletteColor::Gray => Color::from_rgb(0.55, 0.58, 0.62),
    }
}

/// The same hue as a translucent cell fill, so text stays readable over it.
fn fill_color(color: PaletteColor) -> Color {
    Color {
        a: 0.25,
        ..palette_color(color)
    }
}

/// Move a grid selection by `(drow, dcol)`, clamped to the grid bounds. Extend
/// keeps the anchor and moves the extent (shift-arrow); a plain move relocates
/// the whole single-cell selection. Pure, so the clamping is unit-tested.
fn next_selection(current: GridSelection, drow: i32, dcol: i32, extend: bool) -> GridSelection {
    let clamp = |value: usize, delta: i32, limit: usize| -> usize {
        (value as i32 + delta).clamp(0, limit as i32 - 1) as usize
    };
    if extend {
        let (row, col) = current.extent;
        GridSelection {
            anchor: current.anchor,
            extent: (clamp(row, drow, GRID_ROWS), clamp(col, dcol, GRID_COLS)),
        }
    } else {
        let (row, col) = current.anchor;
        GridSelection::cell(clamp(row, drow, GRID_ROWS), clamp(col, dcol, GRID_COLS))
    }
}

/// The style for one width-picker chip: filled when `active`, dimmed when
/// disabled (too small to hold the value/format), a plain outline otherwise.
fn width_chip_style(
    active: bool,
    enabled: bool,
    palette: theme::Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, _status| {
        let (background, text_color) = if active {
            (Some(palette.accent.into()), palette.bg)
        } else if !enabled {
            (None, palette.hairline)
        } else {
            (None, palette.ink)
        };
        iced::widget::button::Style {
            background,
            text_color,
            border: iced::Border {
                color: palette.hairline,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..iced::widget::button::Style::default()
        }
    }
}

/// A compact interactive control widget for a cell, hosted in place over it via
/// the grid's overlay mechanism. The cell address rides each message, since many
/// controls can be live at once. Returns `None` for non-control displays.
fn control_widget<'a>(address: CellAddress, display: CellDisplay) -> Option<Element<'a, Message>> {
    match display {
        CellDisplay::Slider(info) => {
            let range = (info.minimum.to_f64() as f32)..=(info.maximum.to_f64() as f32);
            let value = info.value.to_f64() as f32;
            Some(slider("", range, value, info.value.to_string(), move |v| {
                Message::SliderChanged(address, v)
            }))
        }
        CellDisplay::Stepper(info) => Some(stepper(
            "",
            info.value.to_string(),
            Message::StepperStepped(address, false),
            Message::StepperStepped(address, true),
        )),
        CellDisplay::Checkbox(info) => {
            Some(toggle("", info.is_on, Message::CheckboxToggled(address)))
        }
        CellDisplay::Dropdown(info) => {
            let options: Vec<String> = info
                .options
                .iter()
                .map(|value| value.display_description())
                .collect();
            let selected = info.value.display_description();
            let lookup = options.clone();
            Some(
                select(options, Some(selected), move |chosen: String| {
                    let index = lookup
                        .iter()
                        .position(|option| *option == chosen)
                        .unwrap_or(0);
                    Message::DropdownPicked(address, index)
                })
                .into(),
            )
        }
        _ => None,
    }
}

/// An inspector row's provenance tag: `log` (muted, inert) or a clickable
/// `B:2 ↗` (accent) that jumps to the cell.
fn origin_tag<'a>(origin: Origin, palette: &theme::Palette) -> Element<'a, Message> {
    match origin {
        Origin::Log => text("log").size(11).color(palette.muted).into(),
        Origin::Cell(address) => mouse_area(
            text(format!("{address} ↗"))
                .font(MONO)
                .size(11)
                .color(palette.accent),
        )
        .on_press(Message::JumpTo(address))
        .interaction(iced::mouse::Interaction::Pointer)
        .into(),
    }
}

/// One log entry: the echoed input, then its outcome (a value, a definition, a
/// note, a raw block, or an error with an aligned caret).
fn entry_view<'a>(
    input: &str,
    outcome: &Outcome,
    palette: &theme::Palette,
    size: f32,
) -> Element<'a, Message> {
    // Secondary lines (comments, info, error text) read one point smaller.
    let small = (size - 1.0).max(1.0);
    // Echoed input in accent, no prefix — matching the original, where the
    // expression is the colored line and the result below it is plain ink.
    let echo = text(input.to_string())
        .font(MONO)
        .size(size)
        .color(palette.accent);

    let result: Element<'a, Message> = match outcome {
        Outcome::Value(value) => text(format!("= {value}"))
            .font(MONO)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Function(signature) => text(format!("λ {signature}"))
            .font(MONO)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Data(declaration) => text(format!("D {declaration}"))
            .font(MONO)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Comment(note) => text(format!("# {note}"))
            .font(MONO)
            .size(small)
            .color(palette.muted)
            .into(),
        Outcome::Info(block) => text(block.clone())
            .font(MONO)
            .size(small)
            .color(palette.ink)
            .into(),
        Outcome::Error { message, position } => {
            let mut lines = column![].spacing(2);
            if let Some(position) = position {
                // No echo prefix now, so the caret aligns directly under column.
                let caret = format!("{}^", " ".repeat(*position));
                lines = lines.push(text(caret).font(MONO).size(size).color(palette.danger));
            }
            lines
                .push(
                    text(format!("error: {message}"))
                        .size(small)
                        .color(palette.danger),
                )
                .into()
        }
    };

    column![echo, result].spacing(2).into()
}

impl App {
    /// The initial state: `App::default`, then the screenshot harness gets a
    /// chance to seed it (a no-op unless `SOROBAN_SHOT` is set — see [`shot`]).
    fn launch() -> Self {
        let mut app = App {
            theme_name: themes::default_name().to_string(),
            font_size: 14.0,
            ..App::default()
        };
        shot::configure(&mut app);
        app
    }
}

fn main() -> iced::Result {
    iced::application(App::launch, App::update, App::view)
        .title(App::window_title)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(icons::FONT_BYTES) // the embedded icon font (toolbar/toggle/close glyphs)
        .window_size(iced::Size::new(1040.0, 680.0))
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_relocates_single_cell() {
        let next = next_selection(GridSelection::cell(4, 2), 1, 0, false);
        assert_eq!(next.anchor, (5, 2));
        assert_eq!(next.extent, (5, 2));
    }

    #[test]
    fn move_clamps_at_the_top_left_corner() {
        let next = next_selection(GridSelection::cell(0, 0), -1, -1, false);
        assert_eq!(next.anchor, (0, 0));
    }

    #[test]
    fn move_clamps_at_the_bottom_right_corner() {
        let start = GridSelection::cell(GRID_ROWS - 1, GRID_COLS - 1);
        let next = next_selection(start, 1, 1, false);
        assert_eq!(next.anchor, (GRID_ROWS - 1, GRID_COLS - 1));
    }

    #[test]
    fn extend_holds_the_anchor_and_moves_the_extent() {
        let start = GridSelection::cell(4, 2);
        let next = next_selection(start, 2, 1, true);
        assert_eq!(next.anchor, (4, 2));
        assert_eq!(next.extent, (6, 3));
    }
}
