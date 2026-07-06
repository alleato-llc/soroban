//! Smoke tests for the `soroban` CLI's two non-interactive modes.
//!
//! The CLI has three modes chosen by invocation shape (see `main.rs`):
//! command-line ARGUMENTS (one shared session), piped STDIN (line per line),
//! and an interactive TTY REPL. Only the first two are observable from an
//! integration test — the REPL (`repl::run`) needs a real terminal and is
//! intentionally left uncovered here, mirroring the Swift CLI's policy of
//! excluding its interactive plumbing from coverage.
//!
//! Because the test harness pipes the child's stdout, `stdout().is_terminal()`
//! is false, so `pretty_output` is off in BOTH modes: values print plain
//! (`5`), with no `= ` prefix and no trailing-comment / hex echo. Errors go to
//! stderr (echoed line + caret + `error: …`) and poison the exit code.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the built `soroban` binary with `args`, feeding `stdin` (empty string
/// = no piped input, i.e. arguments mode). Returns `(stdout, stderr, code)`.
fn run(args: &[&str], stdin: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_soroban"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn soroban binary");

    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin.as_bytes())
        .expect("write to child stdin");

    let output = child.wait_with_output().expect("wait for child");
    (
        String::from_utf8(output.stdout).expect("stdout utf8"),
        String::from_utf8(output.stderr).expect("stderr utf8"),
        output.status.code().expect("exit code"),
    )
}

/// Arguments mode: no piped stdin.
fn args(args: &[&str]) -> (String, String, i32) {
    run(args, "")
}

/// Pipe mode: no arguments, feed lines on stdin.
fn pipe(stdin: &str) -> (String, String, i32) {
    run(&[], stdin)
}

// ---- Arguments mode -------------------------------------------------------

