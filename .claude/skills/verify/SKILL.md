---
name: verify
description: The full pre-push verification battery for both ecosystems — Swift + Rust suites, the shared gherkin runs with scenario-count checks, clippy/fmt, and the differential CLI compare. Run before pushing any engine/spec change.
---

# Verify before pushing (both ecosystems)

## The battery

```sh
# Swift — unit tests + ALL shared scenarios (PickleKit):
cd swift/Engine && swift test
# → find the line "Test scenario(_:) with N test cases passed"

# Rust — format, lint, unit, shared scenarios:
cd rust && cargo fmt --all -- --check \
        && cargo clippy --workspace --all-targets \
        && cargo test --workspace --lib \
        && cargo test -p soroban-engine --test gherkin
# → the gherkin summary prints "N scenarios (N passed)"

# ONLY if rust/gui was touched (excluded from the workspace — never --workspace):
cd rust/gui && cargo test
```

## The checks that catch what "green" doesn't

1. **Scenario COUNT, not color.** Both runners must report the SAME count,
   and it must have RISEN by exactly the scenarios you added. A vacuous run
   (0 or stale count) is indistinguishable from a real pass — the Swift
   runner once passed for weeks executing zero scenarios.
2. **Clippy: local clean ≠ CI clean.** CI runs ubuntu stable clippy with
   `-D warnings`, which tracks NEWER than local — new stable lints fire on
   untouched code after a toolchain bump. A Rust CI failure in under a
   minute is fmt/clippy, not tests. Don't claim CI-parity from a local run;
   if it matters, `rustup toolchain install <ver>` alongside (never update
   the user's default stable unasked).
3. **Differential CLI compare** for anything user-visible:
   ```sh
   swift build --product soroban            # swift/Engine/.build/debug/soroban
   cargo build --bin soroban                # rust/target/debug/soroban
   diff <(printf '<input>\n' | SWIFT_BIN 2>&1) <(printf '<input>\n' | RUST_BIN 2>&1)
   ```
   Include `:mode finance` / `:mode programmer` lines when the behavior is
   mode-scoped. Outputs must be value-identical; the binaries interleave
   stdout/stderr differently when piped, so `sort` both sides before
   declaring a real divergence.
4. **Exit codes**: pipe/one-shot exit 1 if any statement failed; script files
   halt at the first error.

## When it fails

- Swift "No such module" from SourceKit is a phantom — trust `swift test`.
- Rust gherkin stack overflow / weirdness while a subagent is editing:
  you rebuilt a half-written tree; re-run from a stable state first.
- A gherkin count mismatch between engines: one runner is skipping a
  feature file or a step is unregistered (check the step spelling matches
  the feature EXACTLY, both runners).
