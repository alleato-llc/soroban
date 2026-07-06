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

mod binary_panel;
mod message;
mod panels;
mod render;
mod settings;
mod shot;
mod state;
mod themes;
mod update;
mod view;

pub(crate) use message::Message;

use iced::widget::Id;
use iced::{Font, Point, Vector};
use rime::icons::{self};
use rime::theme;
use rime::widgets::GridSelection;
use soroban_engine::Completion;
use soroban_gui::session::Session;
use std::collections::HashMap;
use std::path::PathBuf;

const MONO: Font = Font::MONOSPACE;

/// Bundled monospace fonts (embedded so they render identically on every
/// platform), loaded once at startup and selectable in Settings.
const JETBRAINS_MONO_BYTES: &[u8] = include_bytes!("assets/fonts/JetBrainsMono-Regular.ttf");
const SOURCE_CODE_PRO_BYTES: &[u8] = include_bytes!("assets/fonts/SourceCodePro-Regular.ttf");
const IBM_PLEX_MONO_BYTES: &[u8] = include_bytes!("assets/fonts/IBMPlexMono-Regular.ttf");
const HACK_BYTES: &[u8] = include_bytes!("assets/fonts/Hack-Regular.ttf");
const FIRA_MONO_BYTES: &[u8] = include_bytes!("assets/fonts/FiraMono-Regular.ttf");

/// The embedded font blobs, registered with iced at startup, and the family
/// names (as spelled in each TTF) used to reference them — kept in the same
/// order. Adding a font is: an `include_bytes!` const, an entry here, and its
/// family name in `BUNDLED_FAMILIES`.
const BUNDLED_FONT_BYTES: [&[u8]; 5] = [
    JETBRAINS_MONO_BYTES,
    SOURCE_CODE_PRO_BYTES,
    IBM_PLEX_MONO_BYTES,
    HACK_BYTES,
    FIRA_MONO_BYTES,
];
const BUNDLED_FAMILIES: [&str; 5] = [
    "JetBrains Mono",
    "Source Code Pro",
    "IBM Plex Mono",
    "Hack",
    "Fira Mono",
];

/// Well-known SYSTEM monospace families offered per-platform. iced's font
/// database loads the OS's installed fonts at startup, so `Font::with_name`
/// resolves these when present and falls back to the default monospace when
/// absent — so we curate a list that can actually exist on each OS rather than
/// offer names that would silently no-op.
#[cfg(target_os = "macos")]
const SYSTEM_FAMILIES: &[&str] = &["Menlo", "SF Mono", "Monaco", "Courier New", "Andale Mono"];
#[cfg(target_os = "windows")]
const SYSTEM_FAMILIES: &[&str] = &[
    "Consolas",
    "Cascadia Mono",
    "Cascadia Code",
    "Courier New",
    "Lucida Console",
];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SYSTEM_FAMILIES: &[&str] = &[
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Ubuntu Mono",
    "Noto Sans Mono",
    "FreeMono",
];

/// The Soroban app icon (the Swift app's 256×256 artwork), embedded so the
/// window carries it in the Linux/Windows taskbar. macOS takes its Dock icon
/// from the packaged `Soroban.app` bundle instead (winit ignores the window
/// icon there) — see `rust/gui/packaging`.
const APP_ICON_BYTES: &[u8] = include_bytes!("assets/icon.png");

/// Decode the embedded PNG into an iced window icon. Returns `None` (no icon,
/// never a crash) if the bytes ever fail to decode — the app still runs.
fn app_icon() -> Option<iced::window::Icon> {
    let mut reader = png::Decoder::new(APP_ICON_BYTES).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    // The bundled asset is 8-bit RGBA; bail on any other shape rather than
    // hand a mis-sized buffer to `from_rgba` (which would just Err anyway).
    if info.bit_depth != png::BitDepth::Eight || info.color_type != png::ColorType::Rgba {
        return None;
    }
    iced::window::icon::from_rgba(buf, info.width, info.height).ok()
}

/// The monospace families offered in Settings: `(display name, resolved Font)`.
/// "System" is iced's built-in default monospace, then the bundled families
/// (referenced by the family name embedded in each TTF), then the curated OS
/// system families. Everything after "System" resolves by name via the font
/// database, so an absent system family just falls back to the default.
fn font_choices() -> Vec<(&'static str, Font)> {
    let mut choices = vec![("System", MONO)];
    for name in BUNDLED_FAMILIES {
        choices.push((name, Font::with_name(name)));
    }
    for &name in SYSTEM_FAMILIES {
        choices.push((name, Font::with_name(name)));
    }
    choices
}

/// Resolve a font family display name to its `Font`, falling back to the system
/// monospace for an unknown / unset name.
fn font_for(name: &str) -> Font {
    font_choices()
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, font)| *font)
        .unwrap_or(MONO)
}

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
    /// The chosen monospace family's display name (see [`font_choices`]); empty
    /// means the system default.
    font_name: String,
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
    /// While renaming a sheet from the tab strip: the in-progress draft. `None`
    /// when no rename is open (the inline `rename_bar` shows iff this is `Some`).
    sheet_rename_draft: Option<String>,
    /// The last known cursor position (window coords), tracked from mouse-move
    /// events — where a right-click anchors the cell context menu.
    cursor: (f32, f32),
    /// The open cell context menu's anchor (window coords), or `None` when
    /// closed. A right-click on a selected grid cell opens it.
    cell_menu: Option<Point>,
    /// Which cell-menu submenu (Number Format / Alignment / …) is currently
    /// flown out, by index; `None` when none is hovered.
    cell_menu_submenu: Option<usize>,
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

impl App {
    /// The initial state: `App::default`, then the screenshot harness gets a
    /// chance to seed it (a no-op unless `SOROBAN_SHOT` is set — see [`shot`]).
    fn launch() -> Self {
        let mut app = App {
            theme_name: themes::default_name().to_string(),
            font_size: 14.0,
            font_name: font_choices()[0].0.to_string(),
            ..App::default()
        };
        shot::configure(&mut app);
        app
    }
}

fn main() -> iced::Result {
    let mut app = iced::application(App::launch, App::update, App::view)
        .title(App::window_title)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(icons::FONT_BYTES); // the embedded icon font (toolbar/toggle/close glyphs)
    for bytes in BUNDLED_FONT_BYTES {
        app = app.font(bytes); // bundled monospace families, selectable in Settings
    }
    app.window(iced::window::Settings {
        size: iced::Size::new(1040.0, 680.0),
        icon: app_icon(), // taskbar icon on Linux/Windows (macOS uses the .app)
        ..Default::default()
    })
    .run()
}

#[cfg(test)]
mod app_tests;
