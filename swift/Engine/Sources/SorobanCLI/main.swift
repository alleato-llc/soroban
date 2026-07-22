import Foundation
import LineNoise
import Anzan

/// `soroban` — the engine at the command line. Three modes, chosen by shape:
///
///     soroban "0.1 + 0.2 == 0.3"        one-shot: evaluate each argument
///     echo "sqrt(2)" | soroban          pipe: evaluate each stdin line
///     soroban                           REPL (stdin is a terminal)
///
/// One `Calculator` per invocation, so variables, `ans`, and user functions
/// carry across arguments/lines exactly like the app's log. The engine does
/// all the work — this file is argument plumbing and error presentation.

let cliVersion = "0.1.0"

let usage = """
soroban — Anzan, the exact calculation language (50 significant digits;
+ − × and integer ^ exact). The same engine that powers the Soroban app.

usage:
  soroban [arg ...]            evaluate arguments in one shared session;
                               an argument ending in .anzan runs as a script
                               file (halts at its first error)
  ... | soroban                evaluate stdin, one statement per line — an
                               open ( [ { continues the statement across lines
  soroban                      interactive REPL (exit / quit / ⌃D to leave)

options:
  -h, --help                   this help
  --version                    print the CLI version

examples:
  soroban "0.1 + 0.2 == 0.3"             # 1 — exactly, no float drift
  soroban "pmt(0.05/12, 360, 200000)"    # spreadsheet-grade finance
  soroban "x = 3" "x^2 + 1"              # arguments share one session
  soroban change.anzan                   # run a script file
  soroban lib.anzan "changeFor(0.95)"    # load a script, then evaluate
  soroban "man pmt"                      # built-in documentation

scripts: one statement per line; inside an unclosed ( [ { the statement
continues onto the next line. `#` comments; a `#!/usr/bin/env soroban`
shebang line makes a chmod +x .anzan file directly executable.
"""

func eprint(_ message: String) {
    FileHandle.standardError.write(Data((message + "\n").utf8))
}

/// Error display: the offending line, a caret under the column (when the
/// engine gives one — same offsets the app's log renders), the message.
func report(_ error: EngineError, input: String, echoInput: Bool, caretIndent: Int) {
    if echoInput { eprint(input) }
    if let position = error.position {
        eprint(String(repeating: " ", count: caretIndent + position) + "^")
    }
    eprint("error: \(error.description)")
}

@discardableResult
func evaluate(_ line: String, on calculator: Calculator,
              pretty: Bool, echoInputOnError: Bool, caretIndent: Int) -> Bool {
    // A trailing comment (`5 + 3 # adds`) echoes after the result in pretty
    // mode — display only, kept out of pipe output.
    let trailing = (pretty ? Calculator.trailingComment(in: line) : nil)
        .map { "  # \($0)" } ?? ""
    switch calculator.evaluate(line) {
    case .success(let outcome):
        switch outcome {
        case .value(let value):
            if let block = outcome.rawBlock {
                print(block) // multi-line strings (pretty JSON) print raw
            } else if pretty, case .number(let number) = value,
                      Calculator.usesProgrammerNotation(line),
                      let hex = number.hexText, hex != "0x0" {
                // The line spoke programmer (0x/0b, bit functions) — echo
                // the integer result in hex too. Display only.
                print("= \(value) (\(hex))\(trailing)")
            } else {
                // Echo the clean form — a fixed-width int / decimal prints as its
                // plain number (343353 / 10.50), not its Int32(…)/Decimal(…) form.
                let shown = value.displayDescription
                print(pretty ? "= \(shown)\(trailing)" : shown)
            }
        case .functionDefined(let signature):
            print(pretty ? "λ \(signature)" : signature)
        case .dataDefined(let declaration):
            print(pretty ? "𝑫 \(declaration)" : declaration)
        case .documentation:
            print(outcome.description)
        case .comment(let text):
            // A standalone note: echo it (pretty keeps the # marker).
            print(pretty ? "# \(text)" : text)
        }
        return true
    case .failure(let error):
        report(error, input: line, echoInput: echoInputOnError, caretIndent: caretIndent)
        return false
    }
}

let arguments = Array(CommandLine.arguments.dropFirst())

if arguments.contains("-h") || arguments.contains("--help") {
    print(usage)
    exit(0)
}
if arguments.contains("--version") {
    print(cliVersion)
    exit(0)
}

/// `:mode [normal|programmer|finance]` — show or set the input/display dialect.
/// Programmer mode reads `^` as XOR, `&`/`|` as AND/OR, `<<`/`>>` as shifts, and
/// `%` as modulo (power becomes pow); see docs/MODES.md.
@discardableResult
func handleModeCommand(_ line: String, on calculator: Calculator, quiet: Bool = false) -> Bool {
    let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
    guard parts.count == 2 else {
        if !quiet { print("mode: \(calculator.mode.rawValue) — use :mode normal|programmer|finance") }
        return true
    }
    guard let mode = LanguageMode(rawValue: parts[1].trimmingCharacters(in: .whitespaces).lowercased()) else {
        eprint("unknown mode '\(parts[1])' — use normal, programmer, or finance")
        return false
    }
    calculator.mode = mode
    if !quiet { print("mode: \(mode.rawValue)") }
    return true
}

let calculator = Calculator()
let prettyOutput = isatty(STDOUT_FILENO) == 1

