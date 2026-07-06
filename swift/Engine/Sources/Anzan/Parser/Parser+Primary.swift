// Primaries and the leaf grammar: numbers/identifiers/calls, array & map
// literals, indexed reductions (∑ / ∏), sheet-qualified and namespace-qualified
// references, cell references/ranges, and argument lists (incl. named-argument
// desugaring to a map literal).

extension Parser {
    mutating func primary() throws(EngineError) -> Expression {
        let token = advance()
        switch token.kind {
        case .number(let value):
            return .number(value)

        case .cellReference(let column, let row, _, _):
            return try cellReferenceOrRange(sheet: nil, column: column, row: row)

        case .quotedName(let quoted):
            // 'Q1 Budget'!A:1 — a sheet qualifier when ! follows;
            // otherwise 'Projected Rate' — a NAMED CELL on the owning sheet.
            if isBang(current.kind) {
                return try qualifiedReference(sheet: quoted, at: token.position)
            }
            return .nameReference(sheet: nil, name: quoted)

        case .identifier(let name) where isBang(current.kind):
            // Budget!A:1 — unquoted sheet qualifier.
            return try qualifiedReference(sheet: name, at: token.position)

        case .identifier(let name) where isColonColon(current.kind):
            // Bits::BitFormat — namespace-qualified reference / constructor call.
            return try qualifiedName(namespace: name, position: token.position)

        case .identifier(let name):
            // ∑/∏: a plain call is the variadic function (sum/product); the
            // subscript form is the indexed reduction. Typed `sigma_i` arrives
            // as one identifier; after the ∑/∏ symbol, `_i` arrives separately.
            let lowered = name.lowercased()
            for (keyword, operation) in [("sigma", ReductionOperation.sum),
                                         ("product", .product)] {
                if lowered == keyword {
                    if case .identifier(let subscriptName) = current.kind,
                       subscriptName.hasPrefix("_") {
                        _ = advance()
                        return try mathReduction(operation,
                                                 index: String(subscriptName.dropFirst()),
                                                 position: token.position)
                    }
                } else if lowered.hasPrefix(keyword + "_") {
                    return try mathReduction(operation,
                                             index: String(name.dropFirst(keyword.count + 1)),
                                             position: token.position)
                }
            }

            // man NAME / manual NAME / help NAME: unix-style — the argument is a
            // NAME (never evaluated), space-separated, NO parentheses.
            if lowered == "man" || lowered == "manual" || lowered == "help" {
                if case .leftParen = current.kind {
                    throw EngineError.parseError(
                        message: "use `\(name) name` — e.g. \(name) pmt (no parentheses)",
                        position: current.position)
                }
                guard case .identifier(let subject) = current.kind else {
                    throw EngineError.parseError(
                        message: "\(name) needs a function name — e.g. \(name) pmt",
                        position: current.position)
                }
                _ = advance()
                return .helpRequest(name: subject)
            }

            guard case .leftParen = current.kind else {
                return .variable(name)
            }
            _ = advance()
            let arguments = try argumentList()
            if lowered == "sigma" {
                return .call(name: "sum", arguments: arguments) // ∑(1,2,3) = 6
            }
            if lowered == "if" {
                // Special form: branches stay lazy (the untaken one may
                // divide by zero or recurse).
                guard arguments.count == 3 else {
                    throw EngineError.parseError(
                        message: "if expects (condition, then, else)",
                        position: token.position)
                }
                return .conditional(condition: arguments[0],
                                    then: arguments[1], else: arguments[2])
            }
            return .call(name: name, arguments: arguments) // ∏(…) hits product()

        case .leftParen:
            let inner = try comparison()
            guard case .rightParen = current.kind else {
                throw EngineError.parseError(message: "expected ')'", position: current.position)
            }
            _ = advance()
            return inner

        case .string(let text):
            return .stringLiteral(text)

        case .leftBracket:
            return try arrayLiteral()

        case .leftBrace:
            return try mapLiteral(at: token.position)

        case .end:
            throw EngineError.parseError(message: "unexpected end of expression", position: token.position)

        default:
            throw EngineError.parseError(message: "unexpected token", position: token.position)
        }
    }

