import SorobanEngine
import BigInt
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

    // MARK: Binary bit-editor (Programmer mode)

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
    private(set) var binaryDraft: Value?

    /// The previous result the overlay edits (the implied register).
    var ans: Value { calculator.environment.ans }

    /// The bit view the overlay renders: the live draft if editing, else `ans`.
    var binaryView: Result<BinaryView, BinaryView.Unavailable> {
        BinaryView.make(for: binaryDraft ?? ans, preferredWidth: binaryWidth)
    }
    /// True when there are uncommitted bit flips (the commit affordance shows).
    var binaryHasEdits: Bool { binaryDraft != nil }

    /// Flip bit `index` (0 = LSB) of the working value, staging it as a draft
    /// (no log entry — that waits for `commitBinary`).
    func flipBinaryBit(_ index: Int) {
        guard case .success(let view) = binaryView else { return }
        binaryDraft = view.flippingBit(index).value
    }

    /// Insert the current (possibly bit-edited) value into the input line as a
    /// literal — you fold it into an expression and submit when ready, rather
    /// than it landing in the log on its own. A plain integer inserts as a `0b…`
    /// binary literal (you were editing bits); a typed `Int…` inserts its
    /// canonical constructor (which carries the type and sign).
    func useBinaryValue() {
        guard case .success(let view) = binaryView else { return }
        switch view.kind {
        case .plain: insert(value: "0b" + String(view.pattern, radix: 2))
        case .fixed: insert(value: view.value.description)
        }
    }

    /// Reset the grid to `ans`, discarding staged bit edits.
    func cancelBinaryEdits() { binaryDraft = nil }

    // MARK: Binary bit-editor — formats (named bit ranges)

    /// The active bit-field format — a map `{owner: 3, …}` overlaid on the grid
    /// to label bit ranges; nil = raw bits. A presentational lens, not a value
    /// type: presets are built-in, custom formats persist by being SAVED as an
    /// ordinary map variable in the log (see `saveFormat`).
    var activeFormat: Value?

    /// The active format decoded to ordered fields (each with optional per-bit flags).
    var activeLayout: [BinaryView.FieldSpec]? {
        activeFormat.flatMap { BinaryView.layout(from: $0) }
    }

    /// Built-in formats shipped with the app (not language constructs). Flag
    /// fields decode each bit to a meaning (`r-x`); RGB565 is plain numeric.
    static let binaryFormatPresets: [(name: String, format: Value)] = [
        ("Unix permissions", BinaryView.flagFormatMap([
            ("owner", ["r", "w", "x"]), ("group", ["r", "w", "x"]), ("other", ["r", "w", "x"])])),
        ("TCP flags", BinaryView.flagFormatMap([
            ("flags", ["CWR", "ECE", "URG", "ACK", "PSH", "RST", "SYN", "FIN"])])),
        ("RGB565", BinaryView.formatMap([("r", 5), ("g", 6), ("b", 5)])),
    ]

    /// Custom/saved formats persisted in the workbook — any environment variable
    /// that is a map of positive-integer widths reads back as a format.
    var savedFormats: [(name: String, format: Value)] {
        logVariables
            .compactMap { BinaryView.layout(from: $0.value) != nil ? ($0.key, $0.value) : nil }
            .sorted { $0.0 < $1.0 }
    }

    /// The active format as an editable spec string ("owner:3 group:3 other:3").
    var activeFormatSpec: String {
        (activeLayout ?? []).map { "\($0.name):\($0.width)" }.joined(separator: " ")
    }

    /// The display name of the active format for the menu label — a preset/saved
    /// name when it matches one, else "Custom"; nil when no format is active.
    var activeFormatName: String? {
        guard let format = activeFormat else { return nil }
        if let preset = Self.binaryFormatPresets.first(where: { $0.format == format }) { return preset.name }
        if let saved = savedFormats.first(where: { $0.format == format }) { return saved.name }
        return "Custom"
    }

    func applyFormat(_ value: Value?) {
        activeFormat = value.flatMap { BinaryView.layout(from: $0) != nil ? $0 : nil }
        fitWidthToFormat()
    }

    /// Widen a plain register to at least the active format's total, so the
    /// fields aren't clipped by a too-narrow width (a fixed-width int can't grow).
    private func fitWidthToFormat() {
        guard let layout = activeLayout else { return }
        let total = BinaryView.layoutWidth(layout)
        if total > binaryWidth, let fit = BinaryView.editableWidths.first(where: { $0 >= total }) {
            binaryWidth = fit
        }
    }

    /// Parse a custom spec ("owner:3 group:3", space/comma separated) into the
    /// active format; an empty/invalid spec clears it.
    func applyFormatSpec(_ spec: String) {
        var pairs: [(name: String, width: Int)] = []
        for token in spec.split(whereSeparator: { $0 == " " || $0 == "," }) {
            let parts = token.split(separator: ":")
            guard parts.count == 2, let width = Int(parts[1]), width >= 1 else { continue }
            pairs.append((String(parts[0]), width))
        }
        activeFormat = pairs.isEmpty ? nil : BinaryView.formatMap(pairs)
        fitWidthToFormat()
    }

    /// The `Bits` module schema — a typed home for saved formats (docs/MODULES.md
    /// phases 4–5). Emitted once per workbook, before the first saved format, so
    /// formats persist as typed records (`Bits::BitFormat`) rather than loose maps.
    /// A field's `kind` is "numeric", "flags" (per-bit names), or "enum" (value
    /// labels); the unused list is empty.
    static let bitsNamespaceSource =
        "namespace Bits { data BitField { name: String, bits: Number, kind: String, "
        + "flags: [String], values: [String] }; data BitFormat { fields: [BitField] } }"

    /// A layout rendered as a `Bits::BitFormat(...)` constructor call — the typed,
    /// re-parseable form the binary editor saves.
    private static func bitFormatSource(_ layout: [BinaryView.FieldSpec]) -> String {
        func list(_ strings: [String]) -> String {
            strings.map { "\"\($0)\"" }.joined(separator: ", ")
        }
        let fields = layout.map { spec -> String in
            let kind: String, flags: [String], values: [String]
            if let f = spec.flags, !f.isEmpty {
                kind = "flags"; flags = f; values = []
            } else if let v = spec.values, !v.isEmpty {
                kind = "enum"; flags = []; values = v
            } else {
                kind = "numeric"; flags = []; values = []
            }
            return "Bits::BitField(name: \"\(spec.name)\", bits: \(spec.width), "
                + "kind: \"\(kind)\", flags: [\(list(flags))], values: [\(list(values))])"
        }.joined(separator: ", ")
        return "Bits::BitFormat(fields: [\(fields)])"
    }

    /// Defines the `Bits` schema once per workbook (a one-time log line),
    /// preserving any in-progress input. A no-op once `Bits::BitFormat` exists.
    private func ensureBitsSchema() {
        guard calculator.environment.dataType(named: "Bits::BitFormat") == nil else { return }
        let stash = input
        input = Self.bitsNamespaceSource
        submit()
        suppressNextSuggestionRefresh = true
        input = stash
    }

    /// Persists a layout as a typed `name = Bits::BitFormat(...)` log assignment,
    /// so it lives in the workbook and reappears in `savedFormats`; re-points the
    /// active format at the saved record (a map and a record never compare equal,
    /// so the menu would otherwise read "Custom"). Preserves the input line.
    private func persistFormat(_ layout: [BinaryView.FieldSpec], named name: String) {
        let stash = input
        ensureBitsSchema()
        input = "\(name) = \(Self.bitFormatSource(layout))"
        submit()
        if let saved = calculator.environment.userVariables[name] { activeFormat = saved }
        suppressNextSuggestionRefresh = true
        input = stash
    }

    /// Persist the active format under a name (the "Save current…" path).
    func saveFormat(named name: String) {
        guard let layout = activeLayout else { return }
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        persistFormat(layout, named: trimmed)
    }

    // MARK: Binary bit-editor — visual format builder

    /// Apply a freshly-built layout WITHOUT saving it (transient session state,
    /// like a preset). Defines the schema if needed, then evaluates the typed
    /// constructor off the log (`evaluateFormula` never logs or touches `ans`).
    func applyBuiltFormat(_ layout: [BinaryView.FieldSpec]) {
        guard !layout.isEmpty else { return }
        ensureBitsSchema()
        if case .success(let value) = calculator.evaluateFormula(Self.bitFormatSource(layout)) {
            applyFormat(value)
        }
    }

    /// Save a freshly-built layout under a name (persists + applies).
    func saveBuiltFormat(_ layout: [BinaryView.FieldSpec], named name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !layout.isEmpty, !trimmed.isEmpty else { return }
        persistFormat(layout, named: trimmed)
        fitWidthToFormat()
    }

    /// The current binary value decoded into the active format's fields.
    var binaryFields: [BinaryView.Field] {
        guard let layout = activeLayout, case .success(let view) = binaryView else { return [] }
        return view.fields(layout)
    }

    /// Edit a field by value (writes only its bit range, clamped), staging a draft.
    func setBinaryField(_ name: String, to value: BigInt) {
        guard let layout = activeLayout, case .success(let view) = binaryView else { return }
        binaryDraft = view.setting(field: name, to: value, layout: layout).value
    }

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

        // `:mode [normal|programmer|finance]` — switch the input dialect from the
        // log itself (parity with the CLI's :mode and the toggle), not a
        // calculation. The mode change logs its own dim divider via `mode`'s
        // observer.
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
                let display = result.displayDescription
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

    /// Handles a `:mode …` line typed into the log. A valid dialect switches
    /// `mode` (which logs the divider + persists); anything else logs a usage hint.
    private func applyModeCommand(_ line: String) {
        let parts = line.split(separator: " ", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
        guard parts.count == 2, let requested = LanguageMode(rawValue: parts[1].lowercased()) else {
            log.append(HistoryEntry(expression: line, outcome: .error(
                message: "usage: :mode normal | programmer | finance (currently \(mode.displayName))",
                position: nil)))
            logGeneration += 1
            return
        }
        mode = requested
    }

    func clearLog() {
        log.clear() // empties the persisted tape too
        logGeneration += 1
    }

    // MARK: Autocomplete

    /// SpeedCrunch-style continuation: when the field was empty and the user
    /// just typed a leading binary operator, prepend `ans` so `+5` becomes
    /// `ans+5`. Returns true if it rewrote (the caller then skips the normal
    /// suggestion refresh — the rewrite re-enters onChange and handles it).
    /// Only fires on genuine typing from an empty field, never on a programmatic
    /// set (history recall / accept set `suppressNextSuggestionRefresh` first).
    func applyAnsPrefixIfNeeded(old: String, new: String) -> Bool {
        guard !suppressNextSuggestionRefresh,
              old.allSatisfy({ $0 == " " }),
              let rewritten = Calculator.ansPrefixed(new, mode: mode), rewritten != new
        else { return false }
        suppressNextSuggestionRefresh = true // the re-entrant onChange won't pop suggestions
        input = rewritten
        return true
    }

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
