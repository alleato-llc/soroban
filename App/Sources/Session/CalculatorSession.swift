import SorobanEngine
import Observation
import Foundation

/// Drives the log + input UI: evaluates lines, records history, and provides
/// ↑/↓ recall over past inputs (persisted across launches).
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

    var input = ""
    var activeView: MainView = .log

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
    private(set) var environmentGeneration = 0

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
    private(set) var suggestions: [Completion] = []
    private(set) var selectedSuggestion = 0
    /// Set by programmatic input changes (history recall, accept, …) so the
    /// resulting onChange doesn't immediately pop suggestions back open.
    private var suppressNextSuggestionRefresh = false

    /// The grid; shares this session's calculator so variables and `A:1`
    /// references work in both views.
    let sheet: SheetModel

    /// Save/Save As/Open for `.soroban` workbooks (cells + variables).
    let workbook: WorkbookManager

    private let calculator = Calculator()
    private var inputHistory: [String]
    /// Cursor into `inputHistory` while the user is ↑/↓-navigating; nil when
    /// typing fresh input. The in-progress line is stashed in `draft`.
    private var historyCursor: Int?
    private var draft = ""

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

    static let welcomePool: [String] = welcomeCategories.flatMap(\.examples)

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
    }

    // The log tape + its persistence now live in `LogStore` (a UI-free model);
    // this view-model just appends to it and bumps `logGeneration` for the UI.

    /// Evaluates the current input line and appends to the log.
    func submit() {
        let line = input.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty else { return }

        // Variables and functions are part of the workbook — definitions
        // and assignments dirty it.
        let stateBefore = calculator.environment.changeCount

        let outcome: HistoryEntry.Outcome
        var annotation: String?
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
            // Everything else keeps the canonical, recallable "= …" form.
            if let block = result.rawBlock {
                outcome = .info(block)
            } else if case .value(let value) = result, value.containsHost {
                outcome = .info(value.description)
            } else {
                outcome = .value(result.description)
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
                                annotation: annotation, note: note))
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

        // The line may have (re)defined variables/functions that cells use.
        sheet.recalculate()
        if calculator.environment.changeCount != stateBefore {
            workbook.noteContentChanged()
            environmentGeneration += 1 // the inspector re-reads the log half
        }
    }

    func clearLog() {
        log.clear() // empties the persisted tape too
        logGeneration += 1
    }

    // MARK: Autocomplete

    /// Recomputes suggestions for the identifier being typed at the caret
    /// (end of input). Called from the input field's onChange.
    func refreshSuggestions() {
        if suppressNextSuggestionRefresh {
            suppressNextSuggestionRefresh = false
            dismissSuggestions()
            return
        }
        let word = Calculator.trailingIdentifier(of: input)
        suggestions = word.isEmpty ? [] : calculator.completions(forPrefix: word)
        selectedSuggestion = 0
    }

    func dismissSuggestions() {
        suggestions = []
        selectedSuggestion = 0
    }

    /// ↑/↓ within the open suggestion list (wraps around).
    func moveSuggestion(_ delta: Int) {
        guard !suggestions.isEmpty else { return }
        selectedSuggestion = (selectedSuggestion + delta + suggestions.count) % suggestions.count
    }

    /// Replaces the typed prefix with the chosen candidate; functions get
    /// their opening parenthesis for free.
    func acceptSuggestion(_ index: Int? = nil) {
        let chosen = index ?? selectedSuggestion
        guard suggestions.indices.contains(chosen) else { return }
        let completion = suggestions[chosen]

        suppressNextSuggestionRefresh = true
        input.removeLast(Calculator.trailingIdentifier(of: input).count)
        input += completion.name
        if completion.kind == .function {
            input += "("
        }
    }

    /// ↑ — step back through past inputs.
    func recallPrevious() {
        guard !inputHistory.isEmpty else { return }
        if historyCursor == nil {
            draft = input
            historyCursor = inputHistory.count
        }
        guard let cursor = historyCursor, cursor > 0 else { return }
        historyCursor = cursor - 1
        suppressNextSuggestionRefresh = true
        input = inputHistory[cursor - 1]
    }

    /// ↓ — step forward, ending at the stashed draft.
    func recallNext() {
        guard let cursor = historyCursor else { return }
        suppressNextSuggestionRefresh = true
        if cursor >= inputHistory.count - 1 {
            historyCursor = nil
            input = draft
        } else {
            historyCursor = cursor + 1
            input = inputHistory[cursor + 1]
        }
    }

    /// Clicking a log line: expressions replace the input, results append.
    func recall(expression: String) {
        suppressNextSuggestionRefresh = true
        input = expression
        historyCursor = nil
    }

    func insert(value: String) {
        suppressNextSuggestionRefresh = true
        input += input.isEmpty || input.hasSuffix(" ") ? value : " \(value)"
        historyCursor = nil
    }
}