    /// `[1, 2, 3]` after the consumed '[' — elements are full expressions.
    mutating func arrayLiteral() throws(EngineError) -> Expression {
        if case .rightBracket = current.kind {
            _ = advance()
            return .arrayLiteral([])
        }
        var items = [try comparison()]
        while case .comma = current.kind {
            _ = advance()
            items.append(try comparison())
        }
        guard case .rightBracket = current.kind else {
            throw EngineError.parseError(message: "expected ']' or ','", position: current.position)
        }
        _ = advance()
        return .arrayLiteral(items)
    }

    /// `{name: "Ada", age: 36}` after the consumed '{'. Keys are identifiers
    /// or string literals. One lexing wrinkle: a compact single-letter key
    /// with a number value (`{b:1}`) arrives as a cell-reference TOKEN —
    /// in key position it decomposes back into key + number value.
    mutating func mapLiteral(at position: Int) throws(EngineError) -> Expression {
        var entries: [MapLiteralEntry] = []

        func append(_ key: String, _ value: Expression) throws(EngineError) {
            guard !entries.contains(where: { $0.key == key }) else {
                throw EngineError.parseError(message: "duplicate key '\(key)'", position: position)
            }
            entries.append(MapLiteralEntry(key: key, value: value))
        }

        if case .rightBrace = current.kind {
            _ = advance()
            return .mapLiteral([])
        }
        scan: while true {
            switch current.kind {
            case .cellReference(let column, let row, _, _):
                // {b:1} — the lexer saw a cell reference; here it's a key
                // and its numeric value.
                _ = advance()
                try append(column, .number(BigDecimal(row)))

            case .identifier(let key), .string(let key):
                _ = advance()
                guard case .colon = current.kind else {
                    throw EngineError.parseError(
                        message: "expected ':' after key '\(key)'", position: current.position)
                }
                _ = advance()
                try append(key, try comparison())

            default:
                throw EngineError.parseError(
                    message: "expected a key — e.g. {name: \"Ada\", age: 36}",
                    position: current.position)
            }

            switch current.kind {
            case .comma:
                _ = advance()
            case .rightBrace:
                break scan
            default:
                throw EngineError.parseError(message: "expected '}' or ','",
                                             position: current.position)
            }
        }
        _ = advance() // '}'
        return .mapLiteral(entries)
    }

    /// Indexed reduction in math notation: `∑_i=1^10(i^2)` / `∏_i=1^5(i)`
    /// (typeable as `sigma_i=…` / `product_i=…`). Special forms — the
    /// parenthesized term is NOT evaluated eagerly; it re-evaluates per
    /// index value.
    ///
    /// Bounds are signed primaries (number, variable, cell ref, or
    /// parenthesized expression): that's what keeps the `^` separator
    /// unambiguous with exponentiation. Compound bounds need parentheses —
    /// the plaintext equivalent of LaTeX braces.
    mutating func mathReduction(_ operation: ReductionOperation,
                                index indexName: String,
                                position: Int) throws(EngineError) -> Expression {
        let symbol = operation.symbol
        guard !indexName.isEmpty, indexName.allSatisfy(\.isLetter) else {
            throw EngineError.parseError(
                message: "the \(symbol) index must be a plain variable name (e.g. \(symbol)_i=1^10(i))",
                position: position)
        }
        guard !ReservedNames.contains(indexName.lowercased()) else {
            throw EngineError.parseError(
                message: "cannot use '\(indexName)' as the \(symbol) index", position: position)
        }

        guard case .assign = current.kind else {
            throw EngineError.parseError(
                message: "expected '=' after the \(symbol) index — e.g. \(symbol)_i=1^10(i)",
                position: current.position)
        }
        _ = advance()
        let lower = try signedPrimary()

        guard case .caret = current.kind else {
            throw EngineError.parseError(
                message: "expected '^' before the \(symbol) upper bound — parenthesize compound bounds, e.g. \(symbol)_i=(n-1)^10(i)",
                position: current.position)
        }
        _ = advance()
        let upper = try signedPrimary()

        guard case .leftParen = current.kind else {
            throw EngineError.parseError(
                message: "the \(symbol) term must be parenthesized — e.g. \(symbol)_i=1^10(i)",
                position: current.position)
        }
        _ = advance()
        let body = try comparison()
        guard case .rightParen = current.kind else {
            throw EngineError.parseError(message: "expected ')'", position: current.position)
        }
        _ = advance()

        return .reduction(operation: operation, index: indexName,
                          lower: lower, upper: upper, body: body)
    }

