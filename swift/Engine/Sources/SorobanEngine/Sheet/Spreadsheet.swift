import Anzan
import Foundation // case-insensitive compares, whitespace trimming

/// What the grid shows for a cell.
public enum CellDisplay: Equatable, Sendable {
    case empty
    case text(String)
    case value(BigDecimal)
    case error(String)
    /// A sheet-scoped definition — "λ tax(x)" or "𝑖 rate" (user design:
    /// definitions show a glyph, not a value; the editor shows the source).
    case definition(String)
    /// A comment-only cell (`# a note`) — the host renders it dim; it holds
    /// no value (skipped in ranges, errors on direct reference).
    case note(String)
    /// Control expressions: `slider(…)` / `rate = slider(…)` etc. — the grid
    /// draws the control; interaction rewrites the storage literal in place.
    case slider(SliderInfo)
    case stepper(SliderInfo)
    case checkbox(CheckboxInfo)
    case dropdown(DropdownInfo)
}

/// The spreadsheet's calculation model: sparse raw contents plus memoized
/// evaluation with formula auto-detection and cycle detection.
///
/// Explicit markers override auto-detection:
///  - `=…` is always a formula; every failure (even unknown names) is an error
///  - `"…"` is always text, shown without the quotes (`"123"` stays a label)
///
/// Auto-detect rules for everything else:
///  1. blank → empty
///  2. doesn't parse → text
///  3. parses and references a cell → always a formula (errors surface as errors)
///  4. parses without cell refs → formula if it evaluates; on failure the
///     error kind decides: unknown variable/function means it's a label
///     ("Q1 revenue" parses as `Q1 * revenue`), anything else (division by
///     zero, domain error, arity) is a formula mistake and shows the error
///
/// The evaluation half (displayValue, per-cell classification, numeric reads
/// and range reads) lives in `Spreadsheet+Evaluation.swift`.
public final class Spreadsheet {
    public static let columnCount = 26
    public static let rowCount = 1000

    /// Cells, parsed and statically classified at commit time (see `Cell`).
    public private(set) var cells: [CellAddress: Cell] = [:]

    /// Raw contents view — what persistence stores.
    public var raws: [CellAddress: String] {
        cells.mapValues(\.raw)
    }

    /// Shared engine so cell formulas see log variables. Evaluation goes
    /// through `evaluateFormula`, which never updates `ans`.
    let calculator: Calculator

    /// Shared with every sheet of a SheetStore: tracks which sheet owns the
    /// formula being evaluated (so unqualified refs resolve correctly) and
    /// detects cycles that span sheets.
    let context: ResolutionContext

    /// For error messages ("circular reference involving Budget!A:1") —
    /// set by SheetStore; nil for a standalone single sheet.
    public internal(set) var displayName: String?

    /// Memo for the current generation; cleared by `recalculate()`.
    var cache: [CellAddress: CellDisplay] = [:]

    public convenience init(calculator: Calculator) {
        self.init(calculator: calculator, context: ResolutionContext())
    }

    init(calculator: Calculator, context: ResolutionContext) {
        self.calculator = calculator
        self.context = context
        context.register(self)
    }

    // MARK: Editing

    /// Sets (or clears, with nil/blank) a cell's raw content. Only this cell
    /// and the formulas that (transitively) read it are recomputed — across
    /// sheets — via the dependency graph; everything else keeps its memo.
    /// Definition cells are MOSTLY the exception: any formula anywhere may
    /// call a defined name and λ/𝑫 calls leave no graph edges, so touching
    /// one invalidates everything, like a log variable change. The carve-out
    /// is a 𝑖 cell redefining the SAME variable (a slider drag commit, a
    /// stepper click, an edited value expression): `definedValue` records a
    /// read edge per consumer, so its readers are exactly known and the
    /// hammer isn't needed — that's what keeps controls responsive on big
    /// workbooks.
    public func setCell(_ raw: String?, at address: CellAddress) {
        let old = cells[address]
        cells[address] = raw.flatMap(Cell.init(raw:))
        let new = cells[address]
        if Self.isSameVariableRedefinition(old, new) {
            rebuildDefinitions() // refresh the indexed expression
            context.invalidate(ResolutionContext.CellKey(sheet: ObjectIdentifier(self),
                                                         address: address))
        } else if (old?.isDefinition ?? false) || new?.isDefinition == true {
            rebuildDefinitions()
            context.invalidateEverything()
        } else {
            context.invalidate(ResolutionContext.CellKey(sheet: ObjectIdentifier(self),
                                                         address: address))
        }
    }

