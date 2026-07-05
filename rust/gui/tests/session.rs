//! Headless BDD suite for the Rust app's session layer — runs
//! `tests/features/session.feature` against the UI-free [`Session`] with no
//! iced and no rendering (the Rust counterpart to the Swift
//! `SorobanSessionTests`, but a fast `cargo test`). It exercises the calculator
//! (the log) and the sheet (the grid) through the same view-model the iced
//! shell drives. Rust-only by design: the cross-ecosystem parity oracle is
//! `spec/anzan`, run by the engine's gherkin suite.

use cucumber::{given, then, when, World};
use soroban_engine::{CellAddress, CellDisplay};
use soroban_gui::session::{Outcome, PointClick, Session};
use std::fmt;

/// A stand-in for the app's open inline cell editor (the App holds this state;
/// here the World does, so the point-mode steps can drive it headlessly).
struct Editor {
    address: CellAddress,
    draft: String,
}

#[derive(World)]
#[world(init = Self::fresh)]
struct SessionWorld {
    session: Session,
    /// A stand-in system clipboard for the copy/paste steps (TSV text).
    clipboard: String,
    /// The open inline editor, if any (point-mode steps).
    editor: Option<Editor>,
}

impl SessionWorld {
    /// One world per scenario — a fresh session, exactly what the app builds at
    /// launch. Disk-safe: nothing persists without an explicit save.
    fn fresh() -> Self {
        Self {
            session: Session::new(),
            clipboard: String::new(),
            editor: None,
        }
    }
}

impl fmt::Debug for SessionWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionWorld({} log entries)", self.session.entries().len())
    }
}

/// Parse an `A:1` cell key (panicking loudly on a bad one — a test typo).
fn address(key: &str) -> soroban_engine::CellAddress {
    soroban_engine::CellAddress::from_key(&key.to_uppercase())
        .unwrap_or_else(|| panic!("'{key}' is not a cell address"))
}

/// A cell's display as the comparable string a user "sees" (mirrors the engine
/// gherkin suite's `render`, so cell assertions read identically across suites).
fn render(display: CellDisplay) -> String {
    match display {
        CellDisplay::Empty => String::new(),
        CellDisplay::Text(text) => text,
        CellDisplay::Value(value) => value.to_string(),
        CellDisplay::Error(message) => format!("#ERR {message}"),
        CellDisplay::Definition(glyph) => glyph,
        CellDisplay::Note(comment) => format!("# {comment}"),
        CellDisplay::Slider(info) | CellDisplay::Stepper(info) => format!("slider:{}", info.value),
        CellDisplay::Checkbox(info) => if info.is_on { "checked" } else { "unchecked" }.to_string(),
        CellDisplay::Dropdown(info) => info.value.display_text(),
    }
}

fn shown(world: &SessionWorld, key: &str) -> String {
    render(world.session.display_at(address(key)))
}

/// The most recent log entry's outcome.
fn last_outcome(world: &SessionWorld) -> &Outcome {
    &world
        .session
        .entries()
        .last()
        .expect("no log entry to inspect")
        .outcome
}

// MARK: Setup

#[given("a fresh session")]
fn a_fresh_session(_world: &mut SessionWorld) {
    // `Self::fresh` already built one per scenario; this reads as intent.
}

// MARK: Calculator (the log)

#[when(regex = r#"^I enter "(.*)"$"#)]
fn i_enter(world: &mut SessionWorld, expression: String) {
    world.session.set_input(expression);
    world.session.submit();
}

#[then(regex = r#"^the result is "(.*)"$"#)]
fn the_result_is(world: &mut SessionWorld, expected: String) {
    match last_outcome(world) {
        Outcome::Value(value) => assert_eq!(
            *value, expected,
            "result is '{value}', expected '{expected}'"
        ),
        other => panic!("expected a value '{expected}', got {other:?}"),
    }
}

#[then(regex = r#"^the log defines a function "(.*)"$"#)]
fn the_log_defines_a_function(world: &mut SessionWorld, signature: String) {
    match last_outcome(world) {
        Outcome::Function(actual) => assert!(
            actual.contains(&signature),
            "defined '{actual}', expected a signature containing '{signature}'"
        ),
        other => panic!("expected a function definition '{signature}', got {other:?}"),
    }
}

#[then(regex = r#"^the last line fails mentioning "(.*)"$"#)]
fn the_last_line_fails(world: &mut SessionWorld, fragment: String) {
    match last_outcome(world) {
        Outcome::Error { message, .. } => assert!(
            message.contains(&fragment),
            "failed with '{message}', expected it to mention '{fragment}'"
        ),
        other => panic!("expected an error mentioning '{fragment}', got {other:?}"),
    }
}

#[then(regex = r#"^the last line is a note "(.*)"$"#)]
fn the_last_line_is_a_note(world: &mut SessionWorld, expected: String) {
    match last_outcome(world) {
        Outcome::Comment(text) => assert_eq!(
            *text, expected,
            "note is '{text}', expected '{expected}'"
        ),
        other => panic!("expected a note '{expected}', got {other:?}"),
    }
}

// MARK: Sheet (the grid)

#[when(regex = r#"^I set cell ([A-Za-z]+:[0-9]+) to "(.*)"$"#)]
fn i_set_cell(world: &mut SessionWorld, key: String, raw: String) {
    world.session.set_cell_raw(address(&key), &raw);
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) shows "(.*)"$"#)]
fn cell_shows(world: &mut SessionWorld, key: String, expected: String) {
    let shown = shown(world, &key);
    assert_eq!(shown, expected, "cell {key} shows '{shown}', expected '{expected}'");
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) shows an error mentioning "(.*)"$"#)]
fn cell_shows_error(world: &mut SessionWorld, key: String, fragment: String) {
    let shown = shown(world, &key);
    assert!(
        shown.starts_with("#ERR") && shown.contains(&fragment),
        "cell {key} shows '{shown}', expected an error mentioning '{fragment}'"
    );
}

