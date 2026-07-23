import SorobanEngine
import BigInt
import Observation
import Foundation
import BinaryEditorKit

/// Drives the log + input UI: evaluates lines, records history, and provides
/// ↑/↓ recall over past inputs (persisted across launches).
///
/// The binary bit-editor surface lives in `CalculatorSession+Binary.swift` and
/// the autocomplete / input-history recall in `CalculatorSession+Autocomplete.swift`;
/// this file holds the stored state, evaluation (`submit`), and the log/view
/// plumbing.
/// Which main view is showing above the input bar.
enum MainView {
    case log, sheet

    mutating func toggle() {
        self = self == .log ? .sheet : .log
    }
}

@Observable
@MainActor
final class CalculatorSession {
    /// The calculation log — a UI-free model owning the tape + persistence +
    /// the `History` reflection seam (`LogSource`), decoupled from this
    /// view-model (mirrors `SheetStore` for the grid).
    let log = LogStore()
    /// The running tape, oldest → newest — proxied from the log model.
    var entries: [HistoryEntry] { log.entries }
    /// Observation bridge for the log (the model isn't `@Observable`, like the
    /// grid's `Spreadsheet`): bumped on append/clear so the log view refreshes.
    private(set) var logGeneration = 0

    /// The log's input/display dialect (docs/MODES.md). Persisted like the theme
    /// and pushed to the engine, so the LOG path parses/echoes under it; cells
    /// stay canonical (`.normal`). New input parses in this mode; existing log
    /// entries are inert records and stay as typed. Switching bumps
    /// `logGeneration` so the input-bar affordance refreshes.
    var mode: LanguageMode = .normal {
        didSet {
            calculator.mode = mode
            UserDefaults.standard.set(mode.rawValue, forKey: Self.modeKey)
            // Record a dim divider in the tape so it's clear when the dialect
            // changed (the affordance icon refreshes via @Observable `mode`).
            // Skipped during the init restore and on no-op sets.
            guard modeLoggingEnabled, mode != oldValue else { return }
            log.append(HistoryEntry(expression: "", outcome: .mode("\(mode.displayName) mode")))
            logGeneration += 1
        }
    }
    private static let modeKey = "languageMode"
    /// False during `init`'s mode restore so launching doesn't log a switch.
    private var modeLoggingEnabled = false

    var input = ""
    var activeView: MainView = .log

    // MARK: Binary bit-editor state (Programmer mode)
    //
    // The behavior (bit flips, formats, the visual builder) lives in
    // `CalculatorSession+Binary.swift`; @Observable requires the stored
    // properties in the class body.

    /// Whether the binary overlay is shown. Only ever visible in Programmer
    /// mode (the view gates on it). Defaults ON the first time (discoverable),
    /// then your choice sticks — the ✕ on the overlay, ⌥⌘B, or the View menu
    /// hide/show it, and it won't force itself back open on each mode switch.
    var binaryEditorShown: Bool = {
        UserDefaults.standard.object(forKey: "binaryEditorShown") as? Bool ?? true
    }() {
        didSet { UserDefaults.standard.set(binaryEditorShown, forKey: "binaryEditorShown") }
    }
    /// Display width for a plain integer in the bit grid (a fixed-width int uses
    /// its own). One of `BinaryView.editableWidths`; persisted.
    var binaryWidth: Int = {
        let stored = UserDefaults.standard.integer(forKey: "binaryWidth")
        return BinaryView.editableWidths.contains(stored) ? stored : 32
    }() {
        didSet { UserDefaults.standard.set(binaryWidth, forKey: "binaryWidth") }
    }
    /// The live, uncommitted value while you click bits — nil means the overlay
    /// tracks `ans`. Cleared on any submit (a new result re-syncs the grid).
    /// (Internal setter — written by `CalculatorSession+Binary.swift`.)
    var binaryDraft: Value?

    /// The active bit-field format — a map `{owner: 3, …}` overlaid on the grid
    /// to label bit ranges; nil = raw bits. A presentational lens, not a value
    /// type: presets are built-in, custom formats persist by being SAVED as an
    /// ordinary map variable in the log (see `saveFormat`).
    var activeFormat: Value?

    /// The previous result the overlay edits (the implied register).
    var ans: Value { calculator.environment.ans }

    // MARK: Function reference

    /// Set when autocomplete's ⌘/ asks for a specific entry; the reference
    /// window consumes it and scrolls there.
    var requestedDocEntry: String?