    /// A ∑ bound: optional minus + bound primary. Deliberately not `unary()`
    /// (which falls into `power()` and would consume the `^` bound separator)
    /// and not `primary()` (which would treat `n(` as a call, swallowing the
    /// term's parentheses in `∑_i=1^n(i)`).
    mutating func signedPrimary() throws(EngineError) -> Expression {
        if case .minus = current.kind {
            _ = advance()
            return .unaryMinus(try boundPrimary())
        }
        return try boundPrimary()
    }

    mutating func boundPrimary() throws(EngineError) -> Expression {
        let token = advance()
        switch token.kind {
        case .number(let value):
            return .number(value)
        case .cellReference(let column, let row, _, _):
            return .cellReference(sheet: nil, column: column, row: row)
        case .identifier(let name):
            return .variable(name) // never a call — a following '(' is the term
        case .leftParen:
            let inner = try comparison()
            guard case .rightParen = current.kind else {
                throw EngineError.parseError(message: "expected ')'", position: current.position)
            }
            _ = advance()
            return inner
        default:
            throw EngineError.parseError(
                message: "expected a ∑ bound (number, variable, or parenthesized expression)",
                position: token.position)
        }
    }

    func isBang(_ kind: Token.Kind) -> Bool {
        if case .bang = kind { return true }
        return false
    }

    func isColonColon(_ kind: Token.Kind) -> Bool {
        if case .colonColon = kind { return true }
        return false
    }

    /// `Bits::BitFormat`, `A::B::c` — a namespace-qualified reference (nesting
    /// chains `::`); with `(` it's a qualified call (the constructor of a
    /// namespaced type). The whole qualified name flows as one string
    /// ("A::B::c") that the evaluator resolves.
    mutating func qualifiedName(namespace: String, position: Int) throws(EngineError) -> Expression {
        var qualified = namespace
        repeat {
            _ = advance() // '::'
            guard case .identifier(let member) = current.kind else {
                throw EngineError.parseError(
                    message: "expected a name after '::' — e.g. \(namespace)::Point", position: current.position)
            }
            _ = advance()
            qualified += "::\(member)"
        } while isColonColon(current.kind)
        guard case .leftParen = current.kind else {
            return .variable(qualified)
        }
        _ = advance()
        return .call(name: qualified, arguments: try argumentList())
    }

    /// After a sheet name: `!` then a cell reference, range, or 'named cell'.
    mutating func qualifiedReference(sheet: String,
                                     at position: Int) throws(EngineError) -> Expression {
        guard isBang(current.kind) else {
            throw EngineError.parseError(
                message: "expected '!' after sheet name '\(sheet)' — e.g. '\(sheet)'!A:1",
                position: current.position)
        }
        _ = advance()
        switch current.kind {
        case .cellReference(let column, let row, _, _):
            _ = advance()
            return try cellReferenceOrRange(sheet: sheet, column: column, row: row)
        case .quotedName(let name): // Budget!'Projected Rate'
            _ = advance()
            return .nameReference(sheet: sheet, name: name)
        default:
            throw EngineError.parseError(
                message: "expected a cell or 'named cell' after '\(sheet)!' — e.g. \(sheet)!A:1",
                position: current.position)
        }
    }