#[test]
fn single_expression_prints_plain_value() {
    let (out, err, code) = args(&["2 + 3"]);
    assert_eq!(out, "5\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn exact_arithmetic_no_float_drift() {
    // The headline invariant: exact decimals, no binary float rounding.
    let (out, _err, code) = args(&["0.1 + 0.2 == 0.3"]);
    assert_eq!(out, "1\n");
    assert_eq!(code, 0);
}

#[test]
fn multiple_arguments_share_one_session_via_variable() {
    // `x = 3` echoes its stored value, then `x^2 + 1` reads it back.
    let (out, _err, code) = args(&["x = 3", "x^2 + 1"]);
    assert_eq!(out, "3\n10\n");
    assert_eq!(code, 0);
}

#[test]
fn ans_carries_across_arguments() {
    // The implicit `ans` accumulator threads through the shared session.
    let (out, _err, code) = args(&["10", "ans * 2"]);
    assert_eq!(out, "10\n20\n");
    assert_eq!(code, 0);
}

#[test]
fn function_definition_then_use() {
    // A definition line prints its signature (plain), then the call evaluates.
    let (out, _err, code) = args(&["double(n) = n * 2", "double(21)"]);
    assert_eq!(out, "double(n)\n42\n");
    assert_eq!(code, 0);
}

#[test]
fn data_definition_prints_declaration() {
    let (out, _err, code) = args(&["data Point { x: Number, y: Number }"]);
    assert_eq!(out, "data Point { x: Number, y: Number }\n");
    assert_eq!(code, 0);
}

#[test]
fn standalone_comment_is_echoed_not_an_error() {
    // A comment-only line is a recorded note, not a parse error: plain mode
    // drops the `#` marker and exits 0.
    let (out, err, code) = args(&["# hello there"]);
    assert_eq!(out, "hello there\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn parse_error_renders_caret_and_fails() {
    // A parse error carries a column position → echoed line + caret + message
    // on stderr, exit 1.
    let (out, err, code) = args(&["2 +"]);
    assert_eq!(out, "");
    assert_eq!(code, 1);
    assert!(err.contains("2 +"), "stderr should echo the input: {err:?}");
    assert!(err.contains('^'), "stderr should carry a caret: {err:?}");
    assert!(
        err.contains("error:"),
        "stderr should carry the message: {err:?}"
    );
    // The caret sits under the offending column (0 indent in args mode).
    assert!(err.contains("   ^\n"), "caret column off: {err:?}");
}

#[test]
fn runtime_error_without_position_still_fails() {
    // Division by zero has no source position → no caret, but still exit 1
    // with the message on stderr.
    let (out, err, code) = args(&["1 / 0"]);
    assert_eq!(out, "");
    assert_eq!(code, 1);
    assert!(
        err.contains("division by zero"),
        "expected the runtime error: {err:?}"
    );
    assert!(
        !err.contains('^'),
        "no caret for a position-less error: {err:?}"
    );
}

#[test]
fn later_arguments_run_even_after_an_earlier_error() {
    // First failure poisons the exit code, but subsequent arguments still run.
    let (out, err, code) = args(&["nope +", "7 * 6"]);
    assert_eq!(out, "42\n", "the good argument still prints");
    assert_eq!(code, 1, "the earlier error poisons the exit code");
    assert!(err.contains("error:"), "the error is reported: {err:?}");
}

// ---- Pipe mode ------------------------------------------------------------

#[test]
fn pipe_evaluates_each_line_in_order() {
    let (out, err, code) = pipe("2 + 3\nsqrt(4)\n10 * 10\n");
    assert_eq!(out, "5\n2\n100\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn pipe_shares_one_session_across_lines() {
    let (out, _err, code) = pipe("x = 5\nx * 2\n");
    assert_eq!(out, "5\n10\n");
    assert_eq!(code, 0);
}

#[test]
fn pipe_blank_lines_are_skipped() {
    let (out, _err, code) = pipe("1\n\n2\n");
    assert_eq!(out, "1\n2\n");
    assert_eq!(code, 0);
}

#[test]
fn pipe_comment_only_line_echoed_plain() {
    // Plain mode drops the `#` and keeps going.
    let (out, err, code) = pipe("# note\n7\n");
    assert_eq!(out, "note\n7\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn pipe_one_bad_line_poisons_exit_but_good_lines_print() {
    // A mid-stream error is reported on stderr and flips the exit code to 1,
    // yet the surrounding good lines still produce their output on stdout.
    let (out, err, code) = pipe("x = 5\nx +\nx * 2\n");
    assert_eq!(out, "5\n10\n", "good lines still print, in order");
    assert_eq!(code, 1, "any failed line → exit 1");
    assert!(err.contains('^'), "the bad line renders a caret: {err:?}");
    assert!(
        err.contains("error:"),
        "the bad line reports an error: {err:?}"
    );
}

#[test]
fn pipe_mode_command_switches_dialect_silently() {
    // `:mode programmer` is silent (not a result line); afterwards `&` is
    // bitwise AND, so `5 & 3` == 1.
    let (out, err, code) = pipe(":mode programmer\n5 & 3\n");
    assert_eq!(
        out, "1\n",
        "only the expression prints, mode switch is silent"
    );
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn pipe_unknown_mode_reports_and_fails() {
    let (out, err, code) = pipe(":mode bogus\n");
    assert_eq!(out, "");
    assert_eq!(code, 1);
    assert!(
        err.contains("unknown mode 'bogus'"),
        "expected the unknown-mode message: {err:?}"
    );
}

// ---- Flags ----------------------------------------------------------------

#[test]
fn version_flag_prints_version() {
    let (out, err, code) = args(&["--version"]);
    assert_eq!(out, "0.1.0\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

#[test]
fn help_flag_prints_usage() {
    let (out, err, code) = args(&["--help"]);
    assert_eq!(err, "");
    assert_eq!(code, 0);
    assert!(out.starts_with("soroban — Anzan"), "usage banner: {out:?}");
    assert!(out.contains("usage:"), "usage section present: {out:?}");
    assert!(out.contains("interactive REPL"), "REPL mentioned: {out:?}");
}

#[test]
fn short_help_flag_matches_long() {
    let (long, _, _) = args(&["--help"]);
    let (short, _, code) = args(&["-h"]);
    assert_eq!(short, long);
    assert_eq!(code, 0);
}

#[test]
fn help_takes_precedence_over_an_expression() {
    // `-h`/`--help` is scanned across all arguments before evaluation.
    let (out, _err, code) = args(&["2 + 2", "--help"]);
    assert!(out.starts_with("soroban — Anzan"), "help wins: {out:?}");
    assert_eq!(code, 0);
}
