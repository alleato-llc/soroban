# The `rust/gui` app (iced + rime)

The Rust Soroban desktop app on [iced](https://iced.rs), built with **rime** (the
house component kit, a relative path-dependency). Workspace-excluded — builds and
tests standalone. Its UI-free `Session` (`src/session.rs`, split into
`session/{document,cells,binary,log,inspector,formats,controls,worksheets,
persistence}`) is the headless view-model the cucumber suite drives.

> **Status:** skeleton — full content lands in Phase 2, documenting the shell
> module split (`message`, `update`, `state`, `view`, `render`, `settings`,
> `binary_panel`, `panels`) and the permanent screenshot harness (`src/shot.rs`).

## See also

- [ARCHITECTURE.md](ARCHITECTURE.md) · [../README.md](../README.md) — build/run +
  the `SOROBAN_SHOT_*` harness · [../../docs/MIGRATION.md](../../docs/MIGRATION.md).
