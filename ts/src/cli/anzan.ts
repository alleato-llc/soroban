#!/usr/bin/env node
// anzan — the engine at the command line: the fourth Anzan CLI, matching the
// Swift and Rust `soroban` binaries (swift/docs/CLI.md, rust/docs/CLI.md).
// Four modes, chosen by invocation shape:
//
//     anzan "0.1 + 0.2 == 0.3"        one-shot: evaluate each argument
//     anzan change.anzan              script file: halts at its first error
//     echo "sqrt(2)" | anzan          pipe: evaluate each stdin statement
//     anzan                           REPL (stdin is a terminal)
//
// One `Calculator` per invocation, so variables, `ans`, and user functions
// carry across arguments/lines exactly like the app's log. The engine does
// all the work — this file is argument plumbing and error presentation.

import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import process from "node:process";
import {
  Calculator,
  StatementAccumulator,
  trailingComment,
  type AnzanError,
  type EvalSuccess,
} from "../index.js";

const CLI_VERSION = "0.1.0";

const USAGE = `\
anzan — Anzan, the exact calculation language (50 significant digits;
+ − × and integer ^ exact). The same engine that powers the Soroban app,
compiled to WebAssembly.

usage:
  anzan [arg ...]              evaluate arguments in one shared session;
                               an argument ending in .anzan runs as a script
                               file (halts at its first error)
  ... | anzan                  evaluate stdin, one statement per line — an
                               open ( [ { continues the statement across lines
  anzan                        interactive REPL (exit / quit / ⌃D to leave)

options:
  -h, --help                   this help
  --version                    print the CLI version

examples:
  anzan "0.1 + 0.2 == 0.3"             # 1 — exactly, no float drift
  anzan "pmt(0.05/12, 360, 200000)"    # spreadsheet-grade finance
  anzan "x = 3" "x^2 + 1"              # arguments share one session
  anzan change.anzan                   # run a script file
  anzan lib.anzan "changeFor(0.95)"    # load a script, then evaluate
  anzan "man pmt"                      # built-in documentation

scripts: one statement per line; inside an unclosed ( [ { the statement
continues onto the next line. \`#\` comments; a \`#!/usr/bin/env anzan\`
shebang line makes a chmod +x .anzan file directly executable.`;

/** Error display: the offending line, a caret under the column (when the
 * engine gives one — the same offsets the app's log renders), the message. */
function report(error: AnzanError, input: string, echoInput: boolean, caretIndent: number): void {
  if (echoInput) process.stderr.write(`${input}\n`);
  if (error.position !== undefined) {
    process.stderr.write(`${" ".repeat(caretIndent + error.position)}^\n`);
  }
  process.stderr.write(`error: ${error.error}\n`);
}

