//! Soroban — the Rust/iced desktop app (docs/MIGRATION.md Phase 3b).
//!
//! Slice ①–④: a log-view calculator plus an editable spreadsheet grid, with
//! ⌘\ toggling between them. The log and grid share one engine session
//! ([`session::Session`]) — variables defined in the log are visible in cells,
//! and `updateCell(…)` from the log populates the grid. A formula/edit bar
//! commits cell edits (undoable, ⌘Z / ⇧⌘Z), point mode inserts a cell's
//! reference when you click it mid-formula, and a control strip drives the
//! selected cell's slider / stepper / checkbox / dropdown. This file is the
//! iced shell (state → message → update → view) and the rime-styled rendering;
//! later slices add cell formats, named cells, the binary editor, and workbook
//! save/open.

mod session;

use iced::widget::{column, container, operation, row, scrollable, text, Id};
use iced::{event, keyboard, Element, Event, Font, Length, Subscription, Task, Theme, Vector};
use rime::theme::{self, ThemeChoice};
use rime::widgets::{
    button, card, grid, header_row, section, select, slider, stepper, text_field, toggle,
    CellAlign, GridCell, GridSelection,
};
use session::{Outcome, Session, GRID_COLS, GRID_ROWS};
use soroban_engine::{CellAddress, CellDisplay, Value};

const MONO: Font = Font::MONOSPACE;

/// The edit bar's widget id, used to refocus it after a point-mode reference
/// insertion (a grid click steals focus, so we grab it back).
fn edit_bar_id() -> Id {
    Id::new("soroban-edit-bar")
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
    /// True while the edit bar holds uncommitted typing — the point-mode gate:
    /// a grid click on an operand-expecting draft inserts a reference instead
    /// of moving the selection.
    editing: bool,
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
    Undo,
    Redo,
    SliderChanged(f32),
    StepperStepped(bool),
    CheckboxToggled,
    DropdownPicked(usize),
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(text) => self.session.set_input(text),
            Message::Submit => self.session.submit(),
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
            // Control interactions rewrite the cell's storage literal; reload
            // the edit bar so it shows the new value.
            Message::SliderChanged(value) => {
                if let Some(address) = self.active_cell() {
                    self.session.set_slider(address, value as f64);
                    self.load_draft();
                }
            }
            Message::StepperStepped(up) => {
                if let Some(address) = self.active_cell() {
                    self.session.step_control(address, up);
                    self.load_draft();
                }
            }
            Message::CheckboxToggled => {
                if let Some(address) = self.active_cell() {
                    self.session.toggle_checkbox(address);
                    self.load_draft();
                }
            }
            Message::DropdownPicked(index) => {
                if let Some(address) = self.active_cell() {
                    self.session.set_dropdown_index(address, index);
                    self.load_draft();
                }
            }
        }
        Task::none()
    }

    /// The active (anchor) cell — where the edit bar reads and writes.
    fn active_cell(&self) -> Option<CellAddress> {
        self.grid_selection.map(|selection| {
            let (row, col) = selection.anchor;
            CellAddress::new(col, row)
        })
    }

    /// Reload the edit bar from the active cell's raw content.
    fn load_draft(&mut self) {
        self.edit_draft = self
            .active_cell()
            .map(|address| self.session.cell_raw(address))
            .unwrap_or_default();
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
            return operation::focus(edit_bar_id());
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
                _ => None,
            },
            _ => None,
        });
        keys
    }

    fn view(&self) -> Element<'_, Message> {
        let _scope = theme::enter(self.choice.palette());
        let palette = theme::tokens();

        let theme_label = if matches!(self.choice, ThemeChoice::Dark) {
            "☀"
        } else {
            "☾"
        };
        let toggle_label = match self.mode {
            ViewMode::Log => "Grid  ⌘\\",
            ViewMode::Grid => "Log  ⌘\\",
        };
        let top_bar = row![
            container(header_row(
                "Soroban",
                "Anzan — exact calculation (50 significant digits)"
            ))
            .width(Length::Fill),
            button::secondary(toggle_label, Message::ToggleView),
            button::ghost(theme_label, Message::ToggleTheme),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let body = match self.mode {
            ViewMode::Log => self.log_view(&palette),
            ViewMode::Grid => self.grid_view(&palette),
        };

        container(card(column![top_bar, body].spacing(16)))
            .padding(20)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn log_view(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let input_bar = row![
            text_field(
                "Type an expression — try 0.1 + 0.2",
                self.session.input(),
                Message::InputChanged
            )
            .on_submit(Message::Submit)
            .font(MONO),
            button::primary("=", Message::Submit),
        ]
        .spacing(8);

        // The log, newest first so the latest result sits right under the input.
        let log: Element<'_, Message> = if self.session.entries().is_empty() {
            container(
                text("Results appear here. ↑/↓ recall what you typed; ⌘\\ shows the grid.")
                    .size(13)
                    .color(palette.muted),
            )
            .padding(12)
            .into()
        } else {
            let mut items = column![].spacing(12);
            for entry in self.session.entries().iter().rev() {
                items = items.push(entry_view(&entry.input, &entry.outcome, palette));
            }
            scrollable(items.padding([4, 8]))
                .height(Length::Fill)
                .into()
        };

        column![input_bar, section("Log"), log].spacing(16).into()
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
                .width(Length::Fixed(56.0))
                .center_y(Length::Shrink),
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

        // When the active cell is a control, an interactive strip drives it.
        let mut header = column![edit_bar].spacing(12);
        if let Some(strip) = self.control_strip() {
            header = header.push(strip);
        }

        let palette = *palette;
        let session = &self.session;
        let sheet = grid(GRID_ROWS, GRID_COLS, move |row, col| {
            map_cell(session.cell_display(row, col), &palette)
        })
        .offset(self.grid_offset)
        .selection(self.grid_selection)
        .on_scroll(Message::GridScrolled)
        .on_select(Message::GridSelected);

        column![
            header,
            section(&format!("Grid — {}", self.session.active_sheet_name())),
            container(sheet).height(Length::Fill),
        ]
        .spacing(16)
        .into()
    }

    /// If the active cell is a control (slider / stepper / checkbox / dropdown),
    /// render the interactive widget that drives its stored literal.
    fn control_strip(&self) -> Option<Element<'_, Message>> {
        let address = self.active_cell()?;
        // A control's own 𝑖 name reads better than the raw address when set.
        let label = |name: &Option<String>| name.clone().unwrap_or_else(|| address.to_string());
        match self.session.display_at(address) {
            CellDisplay::Slider(info) => {
                let range = (info.minimum.to_f64() as f32)..=(info.maximum.to_f64() as f32);
                let value = info.value.to_f64() as f32;
                Some(slider(
                    label(&info.name),
                    range,
                    value,
                    info.value.to_string(),
                    Message::SliderChanged,
                ))
            }
            CellDisplay::Stepper(info) => Some(stepper(
                &label(&info.name),
                info.value.to_string(),
                Message::StepperStepped(false),
                Message::StepperStepped(true),
            )),
            CellDisplay::Checkbox(info) => Some(toggle(
                &label(&info.name),
                info.is_on,
                Message::CheckboxToggled,
            )),
            CellDisplay::Dropdown(info) => {
                let options: Vec<String> = info
                    .options
                    .iter()
                    .map(|value| value.display_description())
                    .collect();
                let selected = info.value.display_description();
                let lookup = options.clone();
                let picker = select(options, Some(selected), move |chosen: String| {
                    let index = lookup
                        .iter()
                        .position(|option| *option == chosen)
                        .unwrap_or(0);
                    Message::DropdownPicked(index)
                });
                Some(
                    row![text(label(&info.name)).size(13), picker]
                        .spacing(10)
                        .align_y(iced::Alignment::Center)
                        .into(),
                )
            }
            _ => None,
        }
    }
}

