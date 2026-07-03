//! `soroban` — the engine at the command line. Three modes, chosen by shape:
//!
//!     soroban "0.1 + 0.2 == 0.3"        one-shot: evaluate each argument
//!     echo "sqrt(2)" | soroban          pipe: evaluate each stdin line
//!     soroban                           REPL (stdin is a terminal)
//!
//! One `Calculator` per invocation, so variables, `ans`, and user functions
//! carry across arguments/lines exactly like the app's log. The engine does
//! all the work — this file is argument plumbing and error presentation.
//! Port of swift/Engine/Sources/SorobanCLI/main.swift (LineNoise →
//! rustyline).

mod repl;

use anzan::{Calculator, EngineError, EvalOutcome, LanguageMode, Value};
use std::io::{BufRead, IsTerminal};
use std::process::ExitCode;

const CLI_VERSION: &str = "0.1.0";

const USAGE: &str = "\
soroban — Anzan, the exact calculation language (50 significant digits;
+ − × and integer ^ exact). The same engine that powers the Soroban app.

usage:
  soroban [expression ...]     evaluate arguments in one shared session
  ... | soroban                evaluate each line of stdin
  soroban                      interactive REPL (exit / quit / ⌃D to leave)

options:
  -h, --help                   this help
  --version                    print the CLI version

examples:
  soroban \"0.1 + 0.2 == 0.3\"             # 1 — exactly, no float drift
  soroban \"pmt(0.05/12, 360, 200000)\"    # spreadsheet-grade finance
  soroban \"x = 3\" \"x^2 + 1\"              # arguments share one session
  soroban \"man pmt\"                      # built-in documentation";

fn eprint_line(message: &str) {
    eprintln!("{message}");
}

/// Error display: the offending line, a caret under the column (when the
/// engine gives one — same offsets the app's log renders), the message.
fn report(error: &EngineError, input: &str, echo_input: bool, caret_indent: usize) {
    if echo_input {
        eprint_line(input);
    }
    if let Some(position) = error.position() {
        eprint_line(&format!("{}^", " ".repeat(caret_indent + position)));
    }
    eprint_line(&format!("error: {error}"));
}

pub(crate) fn evaluate(
    line: &str,
    calculator: &mut Calculator,
    pretty: bool,
    echo_input_on_error: bool,
    caret_indent: usize,
) -> bool {
    // A trailing comment (`5 + 3 # adds`) echoes after the result in pretty
    // mode — display only, kept out of pipe output.
    let trailing = if pretty {
        Calculator::trailing_comment(line)
            .map(|c| format!("  # {c}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    match calculator.evaluate(line) {
        Ok(outcome) => {
            match &outcome {
                EvalOutcome::Value(value) => {
                    let programmer_hex = || {
                        if let Value::Number(number) = value {
                            if Calculator::uses_programmer_notation(line) {
                                return number.hex_text().filter(|h| h != "0x0");
                            }
                        }
                        None
                    };
                    if let Some(block) = outcome.raw_block() {
                        // Multi-line strings (pretty JSON) print raw.
                        println!("{block}");
                    } else if pretty && programmer_hex().is_some() {
                        // The line spoke programmer (0x/0b, bit functions) —
                        // echo the integer result in hex too. Display only.
                        let hex = programmer_hex().expect("checked");
                        println!("= {value} ({hex}){trailing}");
                    } else {
                        // Echo the clean form — a fixed-width int / decimal
                        // prints as its plain number (343353 / 10.50), not
                        // its Int32(…)/Decimal(…) form.
                        let shown = value.display_description();
                        if pretty {
                            println!("= {shown}{trailing}");
                        } else {
                            println!("{shown}");
                        }
                    }
                }
                EvalOutcome::FunctionDefined { signature } => {
                    if pretty {
                        println!("λ {signature}");
                    } else {
                        println!("{signature}");
                    }
                }
                EvalOutcome::DataDefined { declaration } => {
                    if pretty {
                        println!("𝑫 {declaration}");
                    } else {
                        println!("{declaration}");
                    }
                }
                EvalOutcome::Documentation(_) => println!("{outcome}"),
                EvalOutcome::Comment(text) => {
                    // A standalone note: echo it (pretty keeps the # marker).
                    if pretty {
                        println!("# {text}");
                    } else {
                        println!("{text}");
                    }
                }
            }
            true
        }
        Err(error) => {
            report(&error, line, echo_input_on_error, caret_indent);
            false
        }
    }
}

/// `:mode [normal|programmer|finance]` — show or set the input/display
/// dialect. Programmer mode reads `^` as XOR, `&`/`|` as AND/OR, `<<`/`>>`
/// as shifts, and `%` as modulo (power becomes pow); see docs/MODES.md.
pub(crate) fn handle_mode_command(line: &str, calculator: &mut Calculator, quiet: bool) -> bool {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() != 2 {
        if !quiet {
            println!(
                "mode: {} — use :mode normal|programmer|finance",
                calculator.mode.name()
            );
        }
        return true;
    }
    let Some(mode) = LanguageMode::from_name(parts[1].trim().to_lowercase().as_str()) else {
        eprint_line(&format!(
            "unknown mode '{}' — use normal, programmer, or finance",
            parts[1]
        ));
        return false;
    };
    calculator.mode = mode;
    if !quiet {
        println!("mode: {}", mode.name());
    }
    true
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if arguments.iter().any(|a| a == "--version") {
        println!("{CLI_VERSION}");
        return ExitCode::SUCCESS;
    }

    let mut calculator = Calculator::new();
    let pretty_output = std::io::stdout().is_terminal();

    // One-shot: every argument is a line; first failure poisons the exit
    // code but later arguments still run (their results may not depend on
    // it).
    if !arguments.is_empty() {
        let mut all_succeeded = true;
        for line in &arguments {
            if !evaluate(line, &mut calculator, pretty_output, true, 0) {
                all_succeeded = false;
            }
        }
        return exit_code(all_succeeded);
    }

    if std::io::stdin().is_terminal() {
        return repl::run(calculator, CLI_VERSION);
    }

    // Pipe: keep going on errors (stderr carries them), exit 1 if any
    // failed.
    let mut all_succeeded = true;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == ":mode" || trimmed.starts_with(":mode ") {
            // Mode switches work in piped scripts too — silent (not a
            // result line).
            if !handle_mode_command(trimmed, &mut calculator, true) {
                all_succeeded = false;
            }
            continue;
        }
        if !evaluate(&line, &mut calculator, false, true, 0) {
            all_succeeded = false;
        }
    }
    exit_code(all_succeeded)
}

fn exit_code(all_succeeded: bool) -> ExitCode {
    if all_succeeded {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
