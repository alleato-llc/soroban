//! Review-screenshot harness — a permanent, env-gated dev affordance.
//!
//! iced can capture its own window via wgpu readback (`window::screenshot`),
//! which sidesteps the macOS screen-recording TCC prompt and works headlessly.
//! This module wires that up so a slice can be reviewed as a PNG without a
//! display. It is **inert** unless `SOROBAN_SHOT` is set: [`configure`] returns
//! early and [`App::shot`](crate::App) stays `None`, so nothing subscribes and
//! nothing renders differently.
//!
//! Everything is parameterized by environment variables — no code edits per shot:
//!
//! - `SOROBAN_SHOT=<path>` — enable; capture the window to `<path>` (a `.png`).
//! - `SOROBAN_SHOT_SEED=<file>` — run each non-empty line of `<file>` through the
//!   log first (expressions populate the log; `updateCell(…)` lines populate the
//!   grid), so the screenshot shows real evaluated state.
//! - `SOROBAN_SHOT_VIEW=grid` — start in the grid view (default: the log).
//! - `SOROBAN_SHOT_SELECT=B4` — select that cell (shows the edit bar / a control
//!   strip / a control's own value).
//! - `SOROBAN_SHOT_MENU=file|edit|view` — open that top menu's dropdown.
//! - `SOROBAN_SHOT_EDIT=1` — open the inline editor on the selected cell.
//! - `SOROBAN_SHOT_TYPE=<text>` — seed live input (log bar / formula bar) and
//!   recompute completions, to capture the autocomplete popup.
//! - `SOROBAN_SHOT_SETTINGS=appearance|calculator` — open the Settings window
//!   on that section.
//! - `SOROBAN_SHOT_THEME=<name>` — apply a named theme (e.g. "One Light").
//! - `SOROBAN_SHOT_FONT=<name>` — apply a bundled font (e.g. "JetBrains Mono").
//! - `SOROBAN_SHOT_PANEL=inspector|reference|bits` — open a side/bottom panel.
//!
//! Capture waits three painted frames (so fonts/layout settle) then requests the
//! screenshot and exits.

use iced::{window, Task};

use crate::{App, GridSelection, Message, ViewMode};

/// The capture state, held by [`App`] only while shot mode is active.
pub struct Shot {
    path: String,
    window: Option<window::Id>,
    frames: u32,
    saved: bool,
}

/// A shot-harness lifecycle event, nested under [`Message::Shot`].
#[derive(Debug, Clone)]
pub enum Event {
    /// The window opened; remember its id so we can screenshot it.
    WindowOpened(window::Id),
    /// A frame painted; capture once a few have settled.
    Frame,
    /// The screenshot arrived; write it and exit.
    Captured(window::Screenshot),
}