    func referenceDocumentation() -> [DocCategory] {
        calculator.documentation()
    }

    func documentation(for name: String) -> FunctionDoc? {
        calculator.documentation(for: name)
    }

    /// Switches log ↔ grid; any open autocomplete belongs to the input bar,
    /// which is hidden in grid mode.
    func toggleView() {
        activeView.toggle()
        dismissSuggestions()
    }

    // MARK: Environment inspector

    /// Sidebar visibility (⌥⌘0), persisted across launches.
    var inspectorVisible = UserDefaults.standard.bool(forKey: "inspectorVisible") {
        didSet { UserDefaults.standard.set(inspectorVisible, forKey: "inspectorVisible") }
    }

    /// Sidebar width — drag its leading edge to resize, persisted. Clamped to
    /// the same range the drag enforces so a stale default can't escape it.
    static let inspectorWidthRange: ClosedRange<CGFloat> = 180...480
    var inspectorWidth: CGFloat = {
        let stored = UserDefaults.standard.double(forKey: "inspectorWidth")
        let width = stored > 0 ? CGFloat(stored) : 240
        return min(max(width, inspectorWidthRange.lowerBound), inspectorWidthRange.upperBound)
    }() {
        didSet { UserDefaults.standard.set(Double(inspectorWidth), forKey: "inspectorWidth") }
    }

    /// Observation bridge for the LOG half of the inspector: bumped when a
    /// submission changed variables/functions/data types (the sheet half
    /// rides `sheet.generation`).
    /// (Internal setter — also written by `CalculatorSession+Binary.swift`.)
    var environmentGeneration = 0

    /// The log-defined environment, read-only (the inspector's data source).
    var logVariables: [String: Value] { calculator.environment.userVariables }
    var logFunctions: [String: UserFunction] { calculator.environment.userFunctions }
    var logDataTypes: [String: DataType] { calculator.environment.userDataTypes }

    /// Inspector row click: show the cell that defines a name.
    func jumpTo(sheetNamed name: String, address: CellAddress) {
        if let index = sheet.sheetNames.firstIndex(where: {
            $0.compare(name, options: .caseInsensitive) == .orderedSame
        }) {
            sheet.activateSheet(at: index)
        }
        activeView = .sheet
        sheet.select(address)
    }

    /// Autocomplete candidates for the word being typed (empty → hidden).
    /// (Internal setters — written by `CalculatorSession+Autocomplete.swift`.)
    var suggestions: [Completion] = []
    var selectedSuggestion = 0
    /// Set by programmatic input changes (history recall, accept, …) so the
    /// resulting onChange doesn't immediately pop suggestions back open.
    var suppressNextSuggestionRefresh = false

    /// The grid; shares this session's calculator so variables and `A:1`
    /// references work in both views.
    let sheet: SheetModel

    /// Save/Save As/Open for `.soroban` workbooks (cells + variables).
    let workbook: WorkbookManager

    let calculator = Calculator()
    var inputHistory: [String]
    /// Cursor into `inputHistory` while the user is ↑/↓-navigating; nil when
    /// typing fresh input. The in-progress line is stashed in `draft`.
    var historyCursor: Int?
    var draft = ""

    private static let historyKey = "inputHistory"
    private static let historyLimit = 200

    /// Three example expressions for the log's empty state, chosen once per
    /// launch from `Self.welcomePool` (so toggling views doesn't reshuffle).
    let welcomeExamples: [String]

