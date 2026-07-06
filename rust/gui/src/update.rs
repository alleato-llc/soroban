//! The `update` reducer and its state-mutation helpers — the shell's
//! event handling, separated from the view builders.

use iced::widget::operation;
use iced::{Point, Task};
use rime::widgets::menu;
use rime::widgets::{rename_field_id, GridSelection};
use soroban_engine::{CellAlignment, LanguageMode};

use crate::render::*;
use crate::shot;
use crate::{grid_editor_id, log_input_id, App, Message, ViewMode};

impl App {
    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        // Any real action closes an open menu (the backdrop only closes on an
        // outside click); the toggle itself opens/switches menus, the screenshot
        // harness's background frames must leave it be, and — crucially now that
        // the menu bar auto-hides — plain cursor movement must NOT close it, or
        // reaching for a submenu item would dismiss the menu before you got there.
        if self.menu_open.is_some()
            && !matches!(
                message,
                Message::ToggleMenu(_) | Message::Shot(_) | Message::PointerMoved(_, _)
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
            Message::ZoomFont(delta) => {
                self.font_size = (self.base_font_size() + delta).clamp(9.0, 28.0)
            }
            Message::ResetFontSize => self.font_size = 14.0,
            Message::SelectFont(name) => self.font_name = name,
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
                // Then an open sheet-rename bar.
                if self.sheet_rename_draft.is_some() {
                    self.sheet_rename_draft = None;
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
            // commit it (display-only, undoable), then close the context menu.
            Message::SetNumberFormat(index) => {
                self.apply_format(|format| format.number_format = number_format_at(index));
                self.close_cell_menu();
            }
            Message::SetAlignment(index) => {
                self.apply_format(|format| format.alignment = CellAlignment::ALL[index]);
                self.close_cell_menu();
            }
            Message::SetTextColor(index) => {
                self.apply_format(|format| format.text_color = color_choice(index));
                self.close_cell_menu();
            }
            Message::SetFillColor(index) => {
                self.apply_format(|format| format.fill_color = color_choice(index));
                self.close_cell_menu();
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
            Message::OpenCsv => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .pick_file()
                {
                    // Opens an editable COPY (a SQLite-backed data sheet); the
                    // source .csv is never written back — edits land in the
                    // .soroban file on Save.
                    if self.session.import_csv(&path).is_ok() {
                        self.mode = ViewMode::Grid;
                        self.load_draft();
                        self.after_document_change();
                    }
                }
            }
            // Multi-sheet: append + switch, click-to-switch, double-click rename,
            // delete. The AppKit app's `SheetTabBar` behavior.
            Message::AddSheet => {
                if self.session.add_sheet().is_ok() {
                    self.mode = ViewMode::Grid;
                    self.sheet_rename_draft = None;
                    self.reset_sheet_view();
                }
            }
            Message::ActivateSheet(index) => {
                self.sheet_rename_draft = None; // a tab click cancels an open rename
                self.session.activate_sheet(index);
                self.reset_sheet_view();
            }
            Message::BeginRenameSheet(index) => {
                self.mode = ViewMode::Grid; // the rename bar lives under the grid strip
                self.session.activate_sheet(index);
                self.reset_sheet_view();
                self.sheet_rename_draft = Some(self.session.active_sheet_name());
                return operation::focus(rename_field_id());
            }
            Message::SheetRenameChanged(text) => {
                if let Some(draft) = self.sheet_rename_draft.as_mut() {
                    *draft = text;
                }
            }
            Message::SheetRenameCommitted => {
                if let Some(draft) = self.sheet_rename_draft.clone() {
                    // On a valid rename, close the bar; on an invalid/duplicate
                    // name, keep it open so the typed name can be corrected.
                    if self.session.rename_active_sheet(&draft).is_ok() {
                        self.sheet_rename_draft = None;
                        self.load_draft();
                    }
                }
            }
            Message::DeleteSheet => {
                if self.session.remove_active_sheet().is_ok() {
                    self.sheet_rename_draft = None;
                    self.reset_sheet_view();
                }
            }
            // Copy / cut / paste the grid selection as TSV (Excel-interop). Only
            // in the grid — in the log, ⌘C/⌘V fall through to normal text copy.
            Message::Copy => {
                self.close_cell_menu();
                if self.mode == ViewMode::Grid {
                    if let Some((r0, r1, c0, c1)) = self.selection_bounds() {
                        return iced::clipboard::write(self.session.selection_tsv(r0, r1, c0, c1));
                    }
                }
            }
            Message::Cut => {
                self.close_cell_menu();
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
                self.close_cell_menu();
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
            // the cursor drifts onto a menu item). Also record the cursor for the
            // right-click cell menu.
            Message::PointerMoved(x, y) => {
                self.cursor = (x, y);
                self.menu_revealed = y <= menu::BAR_HEIGHT + 8.0;
            }
            // Right-click on a selected grid cell → the cell context menu.
            Message::OpenCellMenu => {
                if self.mode == ViewMode::Grid && self.active_cell().is_some() {
                    self.cell_menu = Some(Point::new(self.cursor.0, self.cursor.1));
                    self.cell_menu_submenu = None;
                }
            }
            Message::CloseCellMenu => self.close_cell_menu(),
            Message::ExpandCellSubmenu(which) => self.cell_menu_submenu = which,
            Message::DeleteSelection => {
                if let Some((r0, r1, c0, c1)) = self.selection_bounds() {
                    self.session.clear_range(r0, r1, c0, c1);
                    self.load_draft();
                }
                self.close_cell_menu();
            }
            Message::Shot(event) => return shot::handle(self, event),
        }
        Task::none()
    }
}
