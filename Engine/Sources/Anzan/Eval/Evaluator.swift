/// Walks the AST against an environment. Mutates the environment only via
/// assignment/definition expressions; `ans` updating is the Calculator
/// facade's job.
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
            // first (`Bits::TAU`, a sibling function/type as a value), before
            // sheet scope and globals.
            if let ns = environment.currentNamespace, !name.contains("::") {
                let qualified = "\(ns)::\(name)"
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
            // Members register under qualified names (`Bits::BitField`,
            // `Geometry::area`). A data field referencing a sibling TYPE is
            // qualified to match at declaration; a FUNCTION body resolves its
            // siblings unqualified at call time via the home-namespace context
            // (see apply(user:)). 2a-ii: data + function members; constants and
            // nested namespaces are later slices.
            var declaredTypes: Set<String> = []
            for member in members {
                switch member {
                case .dataDefinition(let typeName, _): declaredTypes.insert(typeName.lowercased())
                case .functionDefinition: break
                default:
                    throw EngineError.domainError(message:
                        "namespace \(name) holds data and function declarations (constants and nesting come later)")
                }
            }
            for member in members {
                switch member {
                case .dataDefinition(let typeName, let fields):
                    let qualified = "\(name)::\(typeName)"
                    guard environment.function(named: qualified) == nil else {
                        throw EngineError.domainError(message: "'\(qualified)' is already a function")
                    }
                    let qualifiedFields = fields.map {
                        DataField(name: $0.name, type: $0.type.qualified(namespace: name, siblings: declaredTypes))
                    }
                    environment.define(DataType(name: qualified, fields: qualifiedFields, source: ""))
                case .functionDefinition(let funcName, let parameters, let body):
                    let qualified = "\(name)::\(funcName)"
                    guard environment.dataType(named: qualified) == nil else {
                        throw EngineError.domainError(message: "'\(qualified)' is already a data type")
                    }
                    // Qualify sibling type annotations so dispatch matches the
                    // namespace's qualified instances (`p: Point` → `p: Geo::Point`).
                    let qualifiedParams = parameters.map {
                        Parameter(name: $0.name, type: $0.type?.qualified(namespace: name, siblings: declaredTypes))
                    }
                    environment.define(UserFunction(name: qualified, parameters: qualifiedParams, body: body, source: ""))
                default:
                    break
                }
            }
            return .number(.zero)
        }
    }

    // The heavyweight cases live OUTSIDE the big switch on purpose: in debug
    // builds the switch's one frame holds every case's locals, and the
    // recursion budget (maxCallDepth × frame size) must fit Swift Testing's
    // small cooperative stacks. Keep new fat cases extracted too.

    private func arrayValue(_ items: [Expression], in environment: EvaluationEnvironment,
                            locals: [String: Value], depth: Int) throws -> Value {
        var values: [Value] = []
        values.reserveCapacity(items.count)
        for item in items {
            values.append(try evaluate(item, in: environment, locals: locals, depth: depth))
        }
        return .array(values)
    }

    private func mapValue(_ entries: [MapLiteralEntry], in environment: EvaluationEnvironment,
                          locals: [String: Value], depth: Int) throws -> Value {
        var values: [Value.MapEntry] = []
        values.reserveCapacity(entries.count)
        for entry in entries {
            values.append(Value.MapEntry(
                key: entry.key,
                value: try evaluate(entry.value, in: environment, locals: locals, depth: depth)))
        }
        return .map(values)
    }

    /// Evaluates a call's arguments; ranges expand in place, so
    /// `sum(A:1..A:9, 10)` sees ≤10 numbers. (Internal, not private:
    /// the tail-call walker in Evaluator+Recursion.swift shares it.)
    func arguments(of expressions: [Expression], in environment: EvaluationEnvironment,
                   locals: [String: Value], depth: Int) throws -> [Value] {
        var arguments: [Value] = []
        arguments.reserveCapacity(expressions.count)
        for expr in expressions {
            if case .cellRange(let sheet, let fc, let fr, let tc, let tr) = expr {
                guard let resolveRange else {
                    throw EngineError.domainError(message: "no sheet available for ranges")
                }
                arguments.append(contentsOf: try resolveRange(sheet, fc, fr, tc, tr)
                    .map(Value.number))
            } else {
                arguments.append(try evaluate(expr, in: environment, locals: locals, depth: depth))
            }
        }
        return arguments
    }

    private func reduce(_ operation: ReductionOperation, index: String,
                        lowerExpr: Expression, upperExpr: Expression, bodyExpr: Expression,
                        in environment: EvaluationEnvironment,
                        locals: [String: Value], depth: Int) throws -> Value {
        let symbol = operation.symbol
        let lower = try requireInt(
            evaluate(lowerExpr, in: environment, locals: locals, depth: depth)
                .asNumber(for: "the \(symbol) lower bound"),
            "\(symbol) lower bound")
        let upper = try requireInt(
            evaluate(upperExpr, in: environment, locals: locals, depth: depth)
                .asNumber(for: "the \(symbol) upper bound"),
            "\(symbol) upper bound")
        // Empty range, by convention: ∑ → 0 (additive identity),
        // ∏ → 1 (multiplicative identity).
        let identity: BigDecimal = operation == .sum ? .zero : .one
        guard lower <= upper else { return .number(identity) }

        let (span, overflow) = upper.subtractingReportingOverflow(lower)
        guard !overflow, span < 100_000 else {
            throw EngineError.domainError(message: "\(symbol) spans more than 100,000 terms")
        }

        var total = identity
        var iterationLocals = locals
        for i in lower...upper {
            iterationLocals[index] = .number(BigDecimal(i)) // shadows globals, like a parameter
            let term = try evaluate(bodyExpr, in: environment,
                                    locals: iterationLocals, depth: depth)
                .asNumber(for: "the \(symbol) term")
            total = operation == .sum ? total + term : total * term
        }
        return .number(total)
    }

    /// Built-ins win (collisions are impossible — definitions are blocked
    /// above); then sheet-scoped λ cells (specific scope over general); then
    /// log functions; then data type constructors; then variables/locals
    /// holding a function value (`f = x -> x * 2` then `f(3)`); then error.
    private func call(name: String, arguments: [Value],
                      in environment: EvaluationEnvironment,
                      locals: [String: Value], depth: Int) throws -> Value {
        if registry.contains(name: name) {
            return try registry.call(name: name, arguments: arguments) { fn, args in
                try self.apply(function: fn, arguments: args, in: environment, depth: depth)
            }
        }
        // Inside a namespaced member, an unqualified call resolves a sibling
        // (function or type constructor) first — `Bits::area` calling `width`.
        if let ns = environment.currentNamespace, !name.contains("::") {
            let qualified = "\(ns)::\(name)"
            if let function = environment.function(named: qualified) {
                return try apply(user: function, arguments: arguments,
                                 captures: [:], in: environment, depth: depth)
            }
            if let type = environment.dataType(named: qualified) {
                return try construct(type, arguments: arguments)
            }
        }
        if let scoped = resolveScopedFunction?(name) {
            return try apply(user: scoped, arguments: arguments,
                             captures: [:], in: environment, depth: depth)
        }
        let overloads = environment.overloads(named: name)
        if !overloads.isEmpty {
            let chosen = try selectOverload(name: name, arguments: arguments, from: overloads)
            return try apply(user: chosen, arguments: arguments,
                             captures: [:], in: environment, depth: depth)
        }
        if let type = resolveScopedDataType?(name) ?? environment.dataType(named: name) {
            return try construct(type, arguments: arguments)
        }
        if case .function(let fn) = locals[name] ?? environment[name] {
            return try apply(function: .function(fn), arguments: arguments,
                             in: environment, depth: depth)
        }
        // Host reflection functions (`cell`, `sheetNames`, …) resolve LAST —
        // a user's own `cell(x) = …` shadows them, like any builtin would.
        if let value = try resolveHostFunction?(name, arguments) {
            return value
        }
        // Host mutations (`updateCell`, `addWorksheet`, …) resolve last of all,
        // and the resolver rejects them outside the log (`allowMutation`).
        if let value = try resolveHostMutation?(name, arguments, allowMutation) {
            return value
        }
        throw EngineError.unknownFunction(name: name)
    }

    /// Picks the user-function overload that matches the argument types.
    /// A typed parameter matches only an argument of that type; an untyped
    /// parameter matches anything. Among matching overloads the most specific
    /// (most typed params) wins; a tie is ambiguous. With no typed match, the
    /// untyped catch-all (if any) is used. Operators reach this the same way.
    func selectOverload(name: String, arguments: [Value],
                        from overloads: [UserFunction]) throws -> UserFunction {
        let arityMatch = overloads.filter { $0.parameters.count == arguments.count }
        guard !arityMatch.isEmpty else {
            // No overload of this arity — surface the standard arity error.
            return overloads[0]
        }
        func fits(_ fn: UserFunction) -> Bool {
            zip(fn.parameters, arguments).allSatisfy { param, arg in
                param.type.map { Self.typeMatches(arg, $0) } ?? true
            }
        }
        let fitting = arityMatch.filter(fits)
        let typed = fitting.filter(\.isTyped)
        if !typed.isEmpty {
            let mostSpecific = typed.map { $0.parameters.filter { $0.type != nil }.count }.max()!
            let best = typed.filter { $0.parameters.filter { $0.type != nil }.count == mostSpecific }
            guard best.count == 1 else {
                throw EngineError.domainError(
                    message: "ambiguous call to '\(name)' — more than one overload matches")
            }
            return best[0]
        }
        if let untyped = fitting.first(where: { !$0.isTyped }) {
            return untyped
        }
        // Right arity, but the argument types match no overload.
        let got = arguments.map(\.kindName).joined(separator: ", ")
        throw EngineError.domainError(
            message: "no overload of '\(name)' accepts (\(got))")
    }

    /// Does a runtime value satisfy a parameter's type annotation? Booleans
    /// are numbers in Anzan, so `Boolean` matches a number; a named type
    /// matches a record of that type (case-insensitive, like the call namespace).
    static func typeMatches(_ value: Value, _ type: TypeAnnotation) -> Bool {
        switch (type, value) {
        case (.number, .number), (.boolean, .number): return true
        case (.string, .string): return true
        case (.named(let typeName), .record(let record)):
            return record.typeName.lowercased() == typeName.lowercased()
        default: return false
        }
    }

    /// The user operator overload (`+(a: Point, b: Point) = …`) matching these
    /// operand types, or nil to fall through to the built-in operator. Throws
    /// only when several overloads are equally specific (ambiguous).
    func operatorOverload(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value,
                          in environment: EvaluationEnvironment) throws -> UserFunction? {
        let overloads = environment.overloads(named: op.rawValue)
        guard !overloads.isEmpty else { return nil }
        let args = [lhs, rhs]
        let fitting = overloads.filter { fn in
            fn.parameters.count == 2 && zip(fn.parameters, args).allSatisfy { param, arg in
                param.type.map { Self.typeMatches(arg, $0) } ?? true
            }
        }
        guard !fitting.isEmpty else { return nil } // none match → built-in
        let mostSpecific = fitting.map { $0.parameters.filter { $0.type != nil }.count }.max()!
        let best = fitting.filter { $0.parameters.filter { $0.type != nil }.count == mostSpecific }
        guard best.count == 1 else {
            throw EngineError.domainError(message:
                "ambiguous '\(op.rawValue)' for \(lhs.kindName) and \(rhs.kindName)")
        }
        return best[0]
    }

    /// Instantiates a data type. Exactly one map argument — what the named-
    /// argument syntax desugars to, and literally the from-map form. Every
    /// declared field must be present and type-correct, nothing extra;
    /// fields canonicalize to declaration order. (Internal, not private:
    /// the tail-call walker in Evaluator+Recursion.swift shares it.)
    func construct(_ type: DataType, arguments: [Value]) throws -> Value {
        guard arguments.count == 1, case .map(let provided) = arguments[0] else {
            let example = type.fields.map { "\($0.name): …" }.joined(separator: ", ")
            throw EngineError.domainError(message:
                "\(type.name)(…) takes named fields — \(type.name)(\(example)) — or one map")
        }
        var entries: [Value.MapEntry] = []
        entries.reserveCapacity(type.fields.count)
        for field in type.fields {
            guard let value = provided.first(where: { $0.key == field.name })?.value else {
                throw EngineError.domainError(message:
                    "\(type.name) is missing '\(field.name)' — it needs \(type.fieldList)")
            }
            entries.append(Value.MapEntry(key: field.name,
                                          value: try field.validated(value, in: type.name)))
        }
        let declared = Set(type.fields.map(\.name))
        if let extra = provided.first(where: { !declared.contains($0.key) }) {
            throw EngineError.domainError(message:
                "\(type.name) has no field '\(extra.key)' — it has \(type.fieldList)")
        }
        return .record(Value.RecordValue(
            typeName: type.name, entries: entries,
            booleanFields: Set(type.fields.filter { $0.type == .boolean }.map(\.name))))
    }

    /// Applies a function VALUE — what the higher-order builtins call back
    /// into. Named references re-resolve (so they follow redefinitions).
    func apply(function value: Value, arguments: [Value],
               in environment: EvaluationEnvironment, depth: Int) throws -> Value {
        guard case .function(let fn) = value else {
            throw EngineError.domainError(
                message: "expected a function (a name or x -> …), got \(value.kindName)")
        }
        switch fn.kind {
        case .builtin(let name):
            return try registry.call(name: name, arguments: arguments) { inner, args in
                try self.apply(function: inner, arguments: args, in: environment, depth: depth)
            }
        case .user(let name):
            if let function = environment.function(named: name) {
                return try apply(user: function, arguments: arguments,
                                 captures: [:], in: environment, depth: depth)
            }
            // Constructors travel by name too (`map(Person, maps)`).
            if let type = resolveScopedDataType?(name) ?? environment.dataType(named: name) {
                return try construct(type, arguments: arguments)
            }
            throw EngineError.unknownFunction(name: name)
        case .lambda(let parameters, let body):
            return try apply(user: UserFunction(name: "lambda",
                                                parameters: parameters.map { Parameter(name: $0) },
                                                body: body, source: ""),
                             arguments: arguments, captures: fn.captures,
                             in: environment, depth: depth)
        }
    }

    /// `arr[0]` (0-based), `"abc"[0]`, `m["key"]`.
    private func subscriptValue(_ base: Value, by index: Value) throws -> Value {
        switch base {
        case .array(let items):
            let position = try requireInt(index.asNumber(for: "an array index"), "array index")
            guard items.indices.contains(position) else {
                throw EngineError.domainError(
                    message: "index \(position) is out of range (array has \(items.count) element\(items.count == 1 ? "" : "s"))")
            }
            return items[position]

        case .string(let text):
            let position = try requireInt(index.asNumber(for: "a string index"), "string index")
            guard position >= 0, position < text.count else {
                throw EngineError.domainError(
                    message: "index \(position) is out of range (string has \(text.count) character\(text.count == 1 ? "" : "s"))")
            }
            return .string(String(text[text.index(text.startIndex, offsetBy: position)]))

        case .map, .record:
            guard case .string(let key) = index else {
                throw EngineError.domainError(
                    message: "map keys are strings — e.g. m[\"name\"], got \(index.kindName)")
            }
            guard let value = base.mapValue(forKey: key) else {
                if case .record(let record) = base {
                    throw EngineError.domainError(
                        message: "\(record.typeName) has no field '\(key)' — it has "
                            + record.entries.map(\.key).joined(separator: ", "))
                }
                throw EngineError.domainError(message: "no key '\(key)' in map")
            }
            return value

        case .host(let object):
            // Host handles define their own indexing (Worksheets[0] / ["Budget"]).
            guard let value = object.index(index) else {
                throw EngineError.domainError(
                    message: "\(object.typeName) can't be indexed by \(index.kindName)")
            }
            return value

        case .number, .fixedInt, .fixedDecimal, .function:
            throw EngineError.domainError(message: "\(base.kindName) can't be indexed")
        }
    }

    private func apply(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        // `+` concatenates as soon as either side is a string — "Q" + 1 is "Q1".
        if op == .add, isString(lhs) || isString(rhs) {
            return .string(lhs.displayText + rhs.displayText)
        }
        // Fixed-width integer arithmetic: the mixing matrix + checked overflow
        // (docs/FIXED-WIDTH.md). Numeric (non-fixedInt) operands skip this and
        // take the exact-decimal path below, unchanged.
        if FixedInt.isInvolved(lhs, rhs) {
            return try FixedInt.applyBinary(op, lhs, rhs)
        }
        // Fixed-precision decimal arithmetic — the money-type mixing matrix.
        if FixedDecimal.isInvolved(lhs, rhs) {
            return try FixedDecimal.applyBinary(op, lhs, rhs)
        }
        let a = try lhs.asNumber(for: op.rawValue)
        let b = try rhs.asNumber(for: op.rawValue)
        switch op {
        case .add: return .number(a + b)
        case .subtract: return .number(a - b)
        case .multiply: return .number(a * b)
        case .divide: return .number(try a / b)
        case .modulo: return .number(try a % b)
        case .power: return .number(try Functions.pow(a, b))
        }
    }

    private func isString(_ value: Value) -> Bool {
        if case .string = value { return true }
        return false
    }

    /// `==`/`!=` are deep equality on any values; ordering needs numbers.
    private func compare(_ op: ComparisonOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        switch op {
        case .equal: return .bool(lhs == rhs)
        case .notEqual: return .bool(lhs != rhs)
        case .less, .greater, .lessOrEqual, .greaterOrEqual:
            let a = try lhs.asNumber(for: op.rawValue)
            let b = try rhs.asNumber(for: op.rawValue)
            switch op {
            case .less: return .bool(a < b)
            case .greater: return .bool(a > b)
            case .lessOrEqual: return .bool(a <= b)
            case .greaterOrEqual: return .bool(a >= b)
            case .equal, .notEqual: fatalError("handled above")
            }
        }
    }
}
