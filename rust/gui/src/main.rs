//! Soroban — the Rust/iced desktop app (docs/MIGRATION.md Phase 3b).
//!
//! Slice ①–④: a log-view calculator plus an editable spreadsheet grid, with
//! ⌘\ toggling between them. The log and grid share one engine session
//! ([`session::Session`]) — variables defined in the log are visible in cells,
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

mod session;
mod shot;

use iced::widget::{column, container, mouse_area, operation, row, scrollable, text, Id};
use iced::{
    event, keyboard, mouse, Color, Element, Event, Font, Length, Subscription, Task, Theme, Vector,
};
use rime::theme::{self, ThemeChoice};
use rime::widgets::{
    bit_grid, button, card, grid, section, select, slider, stepper, text_field, toggle,
    CellAlign, GridCell, GridSelection,
};
use session::{BinaryStatus, Origin, Outcome, Session, GRID_COLS, GRID_ROWS};
use soroban_engine::{
    CellAddress, CellAlignment, CellDisplay, CellFormat, NumberFormat, PaletteColor,
};
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

#[derive(Default)]
struct App {
    session: Session,
    choice: ThemeChoice,
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
    /// The saved file, if any, and the revision at which it was last saved
    /// (compared against the session's live revision for the dirty indicator).
    file_path: Option<PathBuf>,
    saved_revision: u64,
    /// The top action strip auto-hides (like fed's chrome): revealed only while
    /// the pointer is at the top edge. See [`Self::chrome_visible`].
    chrome_revealed: bool,
    /// The review-screenshot harness, present only when `SOROBAN_SHOT` is set —
    /// otherwise `None` and the whole thing is inert. See [`shot`].
    shot: Option<shot::Shot>,
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Submit,
    HistoryPrevious,
    HistoryNext,
    ToggleTheme,
    ToggleView,
    GridScrolled(Vector),
    GridSelected(usize, usize, bool),
    EditChanged(String),
    EditCommitted,
    EditCanceled,
    EditActivated(usize, usize),
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
    NewWorkbook,
    OpenWorkbook,
    SaveWorkbook,
    PointerMoved(f32),
    /// Jump to a cell from an inspector provenance tag (select it, show the grid).
    JumpTo(CellAddress),
    /// Insert a sample expression from the empty-state into the log input.
    SampleClicked(String),
    /// Review-screenshot harness lifecycle (see [`shot`]); inert unless armed.
    Shot(shot::Event),
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(text) => self.session.set_input(text),
            Message::Submit => {
                self.session.submit();
                // The bit editor tracks the newest result until you flip a bit.
                if self.binary_visible {
                    self.session.refresh_binary();
                }
            }
            // ↑/↓ recall history only in the log; the grid owns its own keys.
            Message::HistoryPrevious if self.mode == ViewMode::Log => {
                self.session.recall_previous()
            }
            Message::HistoryNext if self.mode == ViewMode::Log => self.session.recall_next(),
            Message::HistoryPrevious | Message::HistoryNext => {}
            Message::ToggleTheme => self.choice = self.choice.toggled(),
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
            Message::EditChanged(text) => {
                self.edit_draft = text;
                self.editing = true;
            }
            Message::EditCommitted => self.commit_edit(),
            Message::EditCanceled => {
                self.editing = false;
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
            Message::Undo => {
                self.session.undo();
                self.editing = false;
                self.load_draft();
            }
            Message::Redo => {
                self.session.redo();
                self.editing = false;
                self.load_draft();
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
                }
            }
            Message::BitToggled(index) => self.session.flip_binary_bit(index),
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
            // Reveal the action strip at the top edge; a wider keep-band than the
            // trigger (hysteresis) stops it flickering as the pointer hovers it.
            Message::PointerMoved(y) => {
                let threshold = if self.chrome_revealed { 56.0 } else { 6.0 };
                self.chrome_revealed = y < threshold;
            }
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

    /// The active (anchor) cell — where the edit bar reads and writes.
    fn active_cell(&self) -> Option<CellAddress> {
        self.grid_selection.map(|selection| {
            let (row, col) = selection.anchor;
            CellAddress::new(col, row)
        })
    }

