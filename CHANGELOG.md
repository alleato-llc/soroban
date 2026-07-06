# Changelog — cross-cutting

Soroban is a two-ecosystem monorepo, and each ecosystem keeps its own changelog
under its own release track:

- **[swift/CHANGELOG.md](swift/CHANGELOG.md)** — the macOS app, engine, and CLI.
  Released as `vX.Y.Z` (signed, notarized `Soroban.dmg`) by salpa when `swift/**`
  or `spec/**` changes.
- **[rust/CHANGELOG.md](rust/CHANGELOG.md)** — the `anzan`/`soroban-engine`/`cli`
  crates and the `rust/gui` iced app. Released as `rust-vX.Y.Z` (portable
  Linux/Windows/macOS binaries) when `rust/**` or `spec/**` changes.

**This file** records only changes that span **both** ecosystems or the repo as
a whole — shared `spec/**` behavior, the monorepo layout, cross-ecosystem
interchange, and CI/release infrastructure common to both. A change that touches
only one ecosystem belongs in that ecosystem's changelog, not here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
A `spec/**` change is shared language behavior and releases both tracks; note it
here **and** in each ecosystem's changelog as it lands. `[skip ci]` on a commit
still suppresses all release workflows regardless of the paths it touches — see
[docs/RELEASING.md](docs/RELEASING.md).

## [Unreleased]

### Added

- **Stable, version-free download names for the Rust track.** `release-rust.yml`
  now attaches each `rust-v*` release under fixed public names via a
  `gh release upload --clobber` step: `Soroban-cross.dmg` (the signed, notarized
  universal macOS DMG) and `soroban-<os>-<arch>[.exe]` (the portable
  Linux/Windows binaries, `-gui` infix dropped). These names never change across
  releases, so the landing page can link a fixed URL per platform — resolving
  each track's newest tag (`v*` / `rust-v*`) via the GitHub Releases API at build
  time, since GitHub's one repo-wide "latest" can't be trusted per track. This
  is the release-side half of the platform-aware download experience.
- **Cross-ecosystem `.soroban` interchange is now proven both ways.** A new
  Rust-authored fixture `examples/interchange.soroban` (regenerate with `cargo
  run -p soroban-engine --example author_interchange`) is opened and computed by
  *both* ecosystems' suites (`rust/engine/tests/interchange.rs` + Swift's
  `InterchangeTests`), mirroring how the Swift-authored `examples/mortgage.
  soroban` is read by both — so a workbook written by either side is a permanent
  regression guard on the other. The fixture exercises what mortgage doesn't: a
  log variable, a user function, a `data`-type record, a named cell, and a saved
  bit-format variable — all restore and compute identically across Swift ⇄ Rust.

### Changed

- **Documentation reorganized ecosystem-first.** The shared language/format spec
  and a new [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (the common design) stay
  top-level in `docs/`; each ecosystem gained its own authored docs
  ([swift/README.md](swift/README.md) + `swift/docs/`,
  [rust/README.md](rust/README.md) + `rust/docs/`) and a nested `CLAUDE.md`, and
  the root `README.md`/`CLAUDE.md` were slimmed to a monorepo overview + router
  (the Swift app tour and the deep per-subsystem architecture moved into the
  Swift ecosystem docs). New [CONTRIBUTING.md](CONTRIBUTING.md) and
  [spec/README.md](spec/README.md). No behavior or code change;
  `docs/ANZAN.md` (the site-rendered spec) is unchanged.
- **Two independent release tracks, split by ecosystem.** `swift/**` (or
  `spec/**`) changes cut a **macOS release** (`release.yml` → salpa → signed,
  notarized `Soroban.dmg`, tagged `vX.Y.Z`). `rust/**` (or `spec/**`) changes cut
  a **Rust release** (`release-rust.yml` → portable Linux/Windows/macOS binaries,
  tagged `rust-vX.Y.Z` on its own version sequence). Each track is path-gated on
  push to `main` and also runs on manual `workflow_dispatch`; a `spec/**` change
  releases both. Previously *every* push to `main` cut a single macOS release
  (with the Rust binaries attached to it); the Rust binaries now have their own
  versioned track. Because GitHub exposes only one repo-wide "latest" release,
  `releases/latest/download/...` is not a reliable per-track link — stable
  per-track download URLs come from fixed asset names + build-time tag resolution
  on the landing page (see the "stable download names" entry above).
- **CI actions bumped to Node 24.** `actions/upload-artifact@v4` (Node 20, which
  GitHub was force-migrating) → `@v7` (Node 24) across the CI/release workflows,
  clearing the deprecation warning.
- **Per-ecosystem changelogs.** Split the single `CHANGELOG.md` into this
  cross-cutting file plus `swift/CHANGELOG.md` (the dated `v*` history) and
  `rust/CHANGELOG.md` (the Rust port). Ecosystem-specific bugfixes and feature
  parity are recorded in their own file; shared changes stay here.
- Restructured into an ecosystem-first monorepo (Phase 0 of
  [docs/MIGRATION.md](docs/MIGRATION.md)): everything Apple moved under
  `swift/` (`Engine/`, `App/`, `Kit/`, `project.yml`, the app's `salpa.yaml`);
  the Gherkin feature files moved to a shared top-level `spec/`
  (`spec/anzan/`, `spec/session/`), symlinked into the test targets, to serve
  as the cross-ecosystem parity oracle for the Rust port. The repo-root
  `salpa.yaml` now holds only the site deploy.
