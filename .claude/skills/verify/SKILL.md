---
name: verify
description: The full pre-push verification battery across all runners — Swift + Rust suites, the ts/wasm run, the site Playwright smoke, the shared gherkin runs with scenario-count checks, clippy/fmt, and the differential CLI compare. Run before pushing any engine/spec change.
---

# Verify before pushing (all runners)

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

# ts — fresh wasm + binding tests + the THIRD parity runner (cucumber-js):
cd ts && npm run build:wasm && npm test && npm run spec
# → cucumber-js summary; count guard: 279 scenarios (the language subset)

# Site — Playwright smoke on the live REPL island (vendored wasm, on purpose):
cd site && npx playwright test
# → 12 passed

# ONLY if rust/gui was touched (excluded from the workspace — never --workspace):
cd rust/gui && cargo test
```

## The checks that catch what "green" doesn't

1. **Scenario COUNT, not color — the three-runner rule.** Swift and Rust must
   report the IDENTICAL count, risen by exactly the scenarios you added. The
   ts runner executes the language SUBSET of the spec, so its count is lower
   by design (currently 279 vs 579) — but it must also rise when you add
   scenarios in features it runs, and must never silently drop. A vacuous run
   (0 or stale count) is indistinguishable from a real pass — the Swift
   runner once passed for weeks executing zero scenarios.
2. **Local green ≠ CI green across toolchain/arch.** CI runs toolchains and
   architectures that track NEWER or DIFFERENT than your machine, and both
   axes have bitten:
   - *Toolchain:* ubuntu stable clippy with `-D warnings` gained the
     `question_mark` lint in 1.97 and failed untouched code that was clean
     locally. A Rust CI failure in under a minute is fmt/clippy, not tests.
   - *Architecture:* macos-14 runners are x86_64; the gherkin runner's deep
     recursion produced larger x86_64 stack frames that overflowed the
     128KB headroom local arm64 never hit (the thread stack is now 256KB).
   Don't claim CI-parity from a local run; if it matters,
   `rustup toolchain install <ver>` alongside (never update the user's
   default stable unasked), and remember the CI matrix covers an arch you
   probably aren't on.
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
