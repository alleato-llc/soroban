# Soroban — Rust ecosystem

A second, independent implementation of the same Anzan language and `.soroban`
formats, on a cargo workspace. The `anzan`, `soroban-engine`, and `soroban` CLI
crates are complete and pass the shared `spec/` suite; `rust/gui` is the
[iced](https://iced.rs) desktop app (built on **rime**, the house component kit).

For the shared design and the language, start with
[../docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) and
[../docs/ANZAN.md](../docs/ANZAN.md). Deeper implementation docs are in
[docs/](docs/); the porting history is [../docs/MIGRATION.md](../docs/MIGRATION.md).

## The workspace

| Crate | What it is | In workspace? |
|---|---|---|
| `anzan/` | The language (lexer → parser → evaluator + number + functions). No grid/file knowledge. | yes |
| `engine/` | `soroban-engine` — the hosting layer: sheets + persistence. `pub use`s anzan. | yes |
| `cli/` | The `soroban` binary. Depends on `anzan` only. | yes |
| `gui/` | The iced + rime desktop app. | **excluded** (see below) |

**`gui` is deliberately excluded from the workspace** (`exclude = ["gui"]` in
`rust/Cargo.toml`): it depends on the sibling **rime** kit by a relative path
(`../../../rime/rime`) plus iced/wgpu, so it builds and tests standalone with its
own target dir — not via `--workspace`. (`Phase 4` of the migration folds it
back in.)

Detailed structure: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) ·
[docs/ENGINE.md](docs/ENGINE.md) · [docs/CLI.md](docs/CLI.md) ·
[docs/GUI.md](docs/GUI.md) · the `anzan` crate: [docs/ANZAN.md](docs/ANZAN.md).

## Build & test

```sh
# The workspace crates (anzan + engine + cli share one target dir)
cd rust && cargo test --workspace --lib
cd rust && cargo test -p soroban-engine --test gherkin   # shared spec/anzan, 522 scenarios

# The gui app builds/tests standalone (NOT via --workspace)
cd rust/gui && cargo build          # or: cargo test / cargo clippy
cd rust/gui && cargo test --test session   # the Rust-only session cucumber suite
```

The `gherkin` run executes the SAME `spec/**` features as the Swift engine — the
cross-ecosystem parity oracle. See [../spec/README.md](../spec/README.md).

The gui has a permanent, env-gated screenshot harness (`src/shot.rs`) — extend it
with new `SOROBAN_SHOT_*` vars; never re-add or remove the plumbing:

```sh
cd rust/gui && SOROBAN_SHOT=/tmp/out.png SOROBAN_SHOT_VIEW=grid cargo run -q
```

## Agent notes

Working in `rust/`? See [CLAUDE.md](CLAUDE.md) for the ecosystem's conventions
(loaded automatically alongside the root [../CLAUDE.md](../CLAUDE.md)).
