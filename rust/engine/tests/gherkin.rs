//! The parity harness: runs every scenario in `spec/anzan/*.feature` — the
//! SAME feature files the Swift implementation runs through PickleKit — with
//! a calculator wired to a fresh SheetStore, exactly the topology the app
//! builds (the port of SorobanSteps.swift). One fresh world per scenario;
//! scenarios run serialized.
//!
//! Formatting and persistence steps arrive with their modules; their
//! scenarios skip (visibly) until then, as do scenarios tagged
//! @rust-pending.

use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use soroban_engine::{
    BigDecimal, Calculator, CellAddress, CellDisplay, EngineError, EvalOutcome, LanguageMode,
    SheetStore,
};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(World)]
#[world(init = Self::fresh)]
pub struct AnzanWorld {
    calculator: Rc<RefCell<Calculator>>,
    store: SheetStore,
    outcome: Option<Result<EvalOutcome, EngineError>>,
}

impl AnzanWorld {
    /// One world per scenario: a calculator wired to a fresh SheetStore —
    /// exactly the topology the app builds.
    fn fresh() -> Self {
        let calculator = Rc::new(RefCell::new(Calculator::new()));
        let store = SheetStore::new(Rc::clone(&calculator));
        Self {
            calculator,
            store,
            outcome: None,
        }
    }
}

impl fmt::Debug for AnzanWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AnzanWorld(outcome: {:?})", self.outcome)
    }
}

fn address(key: &str) -> CellAddress {
    CellAddress::from_key(&key.to_uppercase())
        .unwrap_or_else(|| panic!("'{key}' is not a cell address"))
}

/// What a user "sees" in a cell, as a comparable string.
fn shown(world: &AnzanWorld, key: &str) -> String {
    render(world.store.display_value(address(key)))
}

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

// MARK: Log

#[given(regex = r#"^I calculate "(.*)"$"#)]
#[when(regex = r#"^I calculate "(.*)"$"#)]
#[then(regex = r#"^I calculate "(.*)"$"#)]
fn calculate(world: &mut AnzanWorld, expression: String) {
    world.outcome = Some(world.calculator.borrow_mut().evaluate(&expression));
}

#[given(regex = r"^the calculator is in (normal|programmer|finance) mode$")]
fn set_mode(world: &mut AnzanWorld, mode: String) {
    world.calculator.borrow_mut().mode =
        LanguageMode::from_name(&mode).expect("gated by the regex");
}

#[then(regex = r#"^the result is "(.*)"$"#)]
fn result_is(world: &mut AnzanWorld, expected: String) {
    match &world.outcome {
        Some(Ok(outcome)) => {
            let shown = outcome.to_string();
            assert_eq!(shown, expected, "expected {expected}, got {shown}");
        }
        other => panic!("expected a result, got {other:?}"),
    }
}

#[then(regex = r#"^the result is within "(.*)" of "(.*)"$"#)]
fn result_near_target(world: &mut AnzanWorld, bound: String, target: String) {
    near(world, &bound, &target);
}

#[then(regex = r#"^the result is within "(.*)" of zero$"#)]
fn result_near_zero(world: &mut AnzanWorld, bound: String) {
    near(world, &bound, "0");
}

fn near(world: &mut AnzanWorld, bound: &str, target: &str) {
    let value = match &world.outcome {
        Some(Ok(outcome)) => outcome
            .numeric_value()
            .unwrap_or_else(|| panic!("expected a numeric result, got {outcome}")),
        other => panic!("expected a numeric result, got {other:?}"),
    };
    let bound = BigDecimal::parse(bound).expect("a numeric bound");
    let target = BigDecimal::parse(target).expect("a numeric target");
    let diff = &value - &target;
    let magnitude = if diff.is_negative() { -&diff } else { diff };
    assert!(
        magnitude <= bound,
        "{value} is not within {bound} of {target}"
    );
}