    /// Reload the edit bar and name box from the active cell.
    fn load_draft(&mut self) {
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
        let point_mode = self.mode == ViewMode::Grid
            && self.editing
            && self.session.expects_operand(&self.edit_draft);
        if point_mode {
            self.edit_draft
                .push_str(&CellAddress::new(col, row).to_string());
            // Focus the inline editor (the in-grid editor is the active one).
            return operation::focus(grid_editor_id());
        }
        if self.editing {
            // Navigating away commits the in-progress edit (Excel behavior).
            self.commit_edit();
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

    fn theme(&self) -> Theme {
        self.choice.theme()
    }

    /// ↑/↓ recall input history; ⌘\ toggles the view; ⌘Z / ⇧⌘Z undo & redo;
    /// Escape cancels an in-progress cell edit.
    fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                ..
            }) => Some(Message::HistoryPrevious),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                ..
            }) => Some(Message::HistoryNext),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => Some(Message::EditCanceled),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(character),
                modifiers,
                ..
            }) if modifiers.command() => match character.as_str() {
                "\\" => Some(Message::ToggleView),
                "z" | "Z" if modifiers.shift() => Some(Message::Redo),
                "z" | "Z" => Some(Message::Undo),
                "n" | "N" => Some(Message::NewWorkbook),
                "o" | "O" => Some(Message::OpenWorkbook),
                "s" | "S" => Some(Message::SaveWorkbook),
                _ => None,
            },
            // Track the pointer's y so the top action strip can auto-hide.
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
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

    /// Whether the top action strip is currently shown — it auto-hides and is
    /// revealed by the pointer at the top edge (fed's chrome pattern).
    fn chrome_visible(&self) -> bool {
        self.chrome_revealed
    }

    /// A slim, LEFT-aligned strip of ghost actions (the macOS menu bar sits at
    /// the top-left, so these do too). The original keeps these in the window
    /// menu bar; iced has none, so this is the honest home — and it auto-hides.
    fn action_bar(&self) -> Element<'_, Message> {
        let theme_label = if matches!(self.choice, ThemeChoice::Dark) {
            "☀"
        } else {
            "☾"
        };
        let toggle_label = match self.mode {
            ViewMode::Log => "Grid",
            ViewMode::Grid => "Log",
        };
        row![
            button::secondary(toggle_label, Message::ToggleView),
            button::ghost("New", Message::NewWorkbook),
            button::ghost("Open", Message::OpenWorkbook),
            button::ghost("Save", Message::SaveWorkbook),
            button::ghost("Bits", Message::ToggleBinary),
            button::ghost("Names", Message::ToggleInspector),
            button::ghost("Reference", Message::ToggleReference),
            button::ghost(theme_label, Message::ToggleTheme),
            container(text("").size(1)).width(Length::Fill),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        let _scope = theme::enter(self.choice.palette());
        let palette = theme::tokens();

        let body = match self.mode {
            ViewMode::Log => self.log_view(&palette),
            ViewMode::Grid => self.grid_view(&palette),
        };

        // Edge-to-edge, no card. The action strip auto-hides — shown only when
        // revealed at the top edge — so the view fills the whole window; the
        // log's own input bar sits at the bottom (REPL layout).
        let mut stack = column![].spacing(10);
        if self.chrome_visible() {
            stack = stack.push(self.action_bar());
        }
        stack = stack.push(body);
        let main = container(stack)
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
        if self.binary_visible {
            column![
                container(horizontal).height(Length::Fill),
                self.binary_panel(&palette)
            ]
            .into()
        } else {
            horizontal
        }
    }

    /// The binary bit-editor strip: a clickable bit grid for the last result,
    /// its value, and a Use button that drops the value into the input.
    fn binary_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let content: Element<'_, Message> = match self.session.binary_status() {
            BinaryStatus::Editable {
                bits,
                value,
                width,
                signed,
            } => {
                let caption = format!(
                    "{value}   ·   {width}-bit {}",
                    if signed { "signed" } else { "unsigned" }
                );
                column![
                    row![
                        text(caption).font(MONO).size(13).color(palette.accent),
                        container(button::secondary("Use in input", Message::UseBinary))
                            .width(Length::Fill)
                            .align_x(iced::alignment::Horizontal::Right),
                    ]
                    .align_y(iced::Alignment::Center),
                    scrollable(bit_grid(bits, Vec::new(), Message::BitToggled)),
                ]
                .spacing(12)
                .into()
            }
            BinaryStatus::Unavailable(reason) => text(reason).size(13).color(palette.muted).into(),
        };

        container(card(
            column![text("Binary").size(15).color(palette.ink), content].spacing(12),
        ))
        .padding(iced::Padding {
            top: 0.0,
            right: 20.0,
            bottom: 20.0,
            left: 20.0,
        })
        .into()
    }

    /// The reference window: every function, operator, and constant — the
    /// user's own first — with a live search filter.
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
                text("Reference").size(15).color(palette.ink),
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
        let mut sections = column![text("Environment").size(15).color(palette.ink)].spacing(16);
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
        let log: Element<'_, Message> = if self.session.entries().is_empty() {
            self.empty_log(palette)
        } else {
            let mut items = column![].spacing(12);
            for entry in self.session.entries().iter() {
                items = items.push(entry_view(&entry.input, &entry.outcome, palette));
            }
            scrollable(items.padding([4, 8]))
                .height(Length::Fill)
                .into()
        };

        // The input is pinned to the BOTTOM, behind a `>` prompt; Enter submits
        // (no `=` button — the original has none). The two signature corner
        // icons (docs / grid) sit at the right, always visible like the original.
        let input_bar = row![
            text(">").font(MONO).size(16).color(palette.muted),
            text_field("Expression", self.session.input(), Message::InputChanged)
                .id(log_input_id())
                .on_submit(Message::Submit)
                .font(MONO),
            button::ghost("📖", Message::ToggleReference),
            button::ghost("▦", Message::ToggleView),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        column![log, input_bar].spacing(12).into()
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
        container(column)
            .padding(12)
            .height(Length::Fill)
            .into()
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
            .on_submit(Message::EditCommitted)
            .font(MONO),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Controls now render inline in their cells (below); the header keeps the
        // formula/name bar and the format bar.
        let mut header = column![edit_bar].spacing(12);
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
        .on_scroll(Message::GridScrolled)
        .on_select(Message::GridSelected)
        .on_activate(Message::EditActivated);

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
                    .on_submit(Message::EditCommitted)
                    .padding(2)
                    .size(13)
                    .font(MONO);
                sheet = sheet.editor(row, col, editor);
            }
        }

        // A sheet-tab strip at the bottom-left, like the original's `Mortgage +`.
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
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        column![
            header,
            container(sheet).height(Length::Fill),
            sheet_tab,
        ]
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
        CellDisplay::Definition(glyph) => GridCell::new(glyph).text_color(palette.accent),
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
        CellDisplay::Checkbox(info) => Some(toggle(
            "",
            info.is_on,
            Message::CheckboxToggled(address),
        )),
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
) -> Element<'a, Message> {
    // Echoed input in accent, no prefix — matching the original, where the
    // expression is the colored line and the result below it is plain ink.
    let echo = text(input.to_string())
        .font(MONO)
        .size(14)
        .color(palette.accent);

    let result: Element<'a, Message> = match outcome {
        Outcome::Value(value) => text(format!("= {value}"))
            .font(MONO)
            .size(14)
            .color(palette.ink)
            .into(),
        Outcome::Function(signature) => text(format!("λ {signature}"))
            .font(MONO)
            .size(14)
            .color(palette.ink)
            .into(),
        Outcome::Data(declaration) => text(format!("𝑫 {declaration}"))
            .font(MONO)
            .size(14)
            .color(palette.ink)
            .into(),
        Outcome::Comment(note) => text(format!("# {note}"))
            .font(MONO)
            .size(13)
            .color(palette.muted)
            .into(),
        Outcome::Info(block) => text(block.clone())
            .font(MONO)
            .size(13)
            .color(palette.ink)
            .into(),
        Outcome::Error { message, position } => {
            let mut lines = column![].spacing(2);
            if let Some(position) = position {
                // No echo prefix now, so the caret aligns directly under column.
                let caret = format!("{}^", " ".repeat(*position));
                lines = lines.push(text(caret).font(MONO).size(14).color(palette.danger));
            }
            lines
                .push(
                    text(format!("error: {message}"))
                        .size(13)
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
        let mut app = App::default();
        shot::configure(&mut app);
        app
    }
}

fn main() -> iced::Result {
    iced::application(App::launch, App::update, App::view)
        .title(App::window_title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size(iced::Size::new(1040.0, 680.0))
        .run()
}