/// Runs a `.anzan` script file: statements split by the engine's accumulator
/// (an open bracket continues a statement across lines), evaluated in the
/// shared session, HALTING at the first error (script semantics — the caret
/// error gains a trailing `at file:line`). Comment-only statements (including
/// a `#!` shebang) are for the file's reader, not the output. Returns false
/// on the first failure — the caller exits 1.
func runScriptFile(_ path: String, on calculator: Calculator, pretty: Bool) -> Bool {
    guard let source = try? String(contentsOfFile: path, encoding: .utf8) else {
        eprint("error: can't read '\(path)'")
        return false
    }
    var accumulator = StatementAccumulator()
    func run(_ statement: StatementAccumulator.Statement) -> Bool {
        let text = statement.text
        if text.hasPrefix("#") { return true } // shebang / comment: not output
        if text == ":mode" || text.hasPrefix(":mode ") {
            return handleModeCommand(text, on: calculator, quiet: true)
        }
        if !evaluate(text, on: calculator, pretty: pretty,
                     echoInputOnError: true, caretIndent: 0) {
            eprint("at \(path):\(statement.line)")
            return false
        }
        return true
    }
    for line in source.split(separator: "\n", omittingEmptySubsequences: false) {
        if let statement = accumulator.push(String(line)), !run(statement) {
            return false
        }
    }
    let pending = accumulator.pendingText
    do {
        try accumulator.finish()
    } catch {
        report(error, input: pending, echoInput: true, caretIndent: 0)
        eprint("at \(path)")
        return false
    }
    return true
}

// One-shot: every argument is a line — except a `.anzan` argument, which runs
// as a script file. Expression failures poison the exit code but later
// arguments still run; a script failure HALTS the whole invocation (its
// remaining statements didn't run, so later arguments can't trust the session).
if !arguments.isEmpty {
    var allSucceeded = true
    for argument in arguments {
        if argument.hasSuffix(".anzan") {
            if !runScriptFile(argument, on: calculator, pretty: prettyOutput) {
                exit(1)
            }
        } else if !evaluate(argument, on: calculator, pretty: prettyOutput,
                            echoInputOnError: true, caretIndent: 0) {
            allSucceeded = false
        }
    }
    exit(allSucceeded ? 0 : 1)
}

if isatty(STDIN_FILENO) == 1 {
    // REPL via LineNoise: ↑/↓ history (persisted), tab completion and gray
    // signature hints — both fed by the engine's own autocomplete/docs.
    let prompt = "> "
    let lineNoise = LineNoise()
    let historyFile = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".soroban_history").path
    try? lineNoise.loadHistory(fromFile: historyFile)

    // Tab: complete the identifier being typed, keep the rest of the line.
    lineNoise.setCompletionCallback { buffer in
        let prefix = Calculator.trailingIdentifier(of: buffer)
        guard !prefix.isEmpty else { return [] }
        let head = String(buffer.dropLast(prefix.count))
        return calculator.completions(forPrefix: prefix).map { head + $0.name }
    }

    // Hints: after `name(`, ghost the rest of the signature from the docs.
    lineNoise.setHintsCallback { buffer in
        guard buffer.hasSuffix("(") else { return (nil, nil) }
        let name = Calculator.trailingIdentifier(of: String(buffer.dropLast()))
        guard !name.isEmpty,
              let doc = calculator.documentation(for: name),
              let open = doc.signature.firstIndex(of: "(")
        else { return (nil, nil) }
        let rest = String(doc.signature[doc.signature.index(after: open)...])
        return (rest, (127, 127, 127))
    }

    print("Anzan・暗算 \(cliVersion) — Soroban's exact calculation language. man name (or manual/help) for docs, tab completes, :mode switches dialect; exit to leave.")
    // An open ( [ { continues the statement onto the next line (`… ` prompt) —
    // so pasting a pretty-formatted block just works. ⌃C abandons a pending
    // statement.
    var accumulator = StatementAccumulator()
    while true {
        let line: String
        do {
            line = try lineNoise.getLine(prompt: accumulator.isPending ? "… " : prompt)
            print() // linenoise leaves the cursor on the input line
        } catch LinenoiseError.CTRL_C {
            print("^C")
            accumulator = StatementAccumulator()
            continue
        } catch {
            break // EOF (⌃D) or a non-recoverable terminal problem
        }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if !accumulator.isPending {
            if trimmed.isEmpty { continue }
            if trimmed == "exit" || trimmed == "quit" { break }
            if trimmed == ":mode" || trimmed.hasPrefix(":mode ") {
                handleModeCommand(trimmed, on: calculator)
                continue
            }
        }
        guard let statement = accumulator.push(line) else { continue }
        lineNoise.addHistory(statement.text) // the joined one-line form recalls
        try? lineNoise.saveHistory(toFile: historyFile)
        evaluate(statement.text, on: calculator, pretty: true,
                 echoInputOnError: false, caretIndent: prompt.count)
    }
} else {
    // Pipe: statement-aware (an open bracket continues onto the next line —
    // streaming, one statement out as soon as it closes), keep going on errors
    // (stderr carries them), exit 1 if any failed.
    var allSucceeded = true
    var accumulator = StatementAccumulator()
    while let line = readLine(strippingNewline: true) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if !accumulator.isPending {
            if trimmed.isEmpty { continue }
            if trimmed == ":mode" || trimmed.hasPrefix(":mode ") {
                // Mode switches work in piped scripts too — silent (not a result line).
                if !handleModeCommand(trimmed, on: calculator, quiet: true) { allSucceeded = false }
                continue
            }
        }
        guard let statement = accumulator.push(line) else { continue }
        if !evaluate(statement.text, on: calculator, pretty: false,
                     echoInputOnError: true, caretIndent: 0) {
            allSucceeded = false
        }
    }
    let pending = accumulator.pendingText
    do {
        try accumulator.finish()
    } catch {
        report(error, input: pending, echoInput: true, caretIndent: 0)
        allSucceeded = false
    }
    exit(allSucceeded ? 0 : 1)
}