    /// Both sides define a VARIABLE with the same (case-insensitive) name.
    /// Only 𝑖 qualifies: function/data-type calls have no dependency edges,
    /// and a name change orphans readers the graph can't see.
    private static func isSameVariableRedefinition(_ old: Cell?, _ new: Cell?) -> Bool {
        guard case .definition(let before)? = old?.content,
              case .definition(let after)? = new?.content,
              case .variable = before.kind, case .variable = after.kind else {
            return false
        }
        return before.name.lowercased() == after.name.lowercased()
    }

    public func raw(at address: CellAddress) -> String {
        cells[address]?.raw ?? ""
    }

    /// Replaces all contents (used when loading persisted state).
    public func load(_ contents: [CellAddress: String]) {
        cells = contents.compactMapValues(Cell.init(raw:))
        rebuildDefinitions()
        recalculate()
    }

    /// Drops ALL memoized results, everywhere this sheet's context reaches —
    /// for changes the dependency graph can't see (variables, functions,
    /// sheet renames, workbook loads).
    public func recalculate() {
        context.invalidateEverything()
    }

    func clearMemo(at address: CellAddress) {
        cache.removeValue(forKey: address)
    }

    func clearAllMemo() {
        cache.removeAll(keepingCapacity: true)
    }

    // MARK: Named cells ('Projected Rate' — a name for a LOCATION)

    /// One name per cell, ≤64 chars, unique per sheet (case-insensitive,
    /// like sheet names). Distinct from 𝑖 definitions: a definition names a
    /// VALUE (the cell contains it); a cell name names the cell itself,
    /// whatever it holds.
    public private(set) var cellNames: [CellAddress: String] = [:]

    public static let maxNameLength = 64

    /// Sets (or clears, with nil) a cell's name. Validates; resolution is
    /// affected everywhere, so everything recalculates.
    public func setCellName(_ name: String?, at address: CellAddress) throws {
        guard let name else {
            cellNames.removeValue(forKey: address)
            context.invalidateEverything()
            return
        }
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else {
            throw EngineError.domainError(message: "cell names can't be empty")
        }
        guard trimmed.count <= Self.maxNameLength else {
            throw EngineError.domainError(
                message: "cell names are limited to \(Self.maxNameLength) characters")
        }
        guard !trimmed.contains("'"), !trimmed.contains("!") else {
            throw EngineError.domainError(message: "cell names can't contain ' or !")
        }
        if let existing = self.address(forName: trimmed), existing != address {
            throw EngineError.domainError(
                message: "'\(trimmed)' already names cell \(existing)")
        }
        cellNames[address] = trimmed
        context.invalidateEverything()
    }

    /// Case-insensitive lookup (matching sheet-name semantics).
    public func address(forName name: String) -> CellAddress? {
        cellNames.first {
            $0.value.compare(name, options: .caseInsensitive) == .orderedSame
        }?.key
    }

    /// Replaces all names wholesale (workbook load).
    public func loadCellNames(_ names: [CellAddress: String]) {
        cellNames = names
    }

    /// Resolves `'name'` to the cell's numeric value — dependency edges and
    /// cycle detection ride the ordinary cell-read path.
    func numericValue(forName name: String) throws -> BigDecimal {
        guard let target = address(forName: name) else {
            let qualified = displayName.map { " on \($0)" } ?? ""
            throw EngineError.domainError(message: "no cell named '\(name)'\(qualified)")
        }
        return try numericValue(column: target.columnName, row: target.rowNumber)
    }

    // MARK: Sheet-scoped definitions (λ / 𝑖 cells)

    /// One name claimed by a definition cell on this sheet.
    public struct SheetDefinition {
        public let name: String          // as typed
        public let address: CellAddress
        let definition: Cell.Definition

        /// Which of the three a definition cell is — public so the app's
        /// inspector can categorize without reaching into `Cell.Definition`
        /// (which is package-internal).
        public enum Kind: Sendable { case variable, function, dataType }
        public var kind: Kind {
            switch definition.kind {
            case .variable: return .variable
            case .function: return .function
            case .dataType: return .dataType
            }
        }

        /// "f(x, y)" for a λ cell, the bare name otherwise.
        public var signature: String {
            if case .function(let parameters, _) = definition.kind {
                return "\(name)(\(parameters.joined(separator: ", ")))"
            }
            return name
        }
    }

    /// Name (lowercased — one case-insensitive namespace per sheet) → its
    /// canonical definition. When two cells define the same name, the
    /// earliest address (row, then column) wins; the others display errors.
    public private(set) var definitions: [String: SheetDefinition] = [:]