    /// A (possibly qualified) cell, optionally extended to a range by `..`.
    mutating func cellReferenceOrRange(sheet: String?, column: String,
                                       row: Int) throws(EngineError) -> Expression {
        guard case .dotDot = current.kind else {
            return .cellReference(sheet: sheet, column: column, row: row)
        }
        _ = advance()
        guard case .cellReference(let toColumn, let toRow, _, _) = current.kind else {
            throw EngineError.parseError(
                message: "expected a cell after '..' (e.g. A:1..A:9)",
                position: current.position)
        }
        _ = advance()
        return .cellRange(sheet: sheet, fromColumn: column, fromRow: row,
                          toColumn: toColumn, toRow: toRow)
    }

    /// Arguments after a consumed '(' — empty list allowed (`pi()` style not
    /// required, but `rand()` future-proofing is free).
    mutating func argumentList() throws(EngineError) -> [Expression] {
        if case .rightParen = current.kind {
            _ = advance()
            return []
        }
        // Named arguments — Person(name: "Ada", age: 36) — desugar to ONE map
        // literal, which makes them exactly the from-map constructor form.
        if isNamedArgumentStart() {
            return [try namedArguments()]
        }
        var arguments = [try comparison()]
        while case .comma = current.kind {
            _ = advance()
            arguments.append(try comparison())
        }
        guard case .rightParen = current.kind else {
            throw EngineError.parseError(message: "expected ')' or ','", position: current.position)
        }
        _ = advance()
        return arguments
    }

    /// Does this argument list open with `name: value`? Two shapes commit:
    /// an identifier directly followed by ':', or a compact `age:36` that the
    /// lexer fused into a cell-reference token with a MULTI-letter column —
    /// real columns are single letters, so that can't be a cell. A compact
    /// single-letter `f(a:1)` stays a cell reference; write `f(a: 1)`.
    func isNamedArgumentStart() -> Bool {
        if case .identifier = current.kind,
           index + 1 < tokens.count, case .colon = tokens[index + 1].kind {
            return true
        }
        if case .cellReference(let column, _, _, _) = current.kind, column.count > 1 {
            return true
        }
        return false
    }

    /// `name: "Ada", age: 36)` after the consumed '(' — consumes the ')'.
    /// Same lexing wrinkle as map literals: a compact `key:number` arrives
    /// as one cell-reference token and decomposes back into key + value.
    mutating func namedArguments() throws(EngineError) -> Expression {
        let position = current.position
        var entries: [MapLiteralEntry] = []

        func append(_ key: String, _ value: Expression) throws(EngineError) {
            guard !entries.contains(where: { $0.key == key }) else {
                throw EngineError.parseError(message: "duplicate field '\(key)'", position: position)
            }
            entries.append(MapLiteralEntry(key: key, value: value))
        }

        scan: while true {
            switch current.kind {
            case .cellReference(let column, let row, _, _):
                _ = advance()
                try append(column, .number(BigDecimal(row)))

            case .identifier(let key):
                _ = advance()
                guard case .colon = current.kind else {
                    throw EngineError.parseError(
                        message: "expected ':' after '\(key)' — named arguments are name: value",
                        position: current.position)
                }
                _ = advance()
                try append(key, try comparison())

            default:
                throw EngineError.parseError(
                    message: "expected another name: value argument",
                    position: current.position)
            }

            switch current.kind {
            case .comma:
                _ = advance()
            case .rightParen:
                break scan
            default:
                throw EngineError.parseError(message: "expected ')' or ','",
                                             position: current.position)
            }
        }
        _ = advance() // ')'
        return .mapLiteral(entries)
    }
}
