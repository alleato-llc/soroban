/// Walks the AST against an environment. Mutates the environment only via
/// assignment/definition expressions; `ans` updating is the Calculator
/// facade's job.
///
/// The productions are split across sibling files: `Evaluator+Calls.swift`
/// (call resolution, overloads, construction, function-value application),
/// `Evaluator+Namespaces.swift` (namespace registration), `Evaluator+Literals.swift`
/// (arrays/maps/arguments/reductions), `Evaluator+Operators.swift` (binary/
/// comparison operators and subscripting), and `Evaluator+Recursion.swift`
/// (user-function application + the recursion/stack machinery). This file holds
/// the stored resolvers and the big `evaluate` switch.
struct Evaluator {
    let registry: FunctionRegistry
    /// Resolves `A:1`-style references (optionally sheet-qualified —
    /// `Budget!A:1`); nil when no sheet is attached. Cells are scalar:
    /// resolvers speak BigDecimal, the evaluator wraps.
    let resolveCell: ((_ sheet: String?, _ column: String, _ row: Int) throws -> BigDecimal)?
    /// Expands `A:1..B:9` rectangles to their numeric values (empty/text
    /// cells skipped, Excel-style); nil when no sheet is attached.
    let resolveRange: ((_ sheet: String?, _ fromColumn: String, _ fromRow: Int,
                        _ toColumn: String, _ toRow: Int) throws -> [BigDecimal])?
    /// Sheet-scoped λ/𝑖 definitions (cells like `tax(x) = …` / `rate = 0.08`),
    /// resolved against the owning sheet; nil when no sheets are attached.
    /// Scoped names shadow log globals; locals/parameters shadow everything.
    var resolveScopedFunction: ((String) -> UserFunction?)? = nil
    var resolveScopedVariable: ((String) throws -> Value?)? = nil
    /// `'Projected Rate'` named-cell references (optionally sheet-qualified).
    /// Cells are scalar, so this speaks BigDecimal like the cell resolver.
    var resolveName: ((_ sheet: String?, _ name: String) throws -> BigDecimal)? = nil
    /// Sheet-scoped `data` declarations (𝑫 cells), resolved like scoped
    /// functions; nil when no sheets are attached.
    var resolveScopedDataType: ((String) -> DataType?)? = nil
    /// A bare name → a HOST value (`Workbook`, `History`). The host owns the
    /// value; Anzan only navigates it through `.member`/`[…]`/`.method`. The
    /// `inLog` flag (`allowMutation`) lets the host scope a name to the log path
    /// (e.g. `History`) — returning nil in a cell, where the name then degrades
    /// to a text label. nil resolver → no host values.
    var resolveHostValue: ((_ name: String, _ inLog: Bool) -> Value?)? = nil
    /// A free call → a HOST function (`cell(col, row)`, `sheetNames()`), or nil
    /// when the name is not a reflection function. nil resolver → no workbook.
    var resolveHostFunction: ((_ name: String, _ arguments: [Value]) throws -> Value?)? = nil
    /// A free call → a HOST mutation (`updateCell`, `addWorksheet`, …), or nil
    /// when the name is not a mutation. The `inLog` flag (`allowMutation`) lets
    /// the host reject mutations during cell recalc.
    var resolveHostMutation: ((_ name: String, _ arguments: [Value], _ inLog: Bool) throws -> Value?)? = nil
    /// True only on the log path — workbook mutations are allowed here and
    /// rejected (by the host resolver) during cell recalc.
    var allowMutation: Bool = false

