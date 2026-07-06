//! Pure rendering helpers for the shell: turning engine values into grid cells
//! and log rows, the format-bar presets, the palette-color mapping, and the
//! per-cell control widgets. Kept free of `App` so they stay easy to test and
//! reuse across the view builders.

use iced::widget::{column, mouse_area, text};
use iced::{Color, Element, Font};
use rime::theme;
use rime::widgets::{select, slider, stepper, toggle, CellAlign, GridCell, GridSelection};
use soroban_engine::{
    CellAddress, CellAlignment, CellDisplay, CellFormat, NumberFormat, PaletteColor,
};
use soroban_gui::session::{Origin, Outcome, GRID_COLS, GRID_ROWS};

use crate::Message;

/// The engine tags sheet-scoped definitions with math-alphanumeric markers
/// (`𝑫` data, `𝑖` variable) that neither the text nor the icon font renders —
/// they'd show as tofu. Swap them for plain letters for display (`λ` for
/// functions renders fine, so it's left alone). Display-only; the engine's
/// canonical marker is untouched.
fn renderable_definition(marker: String) -> String {
    marker.replace('𝑫', "D").replace('𝑖', "i")
}

/// Render a cell for the grid: the display drives the base text/alignment, then
/// the cell's format overrides the number rendering, alignment, and colors on
/// top.
pub(crate) fn render_cell(
    display: CellDisplay,
    format: &CellFormat,
    palette: &theme::Palette,
) -> GridCell {
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
pub(crate) const NUMBER_FORMAT_LABELS: [&str; 7] = [
    "General", "Number", "Currency", "Percent", "Date", "Hex", "Binary",
];

pub(crate) fn number_format_at(index: usize) -> NumberFormat {
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

pub(crate) fn number_format_index(format: &NumberFormat) -> usize {
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

pub(crate) const ALIGN_LABELS: [&str; 4] = ["Auto", "Left", "Center", "Right"];

pub(crate) fn align_index(alignment: CellAlignment) -> usize {
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

pub(crate) const COLOR_LABELS: [&str; 8] = [
    "None", "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray",
];

pub(crate) fn color_choice(index: usize) -> Option<PaletteColor> {
    index
        .checked_sub(1)
        .and_then(|i| PaletteColor::ALL.get(i).copied())
}

pub(crate) fn color_index(color: Option<PaletteColor>) -> usize {
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
pub(crate) fn next_selection(
    current: GridSelection,
    drow: i32,
    dcol: i32,
    extend: bool,
) -> GridSelection {
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
pub(crate) fn width_chip_style(
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
pub(crate) fn control_widget<'a>(
    address: CellAddress,
    display: CellDisplay,
) -> Option<Element<'a, Message>> {
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
pub(crate) fn origin_tag<'a>(
    origin: Origin,
    palette: &theme::Palette,
    font: Font,
) -> Element<'a, Message> {
    match origin {
        Origin::Log => text("log").size(11).color(palette.muted).into(),
        Origin::Cell(address) => mouse_area(
            text(format!("{address} ↗"))
                .font(font)
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
pub(crate) fn entry_view<'a>(
    input: &str,
    outcome: &Outcome,
    palette: &theme::Palette,
    size: f32,
    font: Font,
) -> Element<'a, Message> {
    // Secondary lines (comments, info, error text) read one point smaller.
    let small = (size - 1.0).max(1.0);
    // Echoed input in accent, no prefix — matching the original, where the
    // expression is the colored line and the result below it is plain ink.
    let echo = text(input.to_string())
        .font(font)
        .size(size)
        .color(palette.accent);

    let result: Element<'a, Message> = match outcome {
        Outcome::Value(value) => text(format!("= {value}"))
            .font(font)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Function(signature) => text(format!("λ {signature}"))
            .font(font)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Data(declaration) => text(format!("D {declaration}"))
            .font(font)
            .size(size)
            .color(palette.ink)
            .into(),
        Outcome::Comment(note) => text(format!("# {note}"))
            .font(font)
            .size(small)
            .color(palette.muted)
            .into(),
        Outcome::Info(block) => text(block.clone())
            .font(font)
            .size(small)
            .color(palette.ink)
            .into(),
        Outcome::Error { message, position } => {
            let mut lines = column![].spacing(2);
            if let Some(position) = position {
                // No echo prefix now, so the caret aligns directly under column.
                let caret = format!("{}^", " ".repeat(*position));
                lines = lines.push(text(caret).font(font).size(size).color(palette.danger));
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
