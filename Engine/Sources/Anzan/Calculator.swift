/// Facade for the Soroban expression engine.
/// Owns the environment (variables, `ans`) and runs the lex → parse → eval pipeline.
public final class Calculator {
    public private(set) var environment = EvaluationEnvironment()

    /// Resolves `A:1`-style cell references against attached sheet(s).
    /// The sheet name is nil for unqualified references — resolve those
    /// against the formula's owning sheet (or the active one, from the log).
    public var cellResolver: ((_ sheet: String?, _ column: String, _ row: Int) throws -> BigDecimal)?

    /// Expands `A:1..B:9` ranges to their numeric values.
    public var rangeResolver: ((_ sheet: String?, _ fromColumn: String, _ fromRow: Int,
                                _ toColumn: String, _ toRow: Int) throws -> [BigDecimal])?

    /// Sheet-scoped definitions (λ/𝑖/𝑫 cells), resolved against the formula's
    /// owning sheet (the active one, from the log). Wired by SheetStore.
    public var scopedFunctionResolver: ((String) -> UserFunction?)?
    public var scopedVariableResolver: ((String) throws -> Value?)?
    public var scopedDataTypeResolver: ((String) -> DataType?)?
    /// Where a name is cell-defined ("Budget!A:3"), for immutability errors.
    public var scopedDefinitionOwner: ((String) -> String?)?
    /// Resolves `'Projected Rate'` named-cell references. Wired by SheetStore.
    public var nameResolver: ((_ sheet: String?, _ name: String) throws -> BigDecimal)?

    /// Resolves a bare name to a HOST value (`Workbook`, `History`). Anzan stays
    /// host-agnostic — the host hands back a `.host(…)` it owns. `inLog` lets the
    /// host scope a name to the log (e.g. `History`, nil in a cell → text label).
    /// Wired by SheetStore; nil in the CLI (no workbook/history).
    public var hostValueResolver: ((_ name: String, _ inLog: Bool) -> Value?)?
    /// Resolves a free call to a host FUNCTION (`cell(col, row)`, `sheetNames()`).
    /// Returns nil when the name isn't a reflection function (fall through to
    /// the normal unknown-function error). Wired by SheetStore.
    public var hostFunctionResolver: ((_ name: String, _ arguments: [Value]) throws -> Value?)?
    /// Resolves a free call to a host MUTATION (`updateCell(…)`, `addWorksheet(…)`).
    /// These CHANGE the workbook, so they run from the log only — `inLog` is
    /// false during cell recalc, and the resolver throws then (recalc must stay
    /// reproducible). Returns nil when the name isn't a mutation. Wired by
    /// SheetStore (direct) and overridden by the app (undoable).
    public var hostMutationResolver: ((_ name: String, _ arguments: [Value], _ inLog: Bool) throws -> Value?)?

    public init() {}

    /// Evaluates one line from the log. On success a value becomes `ans`
    /// (definitions don't). A single leading `=` is tolerated (spreadsheet
    /// muscle memory — pasted cell formulas like `=B:1 * 2` should just work).
    public func evaluate(_ input: String) -> Result<EvalOutcome, EngineError> {
        var line = input.trimmingCharacters(in: .whitespaces)
        if line.hasPrefix("=") {
            line = String(line.dropFirst()).trimmingCharacters(in: .whitespaces)
        }

        // A line that is ONLY a comment is a first-class note — recorded, not
        // a parse error, and it never touches `ans`.
        if let comment = Calculator.standaloneComment(in: line) {
            return .success(.comment(comment))
        }

        let expression: Expression
        do {
            expression = try Parser.parse(line)
        } catch let error as EngineError {
            return .failure(error)
        } catch {
            return .failure(.domainError(message: "\(error)"))
        }

        // Cell-defined names are owned by their cells — the log can't
        // reassign them (single source of truth; edit the cell instead).
        switch expression {
        case .assignment(let name, _), .functionDefinition(let name, _, _),
             .dataDefinition(let name, _):
            if let owner = scopedDefinitionOwner?(name) {
                return .failure(.domainError(
                    message: "'\(name)' is defined in cell \(owner) — edit that cell to change it"))
            }
        default:
            break
        }

        if case .functionDefinition(let name, _, _) = expression {
            return run(expression).map { _ in
                // Keep the original line for workbook serialization — with
                // its trailing # comment, which doubles as documentation.
                environment.setFunctionSource(line, for: name)
                // The just-defined overload is the last appended — report ITS
                // signature (typed dispatch can leave several per name).
                return .functionDefined(
                    signature: environment.overloads(named: name).last?.signature ?? name)
            }
        }

        if case .dataDefinition(let name, _) = expression {
            return run(expression).map { _ in
                // Same source-line persistence contract as functions.
                environment.setDataTypeSource(line, for: name)
                return .dataDefined(declaration: environment.dataType(named: name)?.declaration ?? name)
            }
        }

        if case .helpRequest(let name) = expression {
            guard let doc = documentation(for: name) else {
                return .failure(.domainError(
                    message: "no documentation for '\(name)' — see the Function Reference (⌘/) for everything available"))
            }
            return .success(.documentation(doc))
        }

        return run(expression, allowingMutation: true).map { value in
            environment.ans = value
            return .value(value)
        }
    }