    func evaluate(_ expression: Expression,
                  in environment: EvaluationEnvironment,
                  locals: [String: Value] = [:],
                  depth: Int = 0) throws -> Value {
        switch expression {
        case .number(let value):
            return .number(value)

        case .stringLiteral(let text):
            return .string(text)

        case .arrayLiteral(let items):
            return try arrayValue(items, in: environment, locals: locals, depth: depth)

        case .mapLiteral(let entries):
            return try mapValue(entries, in: environment, locals: locals, depth: depth)

        case .index(let baseExpr, let indexExpr):
            let base = try evaluate(baseExpr, in: environment, locals: locals, depth: depth)
            let index = try evaluate(indexExpr, in: environment, locals: locals, depth: depth)
            return try subscriptValue(base, by: index)

        case .member(let baseExpr, let name):
            let base = try evaluate(baseExpr, in: environment, locals: locals, depth: depth)
            switch base {
            case .map:
                guard let value = base.mapValue(forKey: name) else {
                    throw EngineError.domainError(message: "no key '\(name)' in map")
                }
                return value
            case .record(let record):
                guard let value = base.mapValue(forKey: name) else {
                    throw EngineError.domainError(
                        message: "\(record.typeName) has no field '\(name)' — it has "
                            + record.entries.map(\.key).joined(separator: ", "))
                }
                return value
            case .host(let object):
                // Host handles expose their own members (.name, .worksheets).
                guard let value = object.member(name) else {
                    throw EngineError.domainError(
                        message: "\(object.typeName) has no member '.\(name)'")
                }
                return value
            default:
                throw EngineError.domainError(
                    message: ".\(name) needs a map or data value, got \(base.kindName)")
            }

        case .methodCall(let baseExpr, let name, let argumentExprs):
            let base = try evaluate(baseExpr, in: environment, locals: locals, depth: depth)
            guard case .host(let object) = base else {
                throw EngineError.domainError(
                    message: ".\(name)(…) needs a host value, got \(base.kindName)")
            }
            let arguments = try arguments(of: argumentExprs, in: environment,
                                          locals: locals, depth: depth)
            return try object.call(name, arguments)

        case .variable(let name):
            // Parameters shadow sheet definitions, which shadow log globals.
            if let value = locals[name] {
                return value
            }
            // Inside a namespaced member, an unqualified name resolves a sibling
            // first (a sibling constant, function, or type as a value), walking
            // up the nesting chain, before sheet scope and globals.
            for qualified in Self.siblingCandidates(of: name, in: environment.currentNamespace) {
                if let value = environment[qualified] { return value }
                if environment.function(named: qualified) != nil || environment.dataType(named: qualified) != nil {
                    return .function(FunctionValue(kind: .user(name: qualified)))
                }
            }
            if let scoped = try resolveScopedVariable?(name) {
                return scoped
            }
            if let value = environment[name] {
                return value
            }
            // The host reflection handle (`Workbook`, `History`) — after user
            // variables, so a user can still bind the name, before the function
            // fallbacks. `History` resolves only on the log path; in a cell it
            // returns nil here and falls through to the text-label rule.
            if let host = resolveHostValue?(name, allowMutation) {
                return host
            }
            // A bare function name is a function VALUE — `map(double, arr)`.
            if let scoped = resolveScopedFunction?(name) {
                // Cell-defined: carried structurally (it lives in a cell,
                // not the environment, so a name can't re-resolve it).
                return .function(FunctionValue(
                    kind: .lambda(parameters: scoped.parameters.map { $0.name },
                                  body: scoped.body)))
            }
            if environment.function(named: name) != nil {
                return .function(FunctionValue(kind: .user(name: name)))
            }
            // A bare data type name is its constructor as a value —
            // `map(Person, listOfMaps)`. Carried by name; apply re-resolves.
            if resolveScopedDataType?(name) != nil || environment.dataType(named: name) != nil {
                return .function(FunctionValue(kind: .user(name: name)))
            }
            if registry.contains(name: name) {
                return .function(FunctionValue(kind: .builtin(name)))
            }
            // An imported namespace's member as a bare name. A constant resolves
            // to its value; a function/type to a function value (`map(area, …)`
            // after `import Geo`). Last fallback; re-resolves by qualified name.
            if let qualified = environment.importedName(name) {
                if let value = environment[qualified] { return value }
                return .function(FunctionValue(kind: .user(name: qualified)))
            }
            // A qualified builtin as a value — `map(Finance::pmt, …)`.
            if let bare = registry.resolveQualified(name) {
                return .function(FunctionValue(kind: .builtin(bare)))
            }
            throw EngineError.unknownVariable(name: name)

        case .lambda(let parameters, let body):
            // Closure-by-value: whatever locals are visible now ride along.
            return .function(FunctionValue(kind: .lambda(parameters: parameters, body: body),
                                           captures: locals))

        case .cellReference(let sheet, let column, let row):
            guard let resolveCell else {
                throw EngineError.domainError(message: "no sheet available for \(column):\(row)")
            }
            return .number(try resolveCell(sheet, column, row))

        case .nameReference(let sheet, let name):
            guard let resolveName else {
                throw EngineError.domainError(message: "no sheet available for '\(name)'")
            }
            return .number(try resolveName(sheet, name))

        case .cellRange(_, let fromColumn, let fromRow, _, _):
            // Ranges only mean something as a list of arguments.
            throw EngineError.domainError(
                message: "ranges like \(fromColumn):\(fromRow)..… can only be used inside functions, e.g. sum(A:1..A:9)")

        case .unaryMinus(let inner):
            let value = try evaluate(inner, in: environment, locals: locals, depth: depth)
            return .number(try -value.asNumber(for: "-"))

        case .percent(let inner):
            // `3%` → 3 × 0.01, exact (× never rounds). Numeric only, like unary −.
            let value = try evaluate(inner, in: environment, locals: locals, depth: depth)
            return .number(try value.asNumber(for: "%") * BigDecimal(significand: 1, exponent: -2))

        case .binary(let op, let lhsExpr, let rhsExpr):
            let lhs = try evaluate(lhsExpr, in: environment, locals: locals, depth: depth)
            let rhs = try evaluate(rhsExpr, in: environment, locals: locals, depth: depth)
            // Operator overloading: when a record is involved, a user-defined
            // operator whose typed operands match wins; otherwise the built-in
            // (so plain numeric/string math is untouched and pays no lookup).
            if lhs.isRecord || rhs.isRecord,
               let overload = try operatorOverload(op, lhs, rhs, in: environment) {
                return try apply(user: overload, arguments: [lhs, rhs],
                                 captures: [:], in: environment, depth: depth)
            }
            return try apply(op, lhs, rhs)

        case .call(let name, let argumentExprs):
            return try call(name: name,
                            arguments: arguments(of: argumentExprs, in: environment,
                                                 locals: locals, depth: depth),
                            in: environment, locals: locals, depth: depth)

        case .assignment(let name, let valueExpr):
            let value = try evaluate(valueExpr, in: environment, locals: locals, depth: depth)
            environment[name] = value
            return value

        case .comparison(let op, let lhsExpr, let rhsExpr):
            let lhs = try evaluate(lhsExpr, in: environment, locals: locals, depth: depth)
            let rhs = try evaluate(rhsExpr, in: environment, locals: locals, depth: depth)
            return try compare(op, lhs, rhs)

        case .conditional(let conditionExpr, let thenExpr, let elseExpr):
            let condition = try evaluate(conditionExpr, in: environment, locals: locals, depth: depth)
            // Truthiness: nonzero is true. Only the taken branch evaluates.
            let branch = try condition.asNumber(for: "the if() condition").isZero
                ? elseExpr : thenExpr
            return try evaluate(branch, in: environment, locals: locals, depth: depth)

        case .reduction(let operation, let index, let lowerExpr, let upperExpr, let bodyExpr):
            return try reduce(operation, index: index, lowerExpr: lowerExpr,
                              upperExpr: upperExpr, bodyExpr: bodyExpr,
                              in: environment, locals: locals, depth: depth)

        case .helpRequest:
            // Calculator intercepts this in the log; reaching the evaluator
            // means a context with no documentation surface.
            throw EngineError.domainError(message: "man works in the calculation log, not a cell")

        case .functionDefinition(let name, let parameters, let body):
            guard !registry.contains(name: name) else {
                throw EngineError.domainError(
                    message: "'\(name)' is a built-in function and can't be redefined")
            }
            guard environment.dataType(named: name) == nil else {
                throw EngineError.domainError(
                    message: "'\(name)' is a data type — its constructor can't be redefined")
            }
            // Operator overloads (`+(a: Point, b: Point) = …`): exactly two
            // operands and at least one declared data type, so built-in
            // arithmetic on numbers/strings can never be clobbered.
            if BinaryOperator(rawValue: name) != nil {
                guard parameters.count == 2 else {
                    throw EngineError.domainError(message:
                        "an operator overload takes two operands — e.g. +(a: Point, b: Point) = …")
                }
                let involvesDataType = parameters.contains {
                    if case .named = $0.type { return true } else { return false }
                }
                guard involvesDataType else {
                    throw EngineError.domainError(message:
                        "an operator overload must involve a data type — the built-in '\(name)' "
                        + "on numbers/strings can't be redefined")
                }
            }
            environment.define(UserFunction(name: name, parameters: parameters,
                                            body: body, source: ""))
            // The facade reports definitions via EvalOutcome; this value is
            // never displayed.
            return .number(.zero)

        case .dataDefinition(let name, let fields):
            // Constructors live in the call namespace — built-in and
            // function collisions are rejected both ways (redeclaring your
            // OWN type is allowed, like redefining your own function).
            guard !registry.contains(name: name) else {
                throw EngineError.domainError(
                    message: "'\(name)' is a built-in function and can't be a data type")
            }
            guard environment.function(named: name) == nil else {
                throw EngineError.domainError(
                    message: "'\(name)' is already a function — pick a different name")
            }
            environment.define(DataType(name: name, fields: fields, source: ""))
            return .number(.zero)

        case .namespaceDefinition(let name, let members):
            try registerNamespace(name, members: members, in: environment, depth: depth)
            return .number(.zero)

        case .importDirective(let name):
            // Already imported → idempotent no-op (before the conflict check,
            // which would otherwise flag the namespace's own members).
            if environment.importedNamespaces.contains(where: { $0.lowercased() == name.lowercased() }) {
                return .number(.zero)
            }
            // A builtin module (Finance, Stats, …) is already in the global
            // prelude — importing it is a harmless no-op.
            if registry.isModule(name) {
                return .number(.zero)
            }
            let members = environment.memberNames(ofNamespace: name)
            guard !members.isEmpty else {
                throw EngineError.domainError(message: "no namespace '\(name)' to import")
            }
            // Loud conflicts (docs/MODULES.md): an imported member must not
            // collide with a builtin, a global function/type/variable, or
            // another import. Qualify it instead.
            for member in members {
                if registry.contains(name: member)
                    || environment.function(named: member) != nil
                    || environment.dataType(named: member) != nil
                    || environment[member] != nil
                    || environment.importedName(member) != nil {
                    throw EngineError.domainError(message:
                        "importing \(name) would shadow '\(member)' — use \(name)::\(member) instead")
                }
            }
            environment.addImport(name)
            return .number(.zero)
        }
    }

    // The heavyweight cases live OUTSIDE the big switch on purpose: in debug
    // builds the switch's one frame holds every case's locals, and the
    // recursion budget (maxCallDepth × frame size) must fit Swift Testing's
    // small cooperative stacks. Keep new fat cases extracted too.
}
