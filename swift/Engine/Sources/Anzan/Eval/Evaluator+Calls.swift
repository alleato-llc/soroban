// Call resolution and dispatch: free calls (registry → sheet scope → overloads
// → data-type constructors → function-valued variables → host reflection →
// imports → qualified builtins), overload selection, type matching, operator
// overloads, record construction, and applying a function VALUE.

extension Evaluator {
    /// Built-ins win (collisions are impossible — definitions are blocked
    /// above); then sheet-scoped λ cells (specific scope over general); then
    /// log functions; then data type constructors; then variables/locals
    /// holding a function value (`f = x -> x * 2` then `f(3)`); then error.
    func call(name: String, arguments: [Value],
              in environment: EvaluationEnvironment,
              locals: [String: Value], depth: Int) throws -> Value {
        if registry.contains(name: name) {
            return try registry.call(name: name, arguments: arguments) { fn, args in
                try self.apply(function: fn, arguments: args, in: environment, depth: depth)
            }
        }
        // Inside a namespaced member, an unqualified call resolves a sibling
        // (function or type constructor) first — `Bits::area` calling `width` —
        // walking up the nesting chain.
        for qualified in Self.siblingCandidates(of: name, in: environment.currentNamespace) {
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
        // Imported namespaces are the final fallback (a user/host/builtin name
        // always wins); the import conflict check keeps this unambiguous.
        if let qualified = environment.importedName(name) {
            if let function = environment.function(named: qualified) {
                return try apply(user: function, arguments: arguments,
                                 captures: [:], in: environment, depth: depth)
            }
            if let type = environment.dataType(named: qualified) {
                return try construct(type, arguments: arguments)
            }
        }
        // A qualified builtin (`Finance::pmt`) — the bare name is also global.
        if let bare = registry.resolveQualified(name) {
            return try registry.call(name: bare, arguments: arguments) { fn, args in
                try self.apply(function: fn, arguments: args, in: environment, depth: depth)
            }
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
}
