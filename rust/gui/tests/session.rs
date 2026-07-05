//! Headless BDD suite for the Rust app's session layer — runs
//! `tests/features/session.feature` against the UI-free [`Session`] with no
//! iced and no rendering (the Rust counterpart to the Swift
//! `SorobanSessionTests`, but a fast `cargo test`). It exercises the calculator
//! (the log) and the sheet (the grid) through the same view-model the iced
//! shell drives. Rust-only by design: the cross-ecosystem parity oracle is
//! `spec/anzan`, run by the engine's gherkin suite.

use cucumber::{given, then, when, World};
use soroban_engine::{CellAddress, CellDisplay, NumberFormat};
use soroban_gui::session::{BinaryStatus, Outcome, PointClick, Session};
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

// MARK: Input history (↑/↓ recall)

#[when("I recall the previous input")]
fn i_recall_previous(world: &mut SessionWorld) {
    world.session.recall_previous();
}

#[when("I recall the next input")]
fn i_recall_next(world: &mut SessionWorld) {
    world.session.recall_next();
}

#[then(regex = r#"^the input line holds "(.*)"$"#)]
fn the_input_line_holds(world: &mut SessionWorld, expected: String) {
    let input = world.session.input();
    assert_eq!(input, expected, "input holds '{input}', expected '{expected}'");
}

// MARK: Cut / clear + raw inspection

#[when(regex = r#"^I cut ([A-Za-z]+:[0-9]+) through ([A-Za-z]+:[0-9]+)$"#)]
fn i_cut(world: &mut SessionWorld, from: String, to: String) {
    let a = address(&from);
    let b = address(&to);
    let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
    let (c0, c1) = (a.column.min(b.column), a.column.max(b.column));
    world.clipboard = world.session.selection_tsv(r0, r1, c0, c1);
    world.session.clear_range(r0, r1, c0, c1);
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) contains "(.*)"$"#)]
fn cell_contains(world: &mut SessionWorld, key: String, expected: String) {
    let raw = world.session.cell_raw(address(&key));
    assert_eq!(raw, expected, "cell {key} contains '{raw}', expected '{expected}'");
}

// MARK: Rename a named cell

#[when(regex = r#"^I rename cell ([A-Za-z]+:[0-9]+) to "(.*)"$"#)]
fn i_rename_cell(world: &mut SessionWorld, key: String, name: String) {
    world
        .session
        .set_cell_name(address(&key), &name)
        .unwrap_or_else(|error| panic!("renaming {key} failed: {error}"));
}

#[then(regex = r#"^naming cell ([A-Za-z]+:[0-9]+) "(.*)" is rejected$"#)]
fn naming_is_rejected(world: &mut SessionWorld, key: String, name: String) {
    assert!(
        world.session.set_cell_name(address(&key), &name).is_err(),
        "naming {key} '{name}' was accepted, expected it to be rejected"
    );
}

#[then(regex = r#"^the active sheet is named "(.*)"$"#)]
fn active_sheet_is_named(world: &mut SessionWorld, expected: String) {
    let name = world.session.active_sheet_name();
    assert_eq!(name, expected, "active sheet is '{name}', expected '{expected}'");
}

// MARK: Formatting (display-only)

#[when(regex = r#"^I make cell ([A-Za-z]+:[0-9]+) bold$"#)]
fn i_make_bold(world: &mut SessionWorld, key: String) {
    let address = address(&key);
    let mut format = world.session.cell_format(address);
    format.bold = true;
    world.session.apply_format(address, format);
}

#[when(regex = r#"^I format cell ([A-Za-z]+:[0-9]+) as percent$"#)]
fn i_format_percent(world: &mut SessionWorld, key: String) {
    let address = address(&key);
    let mut format = world.session.cell_format(address);
    format.number_format = NumberFormat::Percent { decimals: 2 };
    world.session.apply_format(address, format);
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) is bold$"#)]
fn cell_is_bold(world: &mut SessionWorld, key: String) {
    assert!(world.session.cell_format(address(&key)).bold, "cell {key} is not bold");
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) is formatted as percent$"#)]
fn cell_is_percent(world: &mut SessionWorld, key: String) {
    let format = world.session.cell_format(address(&key));
    assert!(
        matches!(format.number_format, NumberFormat::Percent { .. }),
        "cell {key} is not percent-formatted"
    );
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) is not bold$"#)]
fn cell_is_not_bold(world: &mut SessionWorld, key: String) {
    assert!(!world.session.cell_format(address(&key)).bold, "cell {key} is still bold");
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) is not named$"#)]
fn cell_is_not_named(world: &mut SessionWorld, key: String) {
    assert!(
        world.session.cell_name(address(&key)).is_none(),
        "cell {key} is still named"
    );
}

