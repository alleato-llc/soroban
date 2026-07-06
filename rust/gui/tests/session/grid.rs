//! Sheet (the grid) steps: cell read/write, named cells, Excel point mode,
//! undo/redo, controls, copy/cut/paste, column widths, the workbook round trip,
//! input-history recall, data-sheet import, formatting, and New.

use crate::{address, shown, Editor, SessionWorld};
use cucumber::{then, when};
use soroban_engine::NumberFormat;
use soroban_gui::session::PointClick;

// MARK: Sheet (the grid)

#[when(regex = r#"^I set cell ([A-Za-z]+:[0-9]+) to "(.*)"$"#)]
fn i_set_cell(world: &mut SessionWorld, key: String, raw: String) {
    world.session.set_cell_raw(address(&key), &raw);
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) shows "(.*)"$"#)]
fn cell_shows(world: &mut SessionWorld, key: String, expected: String) {
    let shown = shown(world, &key);
    assert_eq!(
        shown, expected,
        "cell {key} shows '{shown}', expected '{expected}'"
    );
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
    world.session.clear_point_anchor(); // the app resets on beginEditing
    world.editor = Some(Editor { address, draft });
}

#[when(regex = r#"^I type "(.*)" into the editor$"#)]
fn i_type_into_editor(world: &mut SessionWorld, text: String) {
    world.editor.as_mut().expect("no editor is open").draft = text;
}

#[when(regex = r#"^I click cell ([A-Za-z]+:[0-9]+)$"#)]
fn i_click_cell(world: &mut SessionWorld, key: String) {
    point_click(world, &key, false);
}

#[when(regex = r#"^I shift-click cell ([A-Za-z]+:[0-9]+)$"#)]
fn i_shift_click_cell(world: &mut SessionWorld, key: String) {
    point_click(world, &key, true);
}

/// Drive one point-mode click through the session, exactly as the app's
/// `select_or_point` does — insert keeps the editor open on the new draft, a
/// commit writes the cell and closes it.
fn point_click(world: &mut SessionWorld, key: &str, extend: bool) {
    let editor = world
        .editor
        .take()
        .expect("no editor is open to click from");
    match world
        .session
        .point_click(&editor.draft, address(key), extend)
    {
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
    assert_eq!(
        *draft, expected,
        "editor holds '{draft}', expected '{expected}'"
    );
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
    assert_eq!(
        actual, expected,
        "column {column} width is {actual}, expected {expected}"
    );
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
    assert_eq!(
        input, expected,
        "input holds '{input}', expected '{expected}'"
    );
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
    assert_eq!(
        raw, expected,
        "cell {key} contains '{raw}', expected '{expected}'"
    );
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
    assert_eq!(
        name, expected,
        "active sheet is '{name}', expected '{expected}'"
    );
}

// MARK: Data sheets (CSV import → SQLite table)

#[when(regex = r#"^I import a CSV "(.*)" with rows "(.*)"$"#)]
fn i_import_csv(world: &mut SessionWorld, name: String, rows: String) {
    // `;`-separated records, `,`-separated fields — a compact inline CSV.
    let csv: String = rows.split(';').collect::<Vec<_>>().join("\n");
    // A per-import temp dir so the file's stem (which becomes the sheet name)
    // is exactly `name`, uncontaminated by a disambiguating prefix.
    let dir = std::env::temp_dir().join(format!("soroban-gui-import-{name}"));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("temp dir failed: {error}"));
    let path = dir.join(format!("{name}.csv"));
    std::fs::write(&path, csv).unwrap_or_else(|error| panic!("writing the CSV failed: {error}"));
    world
        .session
        .import_csv(&path)
        .unwrap_or_else(|error| panic!("import failed: {error}"));
}

#[then("the active sheet is a data sheet")]
fn the_active_sheet_is_a_data_sheet(world: &mut SessionWorld) {
    assert!(
        world.session.active_is_data(),
        "the active sheet is not a data sheet"
    );
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
    assert!(
        world.session.cell_format(address(&key)).bold,
        "cell {key} is not bold"
    );
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
    assert!(
        !world.session.cell_format(address(&key)).bold,
        "cell {key} is still bold"
    );
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