    /// Evaluates a spreadsheet cell formula: identical semantics except `ans`
    /// is left untouched, so grid recalculation never disturbs the log session.
    public func evaluateFormula(_ input: String) -> Result<Value, EngineError> {
        do {
            return evaluateFormula(try Parser.parse(input))
        } catch let error as EngineError {
            return .failure(error)
        } catch {
            return .failure(.domainError(message: "\(error)"))
        }
    }

    /// Same, for an already-parsed expression — the sheet parses each cell
    /// once at commit time and re-evaluates the stored AST per recalc.
    /// Function definitions belong to the log, not cells.
    public func evaluateFormula(_ expression: Expression) -> Result<Value, EngineError> {
        if case .functionDefinition = expression {
            return .failure(.domainError(message: "define functions in the calculation log"))
        }
        if case .dataDefinition(let name, _) = expression {
            // Only reachable via `=data …` — the PLAIN form classifies as a
            // sheet definition (a 𝑫 cell) before evaluation ever sees it.
            return .failure(.domainError(
                message: "drop the leading '=' — a plain 'data \(name) { … }' cell declares a sheet data type"))
        }
        if case .assignment(let name, _) = expression {
            // Only reachable via `=name = value` — the PLAIN form classifies
            // as a sheet definition before evaluation ever sees it.
            return .failure(.domainError(
                message: "drop the leading '=' — a plain '\(name) = …' cell defines a sheet variable"))
        }
        if case .helpRequest = expression {
            return .failure(.domainError(message: "man() works in the calculation log"))
        }
        return run(expression)
    }

    /// `allowingMutation` is true only on the log path — workbook mutations
    /// (`updateCell`, `addWorksheet`, …) are rejected during cell recalc so
    /// recalculation stays reproducible.
    private func run(_ expression: Expression,
                     allowingMutation: Bool = false) -> Result<Value, EngineError> {
        do {
            let evaluator = Evaluator(registry: .standard, resolveCell: cellResolver,
                                      resolveRange: rangeResolver,
                                      resolveScopedFunction: scopedFunctionResolver,
                                      resolveScopedVariable: scopedVariableResolver,
                                      resolveName: nameResolver,
                                      resolveScopedDataType: scopedDataTypeResolver,
                                      resolveHostValue: hostValueResolver,
                                      resolveHostFunction: hostFunctionResolver,
                                      resolveHostMutation: hostMutationResolver,
                                      allowMutation: allowingMutation)
            return .success(try evaluator.evaluate(expression, in: environment))
        } catch let error as EngineError {
            return .failure(error)
        } catch {
            return .failure(.domainError(message: "\(error)"))
        }
    }

    /// All built-in function names (for help/autocomplete).
    public static var functionNames: [String] {
        FunctionRegistry.standard.names
    }

    // MARK: Session restore (the workbook half — restoreSession(from:) —
    // lives in SorobanEngine's Calculator+Workbook.swift, so the LANGUAGE
    // module never learns what a Workbook is)

    /// Rebinds persisted variables: pure literals fold directly (the fast
    /// path every pre-`data` workbook takes); anything else — record
    /// constructor calls — evaluates against the current session, which is
    /// why types/functions must already be restored. Unparseable entries
    /// (hand-edited files) are dropped, matching `parsedVariables`.
    public func restoreVariables(_ variables: [String: String]) {
        var folded: [String: Value] = [:]
        var deferred: [(name: String, text: String)] = []
        for (name, text) in variables {
            if let value = Value(parsing: text) {
                folded[name] = value
            } else {
                deferred.append((name, text))
            }
        }
        environment.replaceUserVariables(folded)
        for entry in deferred.sorted(by: { $0.name < $1.name }) {
            if case .success(let value) = evaluateFormula(entry.text) {
                environment[entry.name] = value
            }
        }
    }
}

/// What one log line produced.
public enum EvalOutcome: Equatable, Sendable, CustomStringConvertible {
    case value(Value)
    case functionDefined(signature: String)
    case dataDefined(declaration: String)
    case documentation(FunctionDoc)
    /// A comment-only line (`# note`): a first-class note, recorded by the
    /// host but never affecting `ans`.
    case comment(String)

    /// Convenience so call sites (and a few hundred tests) can keep writing
    /// `.value(BigDecimal(42))`.
    public static func value(_ number: BigDecimal) -> EvalOutcome {
        .value(Value.number(number))
    }

    public var description: String {
        switch self {
        case .value(let value): return value.description
        case .functionDefined(let signature): return signature
        case .dataDefined(let declaration): return declaration
        case .documentation(let doc):
            var lines = [doc.signature, doc.summary]
            lines.append(contentsOf: doc.examples.map { "  e.g. \($0)" })
            return lines.joined(separator: "\n")
        case .comment(let text): return "# \(text)"
        }
    }