    /// Real, valid expressions surveying the engine's depth, grouped by
    /// component for the Examples menu. Every entry is CLI-verified; the
    /// empty-state welcome samples three from the flattened `welcomePool`,
    /// and the Examples menu shows them grouped (always reachable, even
    /// once the persisted log hides the welcome).
    static let welcomeCategories: [(name: String, examples: [String])] = [
        // The flagship: data types + recursion + namespaces + finance, one line.
        ("Showcase", [
            "namespace Cash { data Change { quarters: Number, dimes: Number, nickels: Number, pennies: Number }; coins(c, d) = if(c < d, 0, 1 + coins(c - d, d)); makeChange(c) = Change(quarters: coins(c, 25), dimes: coins(mod(c, 25), 10), nickels: coins(mod(mod(c, 25), 10), 5), pennies: coins(mod(mod(mod(c, 25), 10), 5), 1)); changeForDollar(cost) = makeChange((1 - cost) * 100) }",
            "Cash::changeForDollar(0.95)",
        ]),
        ("Higher-order", [
            "map(n -> n * n, filter(x -> mod(x, 2) == 0, seq(1, 20)))",
            "reduce((a, b) -> a * b, seq(1, 10), 1)",
            "sum(map(x -> x^2, seq(1, 10)))",
            "len(filter(x -> x > 5, [3, 7, 2, 9, 5, 11]))",
        ]),
        ("Reductions", [
            "∑_i=1^100(1 / i^2)",
            "∏_i=1^10(i)",
        ]),
        ("Finance", [
            "pmt(0.0425/12, 360, 450000)",
            "round(100000 * (1 + 0.05/12)^(12 * 10), 2)",
            "npv(0.1, -1000, 300, 400, 500, 600)",
            "fv(0.06, 10, -1200)",
            "ipmt(0.05/12, 1, 360, 200000)",
        ]),
        ("Statistics", [
            "stdev(82, 91, 77, 88, 64, 95)",
            "percentile(seq(1, 100), 0.9)",
            "median(seq(1, 99))",
            "forecast(8, 1, 2, 3, 4, 2, 4, 6, 8)",
        ]),
        ("Combinatorics", [
            "fact(52) / (fact(5) * fact(47))",
            "choose(52, 5)",
            "perm(10, 3)",
            "lcm(12, 18)",
        ]),
        ("Structures", [
            "sort([5, 2, 8, 1, 9, 3])",
            "unique([3, 1, 4, 1, 5, 9, 2, 6, 5, 3])",
            "keys({alpha: 1, beta: 2, gamma: 3})",
            "concat([1, 2, 3], [4, 5, 6])",
            "{name: \"Ada\", born: 1815}.born",
        ]),
        ("JSON & data types", [
            "toJson({name: \"Ada\", scores: [91, 88, 95]})",
            #"fromJson("{\"x\": 3, \"y\": 4}")"#,
            "data Point { x: Number, y: Number }",
        ]),
        ("Definitions & logic", [
            "compound(p, r, n) = p * (1 + r)^n",
            "if(gcd(17, 5) == 1, \"coprime\", \"shares a factor\")",
        ]),
        ("Programmer", [
            "0xFF + 0b1010",
            "fromBase(\"FF\", 16)",
            "bitXor(12, 10)",
            "log(2, 1024)",
        ]),
        ("Dates", [
            "edate(today(), 6)",
            "networkdays(today(), today() + 30)",
        ]),
        ("Scientific", [
            "atan2(1, 1) * 4",
            "exp(1)",
        ]),
        ("Simple", [
            "sqrt(3^2 + 4^2)",
            "2 ^ 64",
            "x = 12 * 80.5",
            "ans * 1.0825",
        ]),
    ]

    // Showcase is menu-only — its namespace one-liner is too long for a
    // welcome suggestion button.
    static let welcomePool: [String] = welcomeCategories.dropFirst().flatMap(\.examples)

    /// Picks an example: fills the input bar (switching to the log, where the
    /// bar lives). Used by the welcome trio and the Examples menu.
    func useExample(_ example: String) {
        activeView = .log
        recall(expression: example)
    }

    init() {
        inputHistory = UserDefaults.standard.stringArray(forKey: Self.historyKey) ?? []
        welcomeExamples = Array(Self.welcomePool.shuffled().prefix(3))
        sheet = SheetModel(calculator: calculator)
        workbook = WorkbookManager(sheet: sheet)
        // The log model loaded its own persisted tape; wire it as the source
        // for the `History` reflection API (log-only) — no adapter needed.
        sheet.store.logSource = log
        // Restore the persisted dialect and push it to the engine (a property
        // observer doesn't fire for the initial set in init, so sync explicitly).
        mode = LanguageMode(rawValue: UserDefaults.standard.string(forKey: Self.modeKey) ?? "") ?? .normal
        calculator.mode = mode
        modeLoggingEnabled = true // from here, user mode switches are logged
    }

    // The log tape + its persistence now live in `LogStore` (a UI-free model);
    // this view-model just appends to it and bumps `logGeneration` for the UI.

