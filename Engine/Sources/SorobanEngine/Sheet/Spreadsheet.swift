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
    private let calculator: Calculator

    /// Shared with every sheet of a SheetStore: tracks which sheet owns the
    /// formula being evaluated (so unqualified refs resolve correctly) and
    /// detects cycles that span sheets.
    let context: ResolutionContext

    /// For error messages ("circular reference involving Budget!A:1") —
    /// set by SheetStore; nil for a standalone single sheet.
    public internal(set) var displayName: String?

    /// Memo for the current generation; cleared by `recalculate()`.
    private var cache: [CellAddress: CellDisplay] = [:]

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

    // MARK: Evaluation

    public func displayValue(at address: CellAddress) -> CellDisplay {
        if let cached = cache[address] { return cached }

        let key = ResolutionContext.CellKey(sheet: ObjectIdentifier(self), address: address)
        guard !context.resolving.contains(key) else {
            // Don't cache: the "circular reference" report belongs to the
            // cell that closed the loop, not everything on the path.
            let qualified = displayName.map { "\($0)!\(address)" } ?? "\(address)"
            return .error("circular reference involving \(qualified)")
        }
        context.resolving.insert(key)
        defer { context.resolving.remove(key) }

        // While this cell evaluates, unqualified references belong to THIS
        // sheet (not whichever tab the user is looking at), and reads are
        // recorded as dependency edges pointing at this cell.
        context.push(self, evaluating: key)
        defer { context.pop() }

        let display = evaluate(cells[address], at: address)
        cache[address] = display
        return display
    }

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

    /// The dynamic half of classification: static facts (markers, parse
    /// outcome) were settled in `Cell.init`; here the stored AST is evaluated
    /// against the current sheet + variables.
    private func evaluate(_ cell: Cell?, at address: CellAddress) -> CellDisplay {
        guard let cell else { return .empty }

        switch cell.content {
        case .explicitText(let text), .plainText(let text):
            return .text(text)

        case .note(let comment):
            return .note(comment)

        case .definition(let definition):
            // Built-in names stay protected, mirroring the log. Functions
            // and data type constructors share the call namespace; variable
            // definitions don't (a 𝑖 named `abs` shadows nothing).
            switch definition.kind {
            case .function, .dataType:
                if FunctionRegistry.standard.contains(name: definition.name) {
                    return .error("'\(definition.name)' is a built-in function and can't be redefined")
                }
            case .variable:
                break
            }
            // Only the canonical cell (first claim) renders the glyph.
            guard definitions[definition.name.lowercased()]?.address == address else {
                let owner = definitions[definition.name.lowercased()]
                    .map { "\($0.address)" } ?? "another cell"
                return .error("'\(definition.name)' is already defined in \(owner)")
            }
            switch definition.kind {
            case .function(let parameters, _):
                return .definition("λ \(definition.name)(\(parameters.joined(separator: ", ")))")
            case .dataType:
                return .definition("𝑫 \(definition.name)")
            case .variable:
                // A 𝑖 whose body is a control expression draws the control
                // (only after the duplicate/builtin checks above — a shadowed
                // slider must show its error, not a working knob).
                if let control = Control.display(for: cell) {
                    return applyingOverride(to: control, at: address)
                }
                return .definition("𝑖 \(definition.name)")
            }

        case .explicitFormula(.failure(let error)):
            return .error("\(error)")

        case .explicitFormula(.success(let expression)):
            if let control = Control.display(for: cell) { // anonymous =slider(…) etc.
                return applyingOverride(to: control, at: address)
            }
            switch calculator.evaluateFormula(expression) {
            case .success(let value): return display(of: value)
            case .failure(let error): return .error("\(error)")
            }

        case .candidate(let expression):
            if let control = Control.display(for: cell) { // anonymous slider(…) etc.
                return applyingOverride(to: control, at: address)
            }
            switch calculator.evaluateFormula(expression) {
            case .success(let value):
                return display(of: value)

            case .failure(let error) where expression.containsCellReference:
                return .error("\(error)") // cell refs are always formulas

            case .failure(.unknownVariable), .failure(.unknownFunction):
                // Unresolved names mean this is a label
                // ("Q1 revenue" parses as Q1 * revenue).
                return .text(cell.raw)

            case .failure(let error):
                // Anything else (division by zero, sqrt(-1), wrong arity, …)
                // only happens to genuine formulas — surface it.
                return .error("\(error)")
            }
        }
    }

    /// Mid-drag, a slider's preview value replaces the stored literal
    /// (clamped). Other controls commit immediately — no preview state.
    private func applyingOverride(to control: CellDisplay, at address: CellAddress) -> CellDisplay {
        guard case .slider(let info) = control,
              let override = sliderOverrides[address] else { return control }
        return .slider(SliderInfo(name: info.name,
                                  value: min(max(override, info.minimum), info.maximum),
                                  minimum: info.minimum, maximum: info.maximum, step: info.step))
    }

    /// Cells hold scalars: numbers display as values, string results render
    /// as text (so `="Q" + quarter` labels work — and behave like text when
    /// referenced: skipped in ranges, error on direct numeric use). Arrays
    /// and maps don't fit in a cell — aggregate them.
    private func display(of value: Value) -> CellDisplay {
        switch value {
        case .number(let number): return .value(number)
        case .string(let text): return .text(text)
        case .array, .map, .record:
            return .error("a cell can't hold \(value.kindName) — aggregate it (e.g. sum(…)) or reference a field")
        case .function:
            return .error("a cell can't hold a function — call it (e.g. =f(A:1))")
        case .host:
            return .error("a cell can't hold \(value.kindName) — read a field from it (e.g. .value)")
        }
    }

    /// Numeric value of a cell as seen from a referencing formula.
    /// Empty cells are 0 (spreadsheet convention); text and errors propagate.
    public func numericValue(column: String, row: Int) throws -> BigDecimal {
        guard let address = CellAddress(columnName: column, rowNumber: row) else {
            throw EngineError.domainError(message: "cell \(column):\(row) is out of range")
        }
        context.recordCellRead(of: ResolutionContext.CellKey(
            sheet: ObjectIdentifier(self), address: address))

        switch displayValue(at: address) {
        case .empty:
            return .zero
        case .value(let value):
            return value
        case .slider(let info), .stepper(let info):
            return info.value // controls read as their current value
        case .checkbox(let info):
            return info.isOn ? .one : .zero
        case .dropdown(let info):
            guard case .number(let value) = info.value else {
                throw EngineError.domainError(
                    message: "cell \(address) is not a number") // string options act like text
            }
            return value
        case .text, .note:
            throw EngineError.domainError(message: "cell \(address) is not a number")
        case .definition(let glyph):
            throw EngineError.domainError(
                message: "cell \(address) is a definition (\(glyph)) — use the name directly")
        case .error(let message):
            throw EngineError.domainError(message: message)
        }
    }

    /// Values in the rectangle spanned by two corners (any orientation),
    /// row-major. Excel semantics: empty and text cells are skipped — so
    /// avg/count over a sparse column do what you expect — while error
    /// cells propagate as errors.
    public func numericValues(fromColumn: String, fromRow: Int,
                              toColumn: String, toRow: Int) throws -> [BigDecimal] {
        guard let from = CellAddress(columnName: fromColumn, rowNumber: fromRow),
              let to = CellAddress(columnName: toColumn, rowNumber: toRow) else {
            throw EngineError.domainError(
                message: "range \(fromColumn):\(fromRow)..\(toColumn):\(toRow) is out of bounds")
        }

        let rows = min(from.row, to.row)...max(from.row, to.row)
        let columns = min(from.column, to.column)...max(from.column, to.column)
        context.recordRangeRead(sheet: ObjectIdentifier(self), rows: rows, columns: columns)

        var values: [BigDecimal] = []
        for row in rows {
            for column in columns {
                switch displayValue(at: CellAddress(column: column, row: row)) {
                case .value(let value):
                    values.append(value)
                case .slider(let info), .stepper(let info):
                    values.append(info.value)
                case .checkbox(let info):
                    values.append(info.isOn ? .one : .zero)
                case .dropdown(let info):
                    if case .number(let value) = info.value {
                        values.append(value) // string selections skip like text
                    }
                case .empty, .text, .definition, .note: // notes skip like text
                    continue
                case .error(let message):
                    throw EngineError.domainError(message: message)
                }
            }
        }
        return values
    }
}