// MARK: Dropdown / stepper controls

#[when(regex = r#"^I pick option ([0-9]+) in the dropdown in ([A-Za-z]+:[0-9]+)$"#)]
fn i_pick_dropdown(world: &mut SessionWorld, index: String, key: String) {
    let index: usize = index.parse().expect("option index must be a number");
    world.session.set_dropdown_index(address(&key), index);
}

#[when(regex = r#"^I step ([A-Za-z]+:[0-9]+) (up|down)$"#)]
fn i_step(world: &mut SessionWorld, key: String, direction: String) {
    world.session.step_control(address(&key), direction == "up");
}

// MARK: New workbook

#[when("I start a new workbook")]
fn i_new_workbook(world: &mut SessionWorld) {
    world.session.new_workbook();
}

#[then(regex = r#"^the sheet has a control in ([A-Za-z]+:[0-9]+)$"#)]
fn sheet_has_control(world: &mut SessionWorld, key: String) {
    let target = address(&key);
    let found = world
        .session
        .control_cells()
        .iter()
        .any(|(addr, _)| *addr == target);
    assert!(found, "no control cell was enumerated at {key}");
}

// MARK: Binary bit editor

#[when("I open the bit editor")]
fn i_open_bit_editor(world: &mut SessionWorld) {
    world.session.refresh_binary();
}

#[then("the bit editor is editable")]
fn bit_editor_is_editable(world: &mut SessionWorld) {
    assert!(
        matches!(world.session.binary_status(), BinaryStatus::Editable { .. }),
        "the bit editor is not editable"
    );
}

#[then("the bit editor is not editable")]
fn bit_editor_not_editable(world: &mut SessionWorld) {
    assert!(
        matches!(world.session.binary_status(), BinaryStatus::Unavailable(_)),
        "the bit editor is unexpectedly editable"
    );
}

#[when(regex = r#"^I flip bit ([0-9]+)$"#)]
fn i_flip_bit(world: &mut SessionWorld, index: String) {
    let index: usize = index.parse().expect("bit index must be a number");
    world.session.flip_binary_bit(index);
}

#[then(regex = r#"^the bit editor value is "(.*)"$"#)]
fn bit_editor_value_is(world: &mut SessionWorld, expected: String) {
    match world.session.binary_status() {
        BinaryStatus::Editable { value, .. } => {
            assert_eq!(value, expected, "bit editor value is '{value}', expected '{expected}'")
        }
        BinaryStatus::Unavailable(reason) => panic!("bit editor unavailable: {reason}"),
    }
}

#[when("I use the bit editor value")]
fn i_use_bit_editor(world: &mut SessionWorld) {
    world.session.use_binary();
}

// MARK: Inspector + reference

#[then(regex = r#"^the inspector lists the variable "(.*)"$"#)]
fn inspector_lists_variable(world: &mut SessionWorld, name: String) {
    let found = world
        .session
        .inspector_variables()
        .iter()
        .any(|row| row.label.contains(&name));
    assert!(found, "the inspector does not list a variable '{name}'");
}

#[then(regex = r#"^the inspector lists the function "(.*)"$"#)]
fn inspector_lists_function(world: &mut SessionWorld, signature: String) {
    let found = world
        .session
        .inspector_functions()
        .iter()
        .any(|row| row.label.contains(&signature));
    assert!(found, "the inspector does not list a function '{signature}'");
}

#[then(regex = r#"^the inspector lists the data type "(.*)"$"#)]
fn inspector_lists_data_type(world: &mut SessionWorld, name: String) {
    let found = world
        .session
        .inspector_data_types()
        .iter()
        .any(|row| row.label.contains(&name));
    assert!(found, "the inspector does not list a data type '{name}'");
}

#[then(regex = r#"^the reference for "(.*)" documents it$"#)]
fn reference_documents(world: &mut SessionWorld, query: String) {
    let needle = query.to_lowercase();
    let found = world
        .session
        .reference(&query)
        .iter()
        .flat_map(|group| &group.entries)
        .any(|entry| entry.signature.to_lowercase().contains(&needle));
    assert!(found, "the reference documents nothing matching '{query}'");
}

#[tokio::main]
async fn main() {
    SessionWorld::cucumber()
        .max_concurrent_scenarios(1) // serialized, like the engine + Swift suites
        .run_and_exit("tests/features")
        .await;
}
