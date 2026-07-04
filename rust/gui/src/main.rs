//! Soroban — the Rust/iced desktop app (docs/MIGRATION.md Phase 3b).
//!
//! Slice ①–④: a log-view calculator plus an editable spreadsheet grid, with
//! ⌘\ toggling between them. The log and grid share one engine session
//! ([`session::Session`]) — variables defined in the log are visible in cells,
//! and `updateCell(…)` from the log populates the grid. A formula/edit bar
//! commits cell edits (undoable, ⌘Z / ⇧⌘Z), point mode inserts a cell's
//! reference when you click it mid-formula, a control strip drives the
//! selected cell's slider / stepper / checkbox / dropdown, a format bar sets
//! its number format, alignment, and colors, and a name box names its location
//! (`'Rate'`). This file is the iced shell (state → message → update → view)
//! and the rime-styled rendering; later slices add the binary editor and
//! workbook save/open.

mod session;

use iced::widget::{column, container, operation, row, scrollable, text, Id};
use iced::{
    event, keyboard, Color, Element, Event, Font, Length, Subscription, Task, Theme, Vector,
};
use rime::theme::{self, ThemeChoice};
use rime::widgets::{
    button, card, grid, header_row, section, select, slider, stepper, text_field, toggle,
    CellAlign, GridCell, GridSelection,
};
use session::{Outcome, Session, GRID_COLS, GRID_ROWS};
use soroban_engine::{
    CellAddress, CellAlignment, CellDisplay, CellFormat, NumberFormat, PaletteColor, Value,
};

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
    /// The name box's contents — the selected cell's name, if any.
    name_draft: String,
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
    SetNumberFormat(usize),
    SetAlignment(usize),
    SetTextColor(usize),
    SetFillColor(usize),
    NameChanged(String),
    NameCommitted,
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
        }
        Task::none()
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

        // When the active cell is a control, an interactive strip drives it.
        let mut header = column![edit_bar].spacing(12);
        if let Some(bar) = self.format_bar() {
            header = header.push(bar);
        }
        if let Some(strip) = self.control_strip() {
            header = header.push(strip);
        }

        let palette = *palette;
        let session = &self.session;
        let sheet = grid(GRID_ROWS, GRID_COLS, move |row, col| {
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
        CellDisplay::Slider(info) | CellDisplay::Stepper(info) => {
            GridCell::right(format.number_format.rendered(&info.value))
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
        .window_size(iced::Size::new(820.0, 620.0))
        .run()
}