    /// Evaluates the current input line and appends to the log.
    func submit() {
        let line = input.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty else { return }

        // `:mode [normal|programmer|scientific [eng]]` — switch the input
        // dialect from the log itself (parity with the CLI's :mode and the
        // toggle), not a calculation. The mode change logs its own dim divider
        // via `mode`'s observer.
        if line == ":mode" || line.hasPrefix(":mode ") {
            applyModeCommand(line)
            input = ""; historyCursor = nil; draft = ""
            return
        }

        // Variables and functions are part of the workbook — definitions
        // and assignments dirty it.
        let stateBefore = calculator.environment.changeCount

        let outcome: HistoryEntry.Outcome
        var annotation: String?
        // When a value displays cleaner than it recalls (a fixed-width int /
        // decimal shows its plain number but recalls its typed constructor), the
        // canonical form rides along as the recall override; nil otherwise.
        var recallOverride: String?
        switch calculator.evaluate(line) {
        case .success(.documentation(let doc)):
            outcome = .info(EvalOutcome.documentation(doc).description)
        case .success(.comment(let text)):
            // A standalone note — recorded dim, never a value.
            outcome = .comment(text)
        case .success(let result):
            // Multi-line strings (pretty JSON) read as a raw block, like man()
            // output; a result carrying a reflection handle (`History`,
            // `Workbook`) is ALSO display-only — its `Workbook(…)`/`[LogEntry(…)]`
            // rendering isn't re-parseable, so it must not be recalled or
            // treated as a value (same reason cells reject host results).
            // Everything else keeps the canonical, recallable "= …" form — but
            // shows the clean `displayDescription` (a plain number for a
            // fixed-width int / decimal), recalling the typed constructor.
            if let block = result.rawBlock {
                outcome = .info(block)
            } else if case .value(let value) = result, value.containsHost {
                outcome = .info(value.description)
            } else {
                // Mode-aware echo (the engine's one display seam): scientific
                // mode shows a plain numeric result as 2.46912e5 (or eng);
                // recall still gives the canonical form.
                let display = result.displayDescription(mode: calculator.mode,
                                                        style: calculator.sciStyle)
                outcome = .value(display)
                let canonical = result.description
                if canonical != display { recallOverride = canonical }
            }
            // The programmer hex echo, identical to the CLI's: an integer
            // result of a line that spoke 0x/0b or the bit/base functions
            // gets its hex form alongside. Display-only — never recalled.
            if case .value(.number(let number)) = result,
               Calculator.usesProgrammerNotation(line),
               let hex = number.hexText, hex != "0x0" {
                annotation = "(\(hex))"
            }
        case .failure(let error):
            outcome = .error(message: "\(error)", position: error.position)
        }
        // A trailing comment on a calculation rides alongside the result,
        // dimmed and display-only (kept out of Insert/Copy, like annotation).
        let note = Calculator.trailingComment(in: line)
        log.append(HistoryEntry(expression: line, outcome: outcome,
                                annotation: annotation, note: note,
                                recallOverride: recallOverride))
        logGeneration += 1

        if inputHistory.last != line {
            inputHistory.append(line)
            if inputHistory.count > Self.historyLimit {
                inputHistory.removeFirst(inputHistory.count - Self.historyLimit)
            }
            UserDefaults.standard.set(inputHistory, forKey: Self.historyKey)
        }

        input = ""
        historyCursor = nil
        draft = ""
        binaryDraft = nil // a new result re-syncs the binary overlay to ans

        // The line may have (re)defined variables/functions that cells use.
        sheet.recalculate()
        if calculator.environment.changeCount != stateBefore {
            workbook.noteContentChanged()
            environmentGeneration += 1 // the inspector re-reads the log half
        }
    }

    /// Handles a `:mode …` line typed into the log, through the engine's one
    /// shared parse seam (`Calculator.setMode` — the same errors as the CLI's).
    /// A valid dialect switches `mode` (which logs the divider + persists);
    /// anything else logs the engine's error or a usage hint.
    private func applyModeCommand(_ line: String) {
        let parts = line.split(separator: " ", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
        guard parts.count == 2 else {
            log.append(HistoryEntry(expression: line, outcome: .error(
                message: "usage: :mode normal | programmer | scientific [eng] (currently \(mode.displayName))",
                position: nil)))
            logGeneration += 1
            return
        }
        do {
            try calculator.setMode(parsing: parts[1])
        } catch {
            log.append(HistoryEntry(expression: line, outcome: .error(
                message: error.description, position: nil)))
            logGeneration += 1
            return
        }
        // Route through the observable property (divider + persistence); the
        // seam already set calculator.mode, so this is an idempotent re-set.
        mode = calculator.mode
    }

    func clearLog() {
        log.clear() // empties the persisted tape too
        logGeneration += 1
    }
}