    /// The numeric result, when the line was a calculation (nil for
    /// definitions and non-numeric values).
    public var numericValue: BigDecimal? {
        if case .value(.number(let value)) = self { return value }
        return nil
    }

    /// A MULTI-line string result, raw — pretty JSON and friends. Hosts
    /// render this as a plain block (like man() output) instead of one
    /// canonical line of `\n` escapes; single-line strings keep their
    /// canonical quoting (the log stays re-parseable).
    public var rawBlock: String? {
        if case .value(.string(let text)) = self, text.contains("\n") {
            return text
        }
        return nil
    }
}

// MARK: - Autocomplete

/// One autocomplete candidate for the input bar.
public struct Completion: Hashable, Sendable {
    public enum Kind: Hashable, Sendable {
        case function, variable, constant
    }

    public let name: String
    public let kind: Kind
}

extension Calculator {
    /// Candidates whose name starts with `prefix` (case-insensitive):
    /// the user's variables, the built-in constants, and every function.
    /// A single candidate that already equals the prefix is omitted —
    /// there's nothing left to complete.
    public func completions(forPrefix prefix: String) -> [Completion] {
        guard !prefix.isEmpty else { return [] }
        let needle = prefix.lowercased()

        var matches: [Completion] = []
        for name in environment.userVariables.keys where name.lowercased().hasPrefix(needle) {
            matches.append(Completion(name: name, kind: .variable))
        }
        for function in environment.userFunctions.values
        where function.name.lowercased().hasPrefix(needle) {
            matches.append(Completion(name: function.name, kind: .function))
        }
        // Data type constructors complete like functions (they take "(").
        for type in environment.userDataTypes.values
        where type.name.lowercased().hasPrefix(needle) {
            matches.append(Completion(name: type.name, kind: .function))
        }
        for name in ["ans", "e", "pi", "tau", "true", "false", "Json"]
        where name.lowercased().hasPrefix(needle) {
            matches.append(Completion(name: name, kind: .constant))
        }
        for name in FunctionRegistry.standard.names where name.lowercased().hasPrefix(needle) {
            matches.append(Completion(name: name, kind: .function))
        }
        // Special forms aren't in the registry.
        for special in ["sigma", "if", "man", "help"] where special.hasPrefix(needle) {
            matches.append(Completion(name: special, kind: .function))
        }

        matches.sort { $0.name.lowercased() < $1.name.lowercased() }
        if matches.count == 1, matches[0].name.lowercased() == needle {
            return []
        }
        return matches
    }

    /// True when a formula draft ends "expecting an operand" — after an
    /// operator, open paren, comma, `=`, comparison, or range dots. This is
    /// Excel's point mode test: clicking a cell while it holds should insert
    /// the cell's reference rather than commit the edit.
    public static func expectsOperand(_ draft: String) -> Bool {
        guard let last = draft.trimmingCharacters(in: .whitespaces).last else {
            return false
        }
        return "+-*/%^(,=<>≤≥≠·×÷−√.[{:$".contains(last) // $ starts a pinned ref
    }

    /// The comment text of a line that is ONLY a comment (`# note`), or nil
    /// when the line has code. Used by hosts and the calculator to treat a
    /// comment-only line/cell as a first-class note instead of a parse error.
    public static func standaloneComment(in line: String) -> String? {
        let split = Lexer.splitComment(line)
        guard split.code.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return split.comment
    }

    /// The trailing comment on a line that ALSO has code (`5 + 3 # adds`),
    /// or nil. Hosts show it dimmed beside the result and keep it on the raw.
    public static func trailingComment(in line: String) -> String? {
        let split = Lexer.splitComment(line)
        guard !split.code.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return split.comment
    }

    /// Did this input line speak programmer? (0x/0b literals at a token
    /// boundary, or the base/bit functions.) The CLI uses it to decide when
    /// an integer result deserves a hex echo — display only, never semantics.
    public static func usesProgrammerNotation(_ line: String) -> Bool {
        let lowered = line.lowercased()
        for name in ["bitand", "bitor", "bitxor", "bitshift", "frombase", "tobase"]
        where lowered.contains(name) {
            return true
        }
        let chars = Array(lowered)
        for i in 0..<max(chars.count - 1, 0)
        where chars[i] == "0" && (chars[i + 1] == "x" || chars[i + 1] == "b") {
            // Token boundary: "10x" is implicit multiplication, "a0b" an
            // identifier — only a bare 0x/0b counts.
            if i == 0 || !(chars[i - 1].isLetter || chars[i - 1].isNumber || chars[i - 1] == "_") {
                return true
            }
        }
        return false
    }

    /// The identifier fragment at the end of an input line — the thing
    /// autocomplete should complete and replace.
    public static func trailingIdentifier(of line: String) -> String {
        let chars = Array(line)
        var start = chars.count
        while start > 0,
              chars[start - 1].isLetter || chars[start - 1].isNumber || chars[start - 1] == "_" {
            start -= 1
        }
        // Identifiers can't start with a digit (that'd be a number literal).
        while start < chars.count, chars[start].isNumber {
            start += 1
        }
        return String(chars[start...])
    }
}
