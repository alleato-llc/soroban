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
  soroban [expression ...]     evaluate arguments in one shared session
  ... | soroban                evaluate each line of stdin
  soroban                      interactive REPL (exit / quit / ⌃D to leave)

options:
  -h, --help                   this help
  --version                    print the CLI version

examples:
  soroban "0.1 + 0.2 == 0.3"             # 1 — exactly, no float drift
  soroban "pmt(0.05/12, 360, 200000)"    # spreadsheet-grade finance
  soroban "x = 3" "x^2 + 1"              # arguments share one session
  soroban "man pmt"                      # built-in documentation
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

// One-shot: every argument is a line; first failure poisons the exit code
// but later arguments still run (their results may not depend on it).
if !arguments.isEmpty {
    var allSucceeded = true
    for line in arguments {
        if !evaluate(line, on: calculator, pretty: prettyOutput,
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
    while true {
        let line: String
        do {
            line = try lineNoise.getLine(prompt: prompt)
            print() // linenoise leaves the cursor on the input line
        } catch LinenoiseError.CTRL_C {
            print("^C")
            continue
        } catch {
            break // EOF (⌃D) or a non-recoverable terminal problem
        }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { continue }
        if trimmed == "exit" || trimmed == "quit" { break }
        if trimmed == ":mode" || trimmed.hasPrefix(":mode ") {
            handleModeCommand(trimmed, on: calculator)
            continue
        }
        lineNoise.addHistory(line)
        try? lineNoise.saveHistory(toFile: historyFile)
        evaluate(line, on: calculator, pretty: true,
                 echoInputOnError: false, caretIndent: prompt.count)
    }
} else {
    // Pipe: keep going on errors (stderr carries them), exit 1 if any failed.
    var allSucceeded = true
    while let line = readLine(strippingNewline: true) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { continue }
        if trimmed == ":mode" || trimmed.hasPrefix(":mode ") {
            // Mode switches work in piped scripts too — silent (not a result line).
            if !handleModeCommand(trimmed, on: calculator, quiet: true) { allSucceeded = false }
            continue
        }
        if !evaluate(line, on: calculator, pretty: false,
                     echoInputOnError: true, caretIndent: 0) {
            allSucceeded = false
        }
    }
    exit(allSucceeded ? 0 : 1)
}