#[when(regex = r#"^I name cell ([A-Za-z]+:[0-9]+) "(.*)"$"#)]
fn i_name_cell(world: &mut SessionWorld, key: String, name: String) {
    world
        .session
        .set_cell_name(address(&key), &name)
        .unwrap_or_else(|error| panic!("naming {key} failed: {error}"));
}

// MARK: Point mode (Excel-style reference insertion while editing)

#[when(regex = r#"^I begin editing cell ([A-Za-z]+:[0-9]+)$"#)]
fn i_begin_editing(world: &mut SessionWorld, key: String) {
    let address = address(&key);
    let draft = world.session.cell_raw(address);
    world.editor = Some(Editor { address, draft });
}

#[when(regex = r#"^I type "(.*)" into the editor$"#)]
fn i_type_into_editor(world: &mut SessionWorld, text: String) {
    world.editor.as_mut().expect("no editor is open").draft = text;
}

#[when(regex = r#"^I click cell ([A-Za-z]+:[0-9]+)$"#)]
fn i_click_cell(world: &mut SessionWorld, key: String) {
    let editor = world.editor.take().expect("no editor is open to click from");
    // Exactly what the app's `select_or_point` does with the click.
    match world.session.point_click(&editor.draft, address(&key)) {
        PointClick::Inserted(draft) => {
            world.editor = Some(Editor {
                address: editor.address,
                draft,
            });
        }
        PointClick::Commit => {
            world.session.set_cell_raw(editor.address, &editor.draft);
            // The editor is now closed (committed) — `world.editor` stays None.
        }
    }
}

#[then(regex = r#"^the editor holds "(.*)"$"#)]
fn the_editor_holds(world: &mut SessionWorld, expected: String) {
    let draft = &world.editor.as_ref().expect("the editor has closed").draft;
    assert_eq!(*draft, expected, "editor holds '{draft}', expected '{expected}'");
}

#[then("the editor is closed")]
fn the_editor_is_closed(world: &mut SessionWorld) {
    assert!(
        world.editor.is_none(),
        "expected the click to commit and close the editor, but it is still open"
    );
}

// MARK: Undo / redo

#[when("I undo")]
fn i_undo(world: &mut SessionWorld) {
    world.session.undo();
}

#[when("I redo")]
fn i_redo(world: &mut SessionWorld) {
    world.session.redo();
}

// MARK: Controls

#[when(regex = r#"^I toggle the checkbox in ([A-Za-z]+:[0-9]+)$"#)]
fn i_toggle_checkbox(world: &mut SessionWorld, key: String) {
    world.session.toggle_checkbox(address(&key));
}

#[when(regex = r#"^I set the slider in ([A-Za-z]+:[0-9]+) to "([0-9.]+)"$"#)]
fn i_set_slider(world: &mut SessionWorld, key: String, target: String) {
    let target: f64 = target.parse().expect("slider target must be a number");
    world.session.set_slider(address(&key), target);
}

// MARK: Copy / paste

#[when(regex = r#"^I copy ([A-Za-z]+:[0-9]+) through ([A-Za-z]+:[0-9]+)$"#)]
fn i_copy(world: &mut SessionWorld, from: String, to: String) {
    let a = address(&from);
    let b = address(&to);
    let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
    let (c0, c1) = (a.column.min(b.column), a.column.max(b.column));
    world.clipboard = world.session.selection_tsv(r0, r1, c0, c1);
}

#[when(regex = r#"^I paste at ([A-Za-z]+:[0-9]+)$"#)]
fn i_paste_at(world: &mut SessionWorld, key: String) {
    let tsv = world.clipboard.clone();
    world.session.paste_tsv(address(&key), &tsv);
}

// MARK: Column widths

#[when(regex = r#"^I set column ([A-Za-z]+) width to "([0-9.]+)"$"#)]
fn i_set_column_width(world: &mut SessionWorld, column: String, width: String) {
    let col = soroban_engine::CellAddress::column_index(&column)
        .unwrap_or_else(|| panic!("'{column}' is not a column"));
    let width: f32 = width.parse().expect("width must be a number");
    world.session.set_column_width(col, width);
}

#[then(regex = r#"^column ([A-Za-z]+) width is "([0-9.]+)"$"#)]
fn column_width_is(world: &mut SessionWorld, column: String, expected: String) {
    let col = soroban_engine::CellAddress::column_index(&column)
        .unwrap_or_else(|| panic!("'{column}' is not a column"));
    let expected: f32 = expected.parse().expect("width must be a number");
    let actual = world.session.column_widths()[col];
    assert_eq!(actual, expected, "column {column} width is {actual}, expected {expected}");
}

// MARK: Workbook round trip

#[when("I save and reopen the workbook")]
fn i_save_and_reopen(world: &mut SessionWorld) {
    let path = std::env::temp_dir().join("soroban-gui-session-roundtrip.soroban");
    world
        .session
        .save_to(&path)
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    world
        .session
        .open_from(&path)
        .unwrap_or_else(|error| panic!("open failed: {error}"));
}

#[tokio::main]
async fn main() {
    SessionWorld::cucumber()
        .max_concurrent_scenarios(1) // serialized, like the engine + Swift suites
        .run_and_exit("tests/features")
        .await;
}