#[then(regex = r#"^the calculation fails mentioning "(.*)"$"#)]
fn calculation_fails(world: &mut AnzanWorld, fragment: String) {
    match &world.outcome {
        Some(Err(error)) => {
            let text = error.to_string();
            assert!(
                text.contains(&fragment),
                "error '{text}' doesn't mention '{fragment}'"
            );
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[then(regex = r#"^documentation is shown mentioning "(.*)"$"#)]
fn documentation_shown(world: &mut AnzanWorld, fragment: String) {
    match &world.outcome {
        Some(Ok(EvalOutcome::Documentation(doc))) => {
            let text = format!(
                "{} {} {}",
                doc.signature,
                doc.summary,
                doc.examples.join(" ")
            );
            assert!(
                text.contains(&fragment),
                "documentation doesn't mention '{fragment}': {text}"
            );
        }
        other => panic!("expected documentation, got {other:?}"),
    }
}

// MARK: Grid

#[given(regex = r#"^cell ([A-Za-z]+:[0-9]+) contains "(.*)"$"#)]
#[when(regex = r#"^cell ([A-Za-z]+:[0-9]+) contains "(.*)"$"#)]
#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) contains "(.*)"$"#)]
fn cell_contains(world: &mut AnzanWorld, key: String, raw: String) {
    world
        .store
        .active_sheet()
        .grid
        .set_cell(Some(&raw), address(&key));
}

#[given(regex = r"^the sheet contains:$")]
fn sheet_contains(world: &mut AnzanWorld, step: &Step) {
    let table = step
        .table
        .as_ref()
        .expect("this step needs a | cell | value | table");
    let header = &table.rows[0];
    let cell_col = header
        .iter()
        .position(|h| h == "cell")
        .expect("a 'cell' column");
    let value_col = header
        .iter()
        .position(|h| h == "value")
        .expect("a 'value' column");
    for row in &table.rows[1..] {
        world
            .store
            .active_sheet()
            .grid
            .set_cell(Some(&row[value_col]), address(&row[cell_col]));
    }
}

#[given(regex = r#"^cell ([A-Za-z]+:[0-9]+) is named "(.*)"$"#)]
fn cell_named(world: &mut AnzanWorld, key: String, name: String) {
    world
        .store
        .active_sheet()
        .grid
        .set_cell_name(Some(&name), address(&key))
        .unwrap_or_else(|e| panic!("naming failed: {e}"));
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) shows "(.*)"$"#)]
fn cell_shows(world: &mut AnzanWorld, key: String, expected: String) {
    let shown = shown(world, &key);
    assert_eq!(
        shown, expected,
        "cell {key} shows '{shown}', expected '{expected}'"
    );
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) is a slider set to "(.*)"$"#)]
fn cell_is_slider(world: &mut AnzanWorld, key: String, expected: String) {
    let shown = shown(world, &key);
    assert_eq!(
        shown,
        format!("slider:{expected}"),
        "cell {key} is '{shown}', expected a slider at {expected}"
    );
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) shows an error mentioning "(.*)"$"#)]
fn cell_shows_error(world: &mut AnzanWorld, key: String, fragment: String) {
    let shown = shown(world, &key);
    assert!(
        shown.starts_with("#ERR") && shown.contains(&fragment),
        "cell {key} shows '{shown}', expected an error mentioning '{fragment}'"
    );
}

// MARK: Formatting (display-only; rendering is engine logic)

#[given(regex = r#"^cell ([A-Za-z]+:[0-9]+) is formatted as "(.*)"$"#)]
fn cell_formatted(world: &mut AnzanWorld, key: String, format_name: String) {
    use soroban_engine::NumberFormat;
    let sheet = world.store.active_sheet();
    let address = address(&key);
    let mut formats = sheet.formats.borrow_mut();
    let format = formats.entry(address).or_default();
    format.number_format = match format_name.as_str() {
        "number" => NumberFormat::Number { decimals: 2 },
        "dollars" => NumberFormat::Currency {
            symbol: "$".to_string(),
            decimals: 2,
        },
        "euros" => NumberFormat::Currency {
            symbol: "€".to_string(),
            decimals: 2,
        },
        "percent" => NumberFormat::Percent { decimals: 2 },
        "a date" => NumberFormat::Date,
        "hex" => NumberFormat::Hex,
        "binary" => NumberFormat::Binary,
        other => panic!("unknown format '{other}'"),
    };
}

#[then(regex = r#"^cell ([A-Za-z]+:[0-9]+) displays "(.*)"$"#)]
fn cell_displays(world: &mut AnzanWorld, key: String, expected: String) {
    let sheet = world.store.active_sheet();
    let address = address(&key);
    let CellDisplay::Value(value) = world.store.display_value_on(&sheet, address) else {
        panic!("cell {key} doesn't hold a value");
    };
    let format = sheet
        .formats
        .borrow()
        .get(&address)
        .cloned()
        .unwrap_or_default();
    let displayed = format.number_format.rendered(&value);
    assert_eq!(
        displayed, expected,
        "cell {key} displays '{displayed}', expected '{expected}'"
    );
}

// MARK: Worksheets

#[given(regex = r#"^a sheet named "(.*)"$"#)]
fn sheet_named(world: &mut AnzanWorld, name: String) {
    world.store.add_sheet().expect("add sheet");
    let index = world.store.sheets().len() - 1;
    world
        .store
        .rename(index, &name)
        .unwrap_or_else(|e| panic!("rename failed: {e}"));
}

#[given(regex = r#"^cell ([A-Za-z]+:[0-9]+) on "(.*)" contains "(.*)"$"#)]
fn cell_on_sheet_contains(world: &mut AnzanWorld, key: String, sheet: String, raw: String) {
    let sheet = world
        .store
        .sheet_named(&sheet)
        .unwrap_or_else(|| panic!("no sheet named '{sheet}'"));
    sheet.grid.set_cell(Some(&raw), address(&key));
}

// MARK: Persistence

/// Engine-level round trip: raws + names through the codec, into a FRESH
/// store on the same calculator (which rewires the resolvers). Types →
/// functions → variables restore exactly like the app on open.
#[when(regex = r"^the workbook is saved and reopened$")]
fn saved_and_reopened(world: &mut AnzanWorld) {
    use soroban_engine::workbook::{restore_session, SheetPayload, Workbook};
    use std::collections::HashMap;

    let payloads: Vec<SheetPayload> = world
        .store
        .sheets()
        .iter()
        .map(|sheet| {
            let mut payload = SheetPayload::new(
                sheet.name(),
                sheet
                    .grid
                    .raws()
                    .into_iter()
                    .map(|(a, raw)| (a.to_string(), raw))
                    .collect::<HashMap<String, String>>(),
            );
            payload.names = sheet
                .grid
                .cell_names()
                .into_iter()
                .map(|(a, n)| (a.to_string(), n))
                .collect();
            payload
        })
        .collect();

    let encoded = {
        let calculator = world.calculator.borrow();
        let environment = calculator.environment();
        let functions: Vec<soroban_engine::UserFunction> = environment
            .all_user_functions()
            .into_iter()
            .cloned()
            .collect();
        Workbook::new(
            payloads,
            None,
            environment.user_variables(),
            &functions,
            environment.user_data_types(),
            environment.namespace_sources().to_vec(),
            environment.imported_namespaces().to_vec(),
        )
        .encode()
        .expect("encode")
    };
    let decoded = Workbook::decode(&encoded).expect("decode");

    let store = SheetStore::new(Rc::clone(&world.calculator));
    restore_session(&mut world.calculator.borrow_mut(), &decoded);
    let mut sheets = Vec::new();
    for payload in &decoded.sheets {
        let sheet = store.make_sheet(&payload.name);
        let contents: HashMap<CellAddress, String> = payload
            .cells
            .iter()
            .filter_map(|(key, raw)| CellAddress::from_key(key).map(|a| (a, raw.clone())))
            .collect();
        let names = payload
            .names
            .iter()
            .filter_map(|(key, name)| CellAddress::from_key(key).map(|a| (a, name.clone())))
            .collect();
        sheet.grid.load(&contents);
        sheet.grid.load_cell_names(names);
        sheets.push(sheet);
    }
    let first = decoded.sheets.first().map(|p| p.name.clone());
    store.replace_sheets(sheets, first.as_deref());
    world.store = store;
}

#[tokio::main]
async fn main() {
    AnzanWorld::cucumber()
        .max_concurrent_scenarios(1) // serialized, like the Swift suite
        .filter_run_and_exit("../../spec/anzan", |_, _, scenario| {
            !scenario.tags.iter().any(|tag| tag == "rust-pending")
        })
        .await;
}