/// Read `SOROBAN_SHOT*` and, when enabled, seed the app and arm the capture.
/// A no-op (leaving `app.shot == None`) when `SOROBAN_SHOT` is unset.
pub fn configure(app: &mut App) {
    let Ok(path) = std::env::var("SOROBAN_SHOT") else {
        return;
    };

    if let Ok(seed) = std::env::var("SOROBAN_SHOT_SEED") {
        if let Ok(contents) = std::fs::read_to_string(&seed) {
            for line in contents.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    app.session.set_input(line.to_string());
                    app.session.submit();
                }
            }
        }
    }

    if std::env::var("SOROBAN_SHOT_VIEW").as_deref() == Ok("grid") {
        app.mode = ViewMode::Grid;
    }
    if let Ok(cell) = std::env::var("SOROBAN_SHOT_SELECT") {
        if let Some((row, col)) = parse_cell(&cell) {
            app.grid_selection = Some(GridSelection::cell(row, col));
        }
    }
    app.load_draft();
    if std::env::var("SOROBAN_SHOT_EDIT").is_ok() {
        app.editing = true;
    }
    // Seed live (uncommitted) input and recompute completions, to shoot the
    // autocomplete popup — the log bar in log view, the formula bar in grid.
    if let Ok(text) = std::env::var("SOROBAN_SHOT_TYPE") {
        match app.mode {
            ViewMode::Log => app.session.set_input(text),
            ViewMode::Grid => {
                app.edit_draft = text;
                app.editing = true;
            }
        }
        app.refresh_suggestions();
    }
    match std::env::var("SOROBAN_SHOT_MENU").as_deref() {
        Ok("file") => app.menu_open = Some(0),
        Ok("edit") => app.menu_open = Some(1),
        Ok("sheet") => app.menu_open = Some(2),
        Ok("view") => app.menu_open = Some(3),
        _ => {}
    }
    // Open the right-click cell context menu (formatting + clipboard verbs) at a
    // fixed anchor, for shooting it over the grid.
    if let Ok(which) = std::env::var("SOROBAN_SHOT_CELLMENU") {
        app.mode = ViewMode::Grid;
        if app.grid_selection.is_none() {
            app.grid_selection = Some(GridSelection::cell(3, 1));
        }
        app.load_draft();
        app.cell_menu = Some(iced::Point::new(260.0, 300.0));
        // Optionally fly out a submenu by name (number/alignment/text/fill).
        app.cell_menu_submenu = match which.as_str() {
            "number" => Some(0),
            "alignment" => Some(1),
            "text" => Some(2),
            "fill" => Some(3),
            _ => None,
        };
    }
    match std::env::var("SOROBAN_SHOT_PANEL").as_deref() {
        Ok("inspector") => app.inspector_visible = true,
        Ok("reference") => app.reference_visible = true,
        Ok("bits") => app.binary_visible = true,
        _ => {}
    }
    // Apply a named theme by its catalog name (e.g. "One Light").
    if let Ok(name) = std::env::var("SOROBAN_SHOT_THEME") {
        app.theme_name = name;
    }
    // Apply a bundled font family by name (e.g. "JetBrains Mono").
    if let Ok(name) = std::env::var("SOROBAN_SHOT_FONT") {
        app.font_name = name;
    }
    // Open the Settings window on a section (`appearance` / `calculator`).
    match std::env::var("SOROBAN_SHOT_SETTINGS").as_deref() {
        Ok("appearance") => {
            app.settings_open = true;
            app.settings_section = 0;
        }
        Ok("calculator") => {
            app.settings_open = true;
            app.settings_section = 1;
        }
        _ => {}
    }
    app.session.refresh_binary();
    // Optionally set the bit-editor width and apply a named format, to shoot the
    // decoded field bands (e.g. `SOROBAN_SHOT_FORMAT="Unix permissions"`).
    if let Ok(width) = std::env::var("SOROBAN_SHOT_WIDTH") {
        if let Ok(width) = width.parse() {
            app.session.set_binary_width(width);
        }
    }
    if let Ok(name) = std::env::var("SOROBAN_SHOT_FORMAT") {
        app.session.apply_binary_format(Some(&name));
    }
    // Optionally open the visual format builder (`new` empty, `edit` seeded).
    match std::env::var("SOROBAN_SHOT_BUILD").as_deref() {
        Ok("new") => app.session.begin_format_build(false),
        Ok("edit") => app.session.begin_format_build(true),
        _ => {}
    }

    app.shot = Some(Shot {
        path,
        window: None,
        frames: 0,
        saved: false,
    });
}

/// Drive the capture forward from a [`Message::Shot`] event.
pub fn handle(app: &mut App, event: Event) -> Task<Message> {
    let Some(shot) = &mut app.shot else {
        return Task::none();
    };
    match event {
        Event::WindowOpened(id) => {
            shot.window = Some(id);
            Task::none()
        }
        Event::Frame => {
            if !shot.saved {
                if let Some(id) = shot.window {
                    shot.frames += 1;
                    if shot.frames >= 3 {
                        return window::screenshot(id).map(|s| Message::Shot(Event::Captured(s)));
                    }
                }
            }
            Task::none()
        }
        Event::Captured(screenshot) => {
            shot.saved = true;
            save_png(&shot.path, &screenshot);
            iced::exit()
        }
    }
}

/// The subscriptions that drive capture — only while shot mode is active.
pub fn subscription(app: &App) -> Option<iced::Subscription<Message>> {
    app.shot.as_ref()?;
    Some(iced::Subscription::batch([
        window::open_events().map(|id| Message::Shot(Event::WindowOpened(id))),
        window::frames().map(|_| Message::Shot(Event::Frame)),
    ]))
}

/// Encode the captured RGBA window into a PNG at `path`.
fn save_png(path: &str, screenshot: &window::Screenshot) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        screenshot.size.width,
        screenshot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer
        .write_image_data(&screenshot.rgba)
        .expect("png image data");
}

/// Parse an `A1`-style cell into 0-based `(row, col)` for [`GridSelection`].
/// Returns `None` on any malformed input (letters then digits, both non-empty).
fn parse_cell(text: &str) -> Option<(usize, usize)> {
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() {
        return None;
    }
    let row: usize = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    let mut col = 0usize;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some((row - 1, col - 1))
}
