//! Headless BDD suite for the Rust app's session layer — runs
//! `tests/features/session.feature` against the UI-free [`Session`] with no
//! iced and no rendering (the Rust counterpart to the Swift
//! `SorobanSessionTests`, but a fast `cargo test`). It exercises the calculator
//! (the log) and the sheet (the grid) through the same view-model the iced
//! shell drives. Rust-only by design: the cross-ecosystem parity oracle is
//! `spec/anzan`, run by the engine's gherkin suite.
//!
//! The runner, the `World`, and the shared step helpers live here; the step
//! definitions themselves are split by concern into the `session/` submodules
//! (cucumber collects them across the whole test binary via `inventory`).

use cucumber::World;
use soroban_engine::{CellAddress, CellDisplay};
use soroban_gui::session::{Outcome, Session};
use std::fmt;

mod session {
    //! The step-definition modules, grouped by concern. They register with the
    //! same `SessionWorld` via cucumber's global `inventory` collection.
    pub mod binary;
    pub mod calculator;
    pub mod grid;
    pub mod inspector;
}

/// A stand-in for the app's open inline cell editor (the App holds this state;
/// here the World does, so the point-mode steps can drive it headlessly).
pub(crate) struct Editor {
    pub address: CellAddress,
    pub draft: String,
}

#[derive(World)]
#[world(init = Self::fresh)]
pub(crate) struct SessionWorld {
    pub session: Session,
    /// A stand-in system clipboard for the copy/paste steps (TSV text).
    pub clipboard: String,
    /// The open inline editor, if any (point-mode steps).
    pub editor: Option<Editor>,
}

impl SessionWorld {
    /// One world per scenario — a fresh session, exactly what the app builds at
    /// launch. Disk-safe: nothing persists without an explicit save.
    fn fresh() -> Self {
        Self {
            // Ephemeral: never touches the real log.json / input_history.json.
            session: Session::ephemeral(),
            clipboard: String::new(),
            editor: None,
        }
    }
}

impl fmt::Debug for SessionWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SessionWorld({} log entries)",
            self.session.entries().len()
        )
    }
}

/// Parse an `A:1` cell key (panicking loudly on a bad one — a test typo).
pub(crate) fn address(key: &str) -> soroban_engine::CellAddress {
    soroban_engine::CellAddress::from_key(&key.to_uppercase())
        .unwrap_or_else(|| panic!("'{key}' is not a cell address"))
}

/// A cell's display as the comparable string a user "sees" (mirrors the engine
/// gherkin suite's `render`, so cell assertions read identically across suites).
pub(crate) fn render(display: CellDisplay) -> String {
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

pub(crate) fn shown(world: &SessionWorld, key: &str) -> String {
    render(world.session.display_at(address(key)))
}

/// The most recent log entry's outcome.
pub(crate) fn last_outcome(world: &SessionWorld) -> Outcome {
    world
        .session
        .entries()
        .last()
        .expect("no log entry to inspect")
        .outcome
        .clone()
}

#[tokio::main]
async fn main() {
    SessionWorld::cucumber()
        .max_concurrent_scenarios(1) // serialized, like the engine + Swift suites
        .run_and_exit("tests/features")
        .await;
}
