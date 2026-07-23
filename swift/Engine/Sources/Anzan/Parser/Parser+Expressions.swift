// The operator-precedence ladder, loosest to tightest: comparison → lambdas →
// the Programmer-mode bitwise band (| ^ & << >>) → additive → term → unary →
// power → postfix. Lambdas are checked at the top of `comparison`, every
// expression entry point.

extension Parser {
    /// One comparison level: `additive (op additive)?`. Single comparison
    /// only — `a < b < c` is rejected (1/0 chaining is never what you meant).
    /// Lambdas are checked first: every expression entry point comes through
    /// here, so `x -> …` works as an argument, an assignment value, a body…
    mutating func comparison() throws(EngineError) -> Expression {
        if let lambda = try lambdaExpression() {
            return lambda
        }
        let lhs = try bitwiseOr()
        guard let op = comparisonOperator(for: current.kind) else { return lhs }
        _ = advance()
        let rhs = try bitwiseOr()
        if comparisonOperator(for: current.kind) != nil {
            throw EngineError.parseError(
                message: "comparisons can't be chained — use and(a < b, b < c)",
                position: current.position)
        }
        return .comparison(op, lhs, rhs)
    }

    func comparisonOperator(for kind: Token.Kind) -> ComparisonOperator? {
        switch kind {
        case .lessThan: return .less
        case .greaterThan: return .greater
        case .lessOrEqual: return .lessOrEqual
        case .greaterOrEqual: return .greaterOrEqual
        case .equalEqual: return .equal
        case .notEqual: return .notEqual
        default: return nil
        }
    }

    /// Speculatively parses `x -> body` or `(a, b) -> body` (also `() -> body`).
    /// Returns nil — position rewound — when the shape doesn't end in `->`,
    /// so `(a, b)` stays a parenthesized expression and `x` stays a variable.
    mutating func lambdaExpression() throws(EngineError) -> Expression? {
        let start = index

        // x -> body
        if case .identifier(let name) = current.kind,
           index + 1 < tokens.count, case .arrow = tokens[index + 1].kind {
            let position = current.position
            index += 2
            return try lambdaBody(parameters: [name], at: position)
        }

        // (a, b) -> body — only a lambda if the parens hold a plain
        // parameter list AND an arrow follows.
        guard case .leftParen = current.kind else { return nil }
        let position = current.position
        _ = advance()
        var parameters: [String] = []
        scan: while true {
            switch current.kind {
            case .rightParen where parameters.isEmpty: // () -> …
                _ = advance()
                break scan
            case .identifier(let parameter):
                parameters.append(parameter)
                _ = advance()
                if case .comma = current.kind {
                    _ = advance()
                    continue
                }
                guard case .rightParen = current.kind else {
                    index = start
                    return nil
                }
                _ = advance()
                break scan
            default:
                index = start
                return nil
            }
        }
        guard case .arrow = current.kind else {
            index = start
            return nil
        }
        _ = advance()
        return try lambdaBody(parameters: parameters, at: position)
    }

    mutating func lambdaBody(parameters: [String],
                             at position: Int) throws(EngineError) -> Expression {
        guard Set(parameters.map { $0.lowercased() }).count == parameters.count else {
            throw EngineError.parseError(message: "duplicate parameter name", position: position)
        }
        for parameter in parameters where ReservedNames.contains(parameter.lowercased()) {
            throw EngineError.parseError(message: "cannot use '\(parameter)' as a parameter",
                                         position: position)
        }
        return .lambda(parameters: parameters, body: try comparison())
    }

    // MARK: Programmer-mode bitwise band (Python precedence)
    //
    // Loosest-to-tightest: `|` · `^` · `&` · `<< >>`, sitting between comparison
    // and additive (so bitwise binds below arithmetic, above comparison — no
    // C-style `a & b == c` trap). Active only in `.programmer`; in other modes
    // these are pass-throughs, and the glyphs that have no other meaning
    // (`| & << >>`) raise a mode-scoped error rather than a vague "unexpected
    // input". `^` is NOT errored here: in `.normal`/`.scientific` it is power,
    // consumed deeper by power().

    mutating func bitwiseOr() throws(EngineError) -> Expression {
        var lhs = try bitwiseXor()
        while case .pipe = current.kind {
            guard mode == .programmer else { throw modeOperatorError("|", "bitOr", at: current.position) }
            _ = advance()
            lhs = .call(name: "bitOr", arguments: [lhs, try bitwiseXor()])
        }
        return lhs
    }

    mutating func bitwiseXor() throws(EngineError) -> Expression {
        var lhs = try bitwiseAnd()
        while mode == .programmer, case .caret = current.kind {
            _ = advance()
            lhs = .call(name: "bitXor", arguments: [lhs, try bitwiseAnd()])
        }
        return lhs
    }

    mutating func bitwiseAnd() throws(EngineError) -> Expression {
        var lhs = try shift()
        while case .ampersand = current.kind {
            guard mode == .programmer else { throw modeOperatorError("&", "bitAnd", at: current.position) }
            _ = advance()
            lhs = .call(name: "bitAnd", arguments: [lhs, try shift()])
        }
        return lhs
    }