    /// Re-derives the index from the cells. Definition cells are rare, so a
    /// full scan per definition edit is cheap.
    private func rebuildDefinitions() {
        definitions.removeAll(keepingCapacity: true)
        let defined = cells.compactMap { address, cell -> (CellAddress, Cell.Definition)? in
            guard case .definition(let definition) = cell.content else { return nil }
            return (address, definition)
        }.sorted { ($0.0.row, $0.0.column) < ($1.0.row, $1.0.column) }
        for (address, definition) in defined {
            let key = definition.name.lowercased()
            guard definitions[key] == nil else { continue } // first claim wins
            definitions[key] = SheetDefinition(name: definition.name,
                                               address: address, definition: definition)
        }
    }

    /// A cell-defined function, callable from this sheet's formulas.
    func definedFunction(named name: String) -> UserFunction? {
        guard let entry = definitions[name.lowercased()],
              case .function(let parameters, let body) = entry.definition.kind else {
            return nil
        }
        return UserFunction(name: entry.name,
                            parameters: parameters.map { Parameter(name: $0) },
                            body: body, source: entry.definition.source)
    }

    /// A cell-declared data type (𝑫 cell), constructible from this sheet's
    /// formulas and from the log while this sheet is active.
    func definedDataType(named name: String) -> DataType? {
        guard let entry = definitions[name.lowercased()],
              case .dataType(let fields) = entry.definition.kind else {
            return nil
        }
        return DataType(name: entry.name, fields: fields, source: entry.definition.source)
    }

    /// Guards `rate = rate + 1`-style self-reference during lazy evaluation.
    private var resolvingDefinitions: Set<String> = []

    /// A cell-defined variable's value — evaluated lazily, per lookup, so the
    /// expression may read cells (the reads attribute to the CONSUMING
    /// formula, which keeps the dependency graph correct). Public for the
    /// inspector sidebar (called without a currentKey, so no edge records).
    public func definedValue(named name: String) throws -> Value? {
        let key = name.lowercased()
        guard let entry = definitions[key],
              case .variable(let expression) = entry.definition.kind else {
            return nil
        }
        guard !resolvingDefinitions.contains(key) else {
            throw EngineError.domainError(
                message: "circular definition involving '\(entry.name)'")
        }
        // The consuming formula depends on this definition CELL — record the
        // edge so a same-name redefinition (slider drag, stepper click) can
        // invalidate just the readers instead of every memo everywhere.
        context.recordCellRead(of: ResolutionContext.CellKey(
            sheet: ObjectIdentifier(self), address: entry.address))
        // A mid-drag slider definition resolves to its preview value.
        if let override = sliderOverrides[entry.address],
           SliderInfo.extract(from: expression, name: entry.name) != nil {
            return .number(override)
        }
        resolvingDefinitions.insert(key)
        defer { resolvingDefinitions.remove(key) }
        return try calculator.evaluateFormula(expression).get()
    }

    /// Where a name is defined, for immutability errors — "Budget!A:3".
    public func definitionOwner(named name: String) -> String? {
        guard let entry = definitions[name.lowercased()] else { return nil }
        let prefix = displayName.map { "\($0)!" } ?? ""
        return prefix + "\(entry.address)"
    }

    // MARK: Slider overrides (live drag previews)

    /// Live drag values for slider cells: while a knob is mid-drag the UI
    /// previews here (throttled recalc) and only rewrites the cell's raw on
    /// release — one undo step, one journal entry, like column resizing.
    public var sliderOverrides: [CellAddress: BigDecimal] = [:]

    /// Sets a mid-drag preview value with TARGETED invalidation: only this
    /// cell and its recorded readers drop their memos (the definition-read
    /// edges in `definedValue` make that sound for named sliders; anonymous
    /// ones have ordinary cell edges). Never the full-recalc hammer — drags
    /// must stay cheap on big workbooks.
    public func setSliderOverride(_ value: BigDecimal, at address: CellAddress) {
        sliderOverrides[address] = value
        context.invalidate(ResolutionContext.CellKey(sheet: ObjectIdentifier(self),
                                                     address: address))
    }

    /// Drops a preview (drag released or cancelled), same targeted scope.
    public func clearSliderOverride(at address: CellAddress) {
        guard sliderOverrides.removeValue(forKey: address) != nil else { return }
        context.invalidate(ResolutionContext.CellKey(sheet: ObjectIdentifier(self),
                                                     address: address))
    }
}