function printSuccess(outcome: EvalSuccess, pretty: boolean, trailing: string): void {
  switch (outcome.kind) {
    case "value":
      if (outcome.rawBlock !== undefined) {
        // Multi-line strings (pretty JSON) print raw.
        console.log(outcome.rawBlock);
      } else if (pretty) {
        console.log(`= ${outcome.displayDescription}${trailing}`);
      } else {
        console.log(outcome.displayDescription);
      }
      break;
    case "function":
      console.log(pretty ? `λ ${outcome.description}` : outcome.description);
      break;
    case "data":
      console.log(pretty ? `𝑫 ${outcome.description}` : outcome.description);
      break;
    case "documentation":
      console.log(outcome.description);
      break;
    case "comment":
      // A standalone note: echo it (pretty keeps the # marker; the canonical
      // description is `# text`).
      console.log(pretty ? outcome.description : outcome.description.replace(/^# /, ""));
      break;
  }
}

function evaluateLine(
  line: string,
  calculator: Calculator,
  pretty: boolean,
  echoInputOnError: boolean,
  caretIndent: number,
): boolean {
  // A trailing comment (`5 + 3 # adds`) echoes after the result in pretty
  // mode — display only, kept out of pipe output.
  const comment = pretty ? trailingComment(line) : undefined;
  const trailing = comment === undefined ? "" : `  # ${comment}`;
  const outcome = calculator.evaluate(line);
  if (!outcome.ok) {
    report(outcome, line, echoInputOnError, caretIndent);
    return false;
  }
  printSuccess(outcome, pretty, trailing);
  return true;
}

/** The dialect + style, for the `:mode` status line — "scientific eng" when
 * the ENG variant is on, otherwise just the mode name. */
function modeText(calculator: Calculator): string {
  return calculator.mode === "scientific" && calculator.sciStyle === "eng"
    ? "scientific eng"
    : calculator.mode;
}

/** `:mode [normal|programmer|scientific [eng]]` — show or set the
 * input/display dialect (docs/MODES.md). The parsing itself is the engine's
 * one shared seam (`setModeParsing`), so the mode list and the unknown-mode
 * errors (`:mode finance` gets the currency-promotion hint) can't drift from
 * the native CLIs'. */
function handleModeCommand(line: string, calculator: Calculator, quiet: boolean): boolean {
  const space = line.indexOf(" ");
  if (space < 0) {
    if (!quiet) {
      console.log(`mode: ${modeText(calculator)} — use :mode normal|programmer|scientific [eng]`);
    }
    return true;
  }
  try {
    calculator.setModeParsing(line.slice(space + 1));
  } catch (e) {
    process.stderr.write(`${e instanceof Error ? e.message : String(e)}\n`);
    return false;
  }
  if (!quiet) console.log(`mode: ${modeText(calculator)}`);
  return true;
}

/** Runs a `.anzan` script file: statements split by the engine's accumulator
 * (an open bracket continues a statement across lines), evaluated in the
 * shared session, HALTING at the first error (script semantics — the caret
 * error gains a trailing `at file:line`). Comment-only statements (including
 * a `#!` shebang) are for the file's reader, not the output. */
function runScriptFile(path: string, calculator: Calculator, pretty: boolean): boolean {
  let source: string;
  try {
    source = readFileSync(path, "utf8");
  } catch {
    process.stderr.write(`error: can't read '${path}'\n`);
    return false;
  }
  const accumulator = new StatementAccumulator();
  for (const line of source.split("\n")) {
    const statement = accumulator.push(line);
    if (!statement) continue;
    const text = statement.text;
    if (text.startsWith("#")) continue; // shebang / comment: not output
    if (text === ":mode" || text.startsWith(":mode ")) {
      if (!handleModeCommand(text, calculator, true)) return false;
      continue;
    }
    if (!evaluateLine(text, calculator, pretty, true, 0)) {
      process.stderr.write(`at ${path}:${statement.line}\n`);
      return false;
    }
  }
  const pending = accumulator.pendingText();
  const error = accumulator.finish();
  if (error) {
    report(error, pending, true, 0);
    process.stderr.write(`at ${path}\n`);
    return false;
  }
  return true;
}

/** Pipe: statement-aware (an open bracket continues onto the next line —
 * streaming, one statement out as soon as it closes), keep going on errors
 * (stderr carries them), exit 1 if any failed. Plain output, like the native
 * CLIs. */
function runPipe(calculator: Calculator): void {
  const accumulator = new StatementAccumulator();
  let allSucceeded = true;
  const lines = createInterface({ input: process.stdin, terminal: false });
  lines.on("line", (line) => {
    const trimmed = line.trim();
    if (!accumulator.isPending()) {
      if (trimmed === "") return;
      if (trimmed === ":mode" || trimmed.startsWith(":mode ")) {
        // Mode switches work in piped scripts too — silent (not a result
        // line).
        if (!handleModeCommand(trimmed, calculator, true)) allSucceeded = false;
        return;
      }
    }
    const statement = accumulator.push(line);
    if (statement && !evaluateLine(statement.text, calculator, false, true, 0)) {
      allSucceeded = false;
    }
  });
  lines.on("close", () => {
    const pending = accumulator.pendingText();
    const error = accumulator.finish();
    if (error) {
      report(error, pending, true, 0);
      allSucceeded = false;
    }
    process.exit(allSucceeded ? 0 : 1);
  });
}

/** The interactive REPL on node:readline: `> ` prompt, `… ` continuation
 * while a bracket is open, exit/quit/⌃D to leave, `:mode` switches the
 * dialect. (The native CLIs add tab completion and signature hints on their
 * line editors; readline keeps this one plumbing-thin.) */
function runRepl(calculator: Calculator): void {
  console.log(
    `Anzan・暗算 ${CLI_VERSION} — Soroban's exact calculation language. man name (or manual/help) for docs, :mode switches dialect; exit to leave.`,
  );
  const prompt = "> ";
  let accumulator = new StatementAccumulator();
  const editor = createInterface({ input: process.stdin, output: process.stdout, prompt });
  editor.prompt();
  editor.on("line", (line) => {
    const trimmed = line.trim();
    if (!accumulator.isPending()) {
      if (trimmed === "") {
        editor.prompt();
        return;
      }
      if (trimmed === "exit" || trimmed === "quit") {
        editor.close();
        return;
      }
      if (trimmed === ":mode" || trimmed.startsWith(":mode ")) {
        handleModeCommand(trimmed, calculator, false);
        editor.prompt();
        return;
      }
    }
    const statement = accumulator.push(line);
    if (statement) {
      evaluateLine(statement.text, calculator, true, false, prompt.length);
    }
    editor.setPrompt(accumulator.isPending() ? "… " : prompt);
    editor.prompt();
  });
  editor.on("SIGINT", () => {
    // ⌃C abandons a pending statement.
    console.log("^C");
    accumulator = new StatementAccumulator();
    editor.setPrompt(prompt);
    editor.prompt();
  });
  editor.on("close", () => process.exit(0));
}

function main(): void {
  const args = process.argv.slice(2);
  if (args.some((a) => a === "-h" || a === "--help")) {
    console.log(USAGE);
    return;
  }
  if (args.some((a) => a === "--version")) {
    console.log(CLI_VERSION);
    return;
  }

  const calculator = new Calculator();
  const prettyOutput = process.stdout.isTTY === true;

  // One-shot: every argument is a line — except a `.anzan` argument, which
  // runs as a script file. Expression failures poison the exit code but
  // later arguments still run; a script failure HALTS the whole invocation
  // (its remaining statements didn't run, so later arguments can't trust
  // the session).
  if (args.length > 0) {
    let allSucceeded = true;
    for (const argument of args) {
      if (argument.endsWith(".anzan")) {
        if (!runScriptFile(argument, calculator, prettyOutput)) process.exit(1);
      } else if (!evaluateLine(argument, calculator, prettyOutput, true, 0)) {
        allSucceeded = false;
      }
    }
    process.exit(allSucceeded ? 0 : 1);
  }

  if (process.stdin.isTTY) {
    runRepl(calculator);
  } else {
    runPipe(calculator);
  }
}

main();