    mutating func shift() throws(EngineError) -> Expression {
        var lhs = try additive()
        while true {
            switch current.kind {
            case .shiftLeft:
                guard mode == .programmer else { throw modeOperatorError("<<", "bitShift", at: current.position) }
                _ = advance()
                lhs = .call(name: "bitShift", arguments: [lhs, try additive()])
            case .shiftRight:
                guard mode == .programmer else { throw modeOperatorError(">>", "bitShift", at: current.position) }
                _ = advance()
                // `a >> n` ≡ bitShift(a, -n) — bitShift shifts right on a negative count.
                lhs = .call(name: "bitShift", arguments: [lhs, .unaryMinus(try additive())])
            default:
                return lhs
            }
        }
    }

    func modeOperatorError(_ glyph: String, _ function: String, at position: Int) -> EngineError {
        .parseError(
            message: "'\(glyph)' is a Programmer-mode operator — use \(function)(…), or switch to Programmer mode",
            position: position)
    }

    mutating func additive() throws(EngineError) -> Expression {
        var lhs = try term()
        while true {
            switch current.kind {
            case .plus:
                _ = advance()
                lhs = .binary(.add, lhs, try term())
            case .minus:
                _ = advance()
                lhs = .binary(.subtract, lhs, try term())
            default:
                return lhs
            }
        }
    }

    mutating func term() throws(EngineError) -> Expression {
        var lhs = try unary()
        while true {
            switch current.kind {
            case .star:
                _ = advance()
                lhs = .binary(.multiply, lhs, try unary())
            case .slash:
                _ = advance()
                lhs = .binary(.divide, lhs, try unary())
            case .percent where mode == .programmer:
                // Programmer mode: `%` is modulo (mod(a, b)), at multiplicative
                // precedence. In other modes `%` is postfix percent (postfix()).
                _ = advance()
                lhs = .call(name: "mod", arguments: [lhs, try unary()])
            case .leftParen, .identifier, .cellReference:
                // Implicit multiplication: `2(3+4)`, `2x`, `(a)(b)`, `2 A:1` —
                // a value against a name, paren, or cell.
                lhs = .binary(.multiply, lhs, try unary())
            case .number:
                // A number directly following another value (`3 4`, `3 % 4`) is
                // almost always a missing operator, not implicit ×. Error toward
                // it instead of silently multiplying.
                throw EngineError.parseError(
                    message: "a number can't directly follow another value — add an operator (e.g. 3 * 4)",
                    position: current.position)
            default:
                return lhs
            }
        }
    }

    mutating func unary() throws(EngineError) -> Expression {
        if case .minus = current.kind {
            _ = advance()
            return .unaryMinus(try unary())
        }
        if case .plus = current.kind { // unary plus is a no-op
            _ = advance()
            return try unary()
        }
        if case .sqrtSign = current.kind { // √x desugars to sqrt(x)
            _ = advance()
            return .call(name: "sqrt", arguments: [try unary()])
        }
        if case .tilde = current.kind { // ~x is bitwise NOT (Programmer mode only)
            guard mode == .programmer else {
                throw modeOperatorError("~", "bitNot", at: current.position)
            }
            _ = advance()
            return .call(name: "bitNot", arguments: [try unary()])
        }
        return try power()
    }

    mutating func power() throws(EngineError) -> Expression {
        let base = try postfix()
        // In Programmer mode `^` is XOR (consumed up the chain by bitwiseXor);
        // only in .normal/.scientific is it power.
        guard mode != .programmer, case .caret = current.kind else { return base }
        _ = advance()
        // Right-associative; the exponent may carry its own unary minus (2^-1).
        return .binary(.power, base, try unary())
    }

    /// Postfix accessors, binding tighter than `^`: `arr[0]`, `m.name`,
    /// chained freely (`people[0].age`, `grid[1][2]`).
    mutating func postfix() throws(EngineError) -> Expression {
        var expr = try primary()
        loop: while true {
            switch current.kind {
            case .leftBracket:
                _ = advance()
                let indexExpr = try comparison()
                guard case .rightBracket = current.kind else {
                    throw EngineError.parseError(message: "expected ']'",
                                                 position: current.position)
                }
                _ = advance()
                expr = .index(base: expr, index: indexExpr)
            case .dot:
                _ = advance()
                guard case .identifier(let name) = current.kind else {
                    throw EngineError.parseError(
                        message: "expected a key name after '.' — e.g. person.age",
                        position: current.position)
                }
                _ = advance()
                if case .leftParen = current.kind {
                    // Method call: base.name(args) — host handles dispatch these.
                    _ = advance()
                    let arguments = try argumentList()
                    expr = .methodCall(base: expr, name: name, arguments: arguments)
                } else {
                    expr = .member(base: expr, name: name)
                }
            case .percent where mode != .programmer:
                // Postfix percent: `3%` → 0.03. In .normal/.scientific `%` is
                // always percent; chains like other postfixes (`A:1%`, `arr[0]%`).
                // In Programmer mode `%` is modulo — left for term() to consume.
                _ = advance()
                expr = .percent(expr)
            case .degree:
                // Postfix degrees: `90°` → radians (× π/180). Mode-agnostic —
                // no mode owns another meaning for `°`.
                _ = advance()
                expr = .degrees(expr)
            default:
                break loop
            }
        }
        return expr
    }
}
