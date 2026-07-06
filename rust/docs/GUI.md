# The `rust/gui` app (iced + rime)

The Rust Soroban desktop app on [iced](https://iced.rs) 0.14, built with **rime**
(the house component kit, a relative path-dependency at `../../../rime/rime`).
**Workspace-excluded** (see [ARCHITECTURE.md](ARCHITECTURE.md)) — it builds and
tests standalone with its own target dir, never via `--workspace`. Package
`soroban-gui`, with both a `[[bin]]` (`src/main.rs`) and a `[lib]`
(`src/lib.rs`).

```sh
cd rust/gui && cargo build          # or cargo test / cargo clippy
cd rust/gui && cargo test --test session
```

The app depends on `soroban-engine` (the whole engine), `rime` + `iced`, and a
few gui-only crates the workspace crates deliberately avoid: `rfd` (native
Open/Save dialogs iced lacks), `png` (the screenshot harness), `dirs` (the
per-user data dir), and serde/serde_json (log/history persistence — pinned here
since gui is workspace-excluded and can't inherit `[workspace.dependencies]`).

> The counterpart to the Swift SwiftUI `App`. Slices ①–⑥ and the remaining
> fidelity gaps are tracked in [../../docs/MIGRATION.md](../../docs/MIGRATION.md)
> (Phase 3b).

## Two halves: `Session` (UI-free) and the iced shell

The crate splits cleanly so the model can be tested without rendering:

- **`src/lib.rs`** exposes `pub mod session` — the **UI-free** engine-facing
  view-model. It exists as a library (not only inside the binary) so the headless
  cucumber suite drives `Session` directly, no iced, no rendering.
- **`src/main.rs`** is the iced shell: `App` state → `Message` → `update` →
  `view`, wrapping a `Session`. The shell modules are private (`mod …`) to the
  binary crate.

### `session.rs` — the view-model (the Swift `CalculatorSession`)

Owns the shared `Calculator` (variables, `ans`, user functions), the `SheetStore`
wired to it (log lines can reference cells and mutate the grid), the log tape, and
the ↑/↓ input history — free of any iced concern. Split by concern:

| Submodule | Concern |
|---|---|
| `session/document.rs` | data sheets (CSV import, the working SQLite store) + the workbook lifecycle (build/save/open/new/reset) |
| `session/cells.rs` | cell read/write, Excel-style point mode, TSV copy/paste, the undo/redo edit machinery |
| `session/controls.rs` | checkbox/dropdown/slider/stepper rewrites, each an undoable literal edit |
| `session/formats.rs` | display-only cell formats + named cell locations (rename reference rewriting) |
| `session/log.rs` | the log: input, autocomplete, language mode, submit/evaluate, the ↑/↓ recall tape |
| `session/inspector.rs` | the environment inspector (live vars/functions/data types) + the reference window |
| `session/binary.rs` | the binary bit-editor: register draft, width picker, presets/layouts, the visual builder, field decode/encode |
| `session/worksheets.rs` | worksheet tabs: naming, activation, add/rename/remove |
| `session/persistence.rs` | log-tape + ↑/↓ history persistence (mirrors Swift's `LogStore`): the per-user data dir, load/save |

Sibling `session/tests.rs` holds the unit tests (persistence round-trips,
worksheet management).

### The iced shell modules (`src/main.rs` + siblings)

| Module | Role |
|---|---|
| `message.rs` | the `Message` type — every event the shell reacts to (~140 variants), isolated so `update`/`view` read cleanly |
| `update.rs` | the `update` reducer + its state-mutation helpers (event handling) |
| `state.rs` | state-mutation helpers shared by the reducer (draft loading, suggestions, selection/point-mode, formatting, the cell menu) |
| `view.rs` | top-level view assembly: theme/font accessors, input subscription, window title, menu bar, `view` itself |
| `panels.rs` | the side/bottom panels + the two main bodies (reference, inspector, log view, grid view) |
| `render.rs` | pure rendering helpers (engine values → grid cells / log rows, format-bar presets, palette-color mapping, per-cell control widgets); kept free of `App` so they stay testable |
| `binary_panel.rs` | the binary bit-editor strip + its visual format builder |
| `settings.rs` | the Settings window sections (appearance, live preview, calculator) |
| `themes.rs` | the ten named palettes ported to rime's nine-token `Palette` (see below) |
| `shot.rs` | the permanent screenshot harness (see below) |

`app_tests.rs` (sibling to `main.rs`) holds crate-root unit tests (selection
movement, the font picker, zoom clamping).

## Theming — Swift JSON → rime `Palette`

`themes.rs` ports the ten AppKit themes to rime's nine-token `Palette`. Six
(Dracula, GitHub, Gruvbox Dark, Nord, Solarized Dark/Light) rime already ships
tuned, so they're reused; the other four (One Light, Soroban Dark/Light, Terminal
Green) are defined here from `swift/App/Resources/Themes/*.json`. The Swift themes
carry seven colors, rime wants nine: `windowBackground→bg`,
`inputBackground→surface`, `resultText→ink`, `secondaryText→muted`,
`accent→accent`, `errorText→danger`; the extra `hairline`/`success`/`warn` have
no Swift source and are chosen to fit.

## The screenshot harness (`src/shot.rs`) — permanent, env-gated

A permanent dev affordance for reviewing a slice as a PNG without a display: iced
captures its own window via wgpu readback (`window::screenshot`), sidestepping the
macOS screen-recording prompt and working headlessly. It is **inert** unless
`SOROBAN_SHOT` is set (`configure` returns early, `App::shot` stays `None`).
**Never re-add or remove this plumbing** — extend it with new `SOROBAN_SHOT_*`
vars for new views. Everything is env-parameterized (no code edits per shot):

```sh
cd rust/gui && SOROBAN_SHOT=/tmp/out.png SOROBAN_SHOT_VIEW=grid cargo run -q
```

Knobs: `SOROBAN_SHOT` (path, enable), `_SEED` (a file of log/`updateCell` lines to
populate state), `_VIEW=grid`, `_SELECT=B4`, `_MENU=file|edit|view`, `_EDIT=1`,
`_TYPE=<text>` (seed live input for the autocomplete popup), `_SETTINGS`,
`_THEME`, `_FONT`, `_PANEL=inspector|reference|bits`, plus the bit-editor knobs
`_WIDTH`, `_FORMAT`, `_BUILD`. Capture waits three painted frames (fonts/layout
settle) then screenshots and exits.

## Tests

The port's own net (not a cross-ecosystem oracle — that's `spec/anzan`, run by the
engine crate). `gui/tests/session.rs` is the headless cucumber runner over the
UI-free `Session` (`harness = false`); its step definitions split by concern into
`gui/tests/session/{binary,calculator,grid,inspector}.rs`, and the scenarios live
in `gui/tests/features/session.feature`. The counterpart to Swift's
`SorobanSessionTests`, but a fast `cargo test`. Unit tests are sibling files
(`app_tests.rs`, `session/tests.rs`, `themes/tests.rs`).

## See also

- [ARCHITECTURE.md](ARCHITECTURE.md) — the workspace/exclusion rationale ·
  [ENGINE.md](ENGINE.md) — the engine it hosts.
- [../README.md](../README.md) — build/run + the `SOROBAN_SHOT_*` harness ·
  [../../docs/MIGRATION.md](../../docs/MIGRATION.md) — slice plan + gaps.
