import Anzan

// The evaluation half of the spreadsheet: memoized per-cell display resolution
// (with cross-sheet cycle detection), the dynamic classification of a cell's
// stored AST, and the numeric reads (single cell + range) that referencing
// formulas use.

extension Spreadsheet {
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
        case .fixedInt(let f): return .value(f.decimal) // shows its numeric value
        case .fixedDecimal(let d): return .value(d.value) // value; CellFormat handles currency padding
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
