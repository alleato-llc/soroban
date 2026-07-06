//! State-mutation helpers shared by the `update` reducer: draft loading,
//! suggestions, selection/point-mode, formatting, and the cell menu.

use iced::widget::operation;
use iced::{Element, Task, Vector};
use rime::widgets::{suggestion_list, GridSelection, MenuItem, Suggestion};
use soroban_engine::{CellAddress, CellFormat};
use soroban_gui::session::{PointClick, Session};

use crate::render::*;
use crate::{edit_bar_id, grid_editor_id, log_input_id, App, Message, ViewMode};

impl App {
    /// After a New/Open replaces the document, drop the transient view state and
    /// mark the session clean at its current revision.
    pub(crate) fn after_document_change(&mut self) {
        self.grid_selection = None;
        self.editing = false;
        self.saved_revision = self.session.revision();
        self.load_draft();
    }

    /// Reset the grid's view state after the *active sheet* changes (switch /
    /// add / delete): drop the selection, scroll, and any open editor, and
    /// reload the edit bar for the now-active sheet. Unlike
    /// [`after_document_change`](Self::after_document_change) it leaves
    /// `saved_revision` alone, so an add/delete still reads as dirty.
    pub(crate) fn reset_sheet_view(&mut self) {
        self.grid_selection = None;
        self.grid_offset = Vector::new(0.0, 0.0);
        self.editing = false;
        self.clear_suggestions();
        self.load_draft();
    }

    /// Close the right-click cell context menu.
    pub(crate) fn close_cell_menu(&mut self) {
        self.cell_menu = None;
        self.cell_menu_submenu = None;
    }

    /// The right-click cell menu's items for the active cell: clipboard verbs
    /// and formatting categories (Number Format / Alignment / Text / Fill).
    /// Mirrors the Swift app's per-cell context menu — the replacement for the
    /// always-on format row. Since the rime context menu is a flat panel (no
    /// flyouts), a category drills IN PLACE: picking one replaces the panel with
    /// that category's options plus a "‹ Back" row.
    pub(crate) fn cell_menu_items(&self) -> Vec<MenuItem<Message>> {
        let format = self
            .active_cell()
            .map(|address| self.session.cell_format(address))
            .unwrap_or_default();

        // A category's single-select options, the current one check-marked, led
        // by a Back row that returns to the top level.
        let options = |labels: &[&'static str], selected: usize, msg: fn(usize) -> Message| {
            let mut items = vec![
                MenuItem::action("‹ Back", Message::ExpandCellSubmenu(None)),
                MenuItem::separator(),
            ];
            items.extend(labels.iter().enumerate().map(|(i, label)| {
                let mark = if i == selected { "✓ " } else { "   " };
                MenuItem::action(format!("{mark}{label}"), msg(i))
            }));
            items
        };

        match self.cell_menu_submenu {
            Some(0) => options(
                &NUMBER_FORMAT_LABELS,
                number_format_index(&format.number_format),
                Message::SetNumberFormat,
            ),
            Some(1) => options(
                &ALIGN_LABELS,
                align_index(format.alignment),
                Message::SetAlignment,
            ),
            Some(2) => options(
                &COLOR_LABELS,
                color_index(format.text_color),
                Message::SetTextColor,
            ),
            Some(3) => options(
                &COLOR_LABELS,
                color_index(format.fill_color),
                Message::SetFillColor,
            ),
            _ => vec![
                MenuItem::shortcut("Copy", "⌘C", Message::Copy),
                MenuItem::shortcut("Cut", "⌘X", Message::Cut),
                MenuItem::shortcut("Paste", "⌘V", Message::Paste),
                MenuItem::action("Delete", Message::DeleteSelection),
                MenuItem::separator(),
                MenuItem::shortcut("Number Format", "▸", Message::ExpandCellSubmenu(Some(0))),
                MenuItem::shortcut("Alignment", "▸", Message::ExpandCellSubmenu(Some(1))),
                MenuItem::shortcut("Text Color", "▸", Message::ExpandCellSubmenu(Some(2))),
                MenuItem::shortcut("Fill Color", "▸", Message::ExpandCellSubmenu(Some(3))),
            ],
        }
    }

    /// True when the document has unsaved changes (the live revision has moved
    /// past the one last written).
    pub(crate) fn is_dirty(&self) -> bool {
        self.session.revision() != self.saved_revision
    }

    /// Mutate the active cell's format via `edit` and commit it undoably.
    pub(crate) fn apply_format(&mut self, edit: impl FnOnce(&mut CellFormat)) {
        if let Some(address) = self.active_cell() {
            let mut format = self.session.cell_format(address);
            edit(&mut format);
            self.session.apply_format(address, format);
        }
    }

    /// The selection's inclusive `(r0, r1, c0, c1)` rect, corners normalized.
    pub(crate) fn selection_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        self.grid_selection.map(|selection| selection.bounds())
    }

    /// The active (anchor) cell — where the edit bar reads and writes.
    pub(crate) fn active_cell(&self) -> Option<CellAddress> {
        self.grid_selection.map(|selection| {
            let (row, col) = selection.anchor;
            CellAddress::new(col, row)
        })
    }

    /// Reload the edit bar and name box from the active cell. Also forgets any
    /// point-mode anchor — every fresh edit or navigation flows through here, so
    /// a stale reference-splice can't hijack the next click.
    pub(crate) fn load_draft(&mut self) {
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
    pub(crate) fn refresh_suggestions(&mut self) {
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
    pub(crate) fn suggestion_popup(&self) -> Option<Element<'_, Message>> {
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
    pub(crate) fn clear_suggestions(&mut self) {
        self.suggestions.clear();
        self.suggest_highlight = None;
    }

    /// Move the highlighted suggestion row by `delta` (±1). Down from "none"
    /// lands on the first row; up from the first row returns to "none" (so the
    /// next Enter submits again); it clamps at the last row.
    pub(crate) fn move_highlight(&mut self, delta: i32) {
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
    pub(crate) fn accept_suggestion(&mut self, index: usize) -> Task<Message> {
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
    pub(crate) fn commit_edit(&mut self) {
        if let Some(address) = self.active_cell() {
            self.session.set_cell_raw(address, &self.edit_draft);
        }
        self.editing = false;
        self.load_draft();
    }

    /// A grid click: in point mode (editing an operand-expecting draft) insert
    /// the clicked cell's reference and refocus the bar; otherwise commit any
    /// pending edit, then move the selection and load its content.
    pub(crate) fn select_or_point(
        &mut self,
        row: usize,
        col: usize,
        extend: bool,
    ) -> Task<Message> {
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
    pub(crate) fn move_selection(&mut self, drow: i32, dcol: i32, extend: bool) {
        let current = self
            .grid_selection
            .unwrap_or_else(|| GridSelection::cell(0, 0));
        self.grid_selection = Some(next_selection(current, drow, dcol, extend));
        if !extend {
            self.load_draft();
        }
    }
}