/// Map a computed cell to a rime grid cell: numbers right-align, labels
/// left-align, errors show `#ERR` in the danger color, definitions/notes get
/// their glyph text.
fn map_cell(display: CellDisplay, palette: &theme::Palette) -> GridCell {
    match display {
        CellDisplay::Empty => GridCell::default(),
        CellDisplay::Text(label) => GridCell::new(label),
        CellDisplay::Value(number) => GridCell::right(number.to_string()),
        CellDisplay::Error(_) => GridCell::new("#ERR")
            .align(CellAlign::Center)
            .text_color(palette.danger),
        CellDisplay::Definition(glyph) => GridCell::new(glyph).text_color(palette.accent),
        CellDisplay::Note(note) => GridCell::new(note).text_color(palette.muted),
        CellDisplay::Slider(info) | CellDisplay::Stepper(info) => {
            GridCell::right(info.value.to_string())
        }
        CellDisplay::Checkbox(info) => {
            GridCell::new(if info.is_on { "true" } else { "false" }).align(CellAlign::Center)
        }
        // The dropdown's value IS the cell's value: a string shows as a label,
        // a number right-aligns like any figure.
        CellDisplay::Dropdown(info) => match info.value {
            Value::String(text) => GridCell::new(text),
            other => GridCell::right(other.to_string()),
        },
    }
}

/// One log entry: the echoed input, then its outcome (a value, a definition, a
/// note, a raw block, or an error with an aligned caret).
fn entry_view<'a>(
    input: &str,
    outcome: &Outcome,
    palette: &theme::Palette,
) -> Element<'a, Message> {
    // Echoed input, monospace so an error caret lines up beneath it.
    let echo = text(format!("› {input}"))
        .font(MONO)
        .size(13)
        .color(palette.muted);

    let result: Element<'a, Message> = match outcome {
        Outcome::Value(value) => text(format!("= {value}"))
            .font(MONO)
            .size(14)
            .color(palette.accent)
            .into(),
        Outcome::Function(signature) => text(format!("λ {signature}"))
            .font(MONO)
            .size(13)
            .color(palette.ink)
            .into(),
        Outcome::Data(declaration) => text(format!("𝑫 {declaration}"))
            .font(MONO)
            .size(13)
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
                // The echo prefix "› " is two columns wide; offset the caret.
                let caret = format!("{}^", " ".repeat(2 + position));
                lines = lines.push(text(caret).font(MONO).size(13).color(palette.danger));
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

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Soroban")
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size(iced::Size::new(760.0, 620.0))
        .run()
}
