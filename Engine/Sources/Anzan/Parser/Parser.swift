/// Pratt (precedence-climbing) parser.
///
/// Grammar, low to high precedence:
///   assignment:  IDENT = expr          (only at top level)
///   additive:    + -
///   term:        * / %  and implicit multiplication (`2(3+4)`, `2x`, `(a)(b)`)
///   unary:       -x
///   power:       ^ (right-associative, binds tighter than unary: -2^2 == -4)
///   primary:     number | ident | ident(args) | (expr)
package struct Parser {
    private let tokens: [Token]
    private var index = 0

    private init(tokens: [Token]) {
        self.tokens = tokens
    }

    package static func parse(_ source: String) throws(EngineError) -> Expression {
        var parser = Parser(tokens: try Lexer.tokenize(source))
        let expr = try parser.statement()
        try parser.expectEnd()
        return expr
    }

    private var current: Token { tokens[index] }

    private mutating func advance() -> Token {
        defer { if index < tokens.count - 1 { index += 1 } }
        return current
    }

    private mutating func expectEnd() throws(EngineError) {
        guard case .end = current.kind else {
            throw EngineError.parseError(message: "unexpected trailing input", position: current.position)
        }
    }

    // MARK: Productions

    /// `x = expr`, `f(a, b) = expr`, or a plain expression.
    private mutating func statement() throws(EngineError) -> Expression {
        if case .identifier(let name) = current.kind,
           index + 1 < tokens.count, case .assign = tokens[index + 1].kind,
           !Parser.isReductionName(name) { // sigma_i=1^10(…) is a summation, not an assignment
            let position = current.position
            guard !ReservedNames.contains(name.lowercased()) else {
                throw EngineError.parseError(message: "cannot assign to '\(name)'", position: position)
            }
            index += 2
            return .assignment(name: name, value: try comparison())
        }
        if let dataDefinition = try dataDefinition() {
            return dataDefinition
        }
        if let definition = try functionDefinition() {
            return definition
        }
        return try comparison()
    }

    /// `data Person { name: String, age: Number, active: Boolean }`.
    /// `data` is a CONTEXTUAL keyword: only the exact shape `data Ident {`
    /// commits (returns nil otherwise), so `data = 5` stays an assignment
    /// and `data` stays a usable variable name. Matched case-insensitively,
    /// like function names.
    private mutating func dataDefinition() throws(EngineError) -> Expression? {
        guard case .identifier(let keyword) = current.kind, keyword.lowercased() == "data",
              index + 2 < tokens.count,
              case .identifier(let name) = tokens[index + 1].kind,
              case .leftBrace = tokens[index + 2].kind else { return nil }
        let namePosition = tokens[index + 1].position
        index += 3

        // Definitely a declaration now — validate and parse the fields.
        guard let first = name.first, first.isUppercase else {
            throw EngineError.parseError(
                message: "data type names start with a capital letter — e.g. data Person { … }",
                position: namePosition)
        }
        guard !ReservedNames.contains(name.lowercased()), !Parser.isReductionName(name) else {
            throw EngineError.parseError(message: "cannot define '\(name)'", position: namePosition)
        }

        var fields: [DataField] = []
        scan: while true {
            switch current.kind {
            case .rightBrace where fields.isEmpty:
                throw EngineError.parseError(
                    message: "a data type needs at least one field — e.g. data \(name) { name: String }",
                    position: current.position)
            case .identifier(let fieldName):
                let fieldPosition = current.position
                _ = advance()
                guard case .colon = current.kind else {
                    throw EngineError.parseError(
                        message: "expected ':' after field '\(fieldName)' — e.g. \(fieldName): Number",
                        position: current.position)
                }
                _ = advance()
                guard case .identifier(let typeName) = current.kind,
                      let type = DataFieldType(parsing: typeName) else {
                    throw EngineError.parseError(
                        message: "field types are Number, String, Boolean, or a declared data "
                            + "type — e.g. \(fieldName): Number",
                        position: current.position)
                }
                _ = advance()
                guard !fields.contains(where: {
                    $0.name.lowercased() == fieldName.lowercased()
                }) else {
                    throw EngineError.parseError(message: "duplicate field '\(fieldName)'",
                                                 position: fieldPosition)
                }
                fields.append(DataField(name: fieldName, type: type))
            default:
                throw EngineError.parseError(
                    message: "expected a field — e.g. data \(name) { name: String, age: Number }",
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
        return .dataDefinition(name: name, fields: fields)
    }

    /// One comparison level: `additive (op additive)?`. Single comparison
    /// only — `a < b < c` is rejected (1/0 chaining is never what you meant).
    /// Lambdas are checked first: every expression entry point comes through
    /// here, so `x -> …` works as an argument, an assignment value, a body…
    private mutating func comparison() throws(EngineError) -> Expression {
        if let lambda = try lambdaExpression() {
            return lambda
        }
        let lhs = try additive()
        guard let op = comparisonOperator(for: current.kind) else { return lhs }
        _ = advance()
        let rhs = try additive()
        if comparisonOperator(for: current.kind) != nil {
            throw EngineError.parseError(
                message: "comparisons can't be chained — use and(a < b, b < c)",
                position: current.position)
        }
        return .comparison(op, lhs, rhs)
    }

    private func comparisonOperator(for kind: Token.Kind) -> ComparisonOperator? {
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
    private mutating func lambdaExpression() throws(EngineError) -> Expression? {
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

    private mutating func lambdaBody(parameters: [String],
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

    /// Speculatively parses `ident(p1, p2, …) = expr`. Returns nil — with the
    /// position rewound — when the lookahead isn't exactly that shape, so
    /// `f(x)` stays a call and `f(2) = 1` stays a parse error downstream.
    private mutating func functionDefinition() throws(EngineError) -> Expression? {
        let start = index
        // The name is an identifier (`f`, `dist`) or an arithmetic operator
        // symbol (`+`), which overloads that operator for typed operands.
        let name: String
        if case .identifier(let identifier) = current.kind {
            name = identifier
        } else if let op = Parser.operatorDefinitionName(for: current.kind) {
            name = op
        } else {
            return nil
        }
        guard index + 1 < tokens.count, case .leftParen = tokens[index + 1].kind else { return nil }
        let namePosition = current.position
        index += 2

        var parameters: [Parameter] = []
        scan: while true {
            switch current.kind {
            case .rightParen where parameters.isEmpty: // f() = …
                _ = advance()
                break scan
            case .identifier(let parameter):
                _ = advance()
                // Optional `: Type` annotation — `dist(p: Point)`.
                var type: TypeAnnotation? = nil
                if case .colon = current.kind {
                    _ = advance()
                    guard case .identifier(let typeName) = current.kind else {
                        index = start
                        return nil
                    }
                    type = TypeAnnotation(parsing: typeName)
                    _ = advance()
                }
                parameters.append(Parameter(name: parameter, type: type))
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

        guard case .assign = current.kind else {
            index = start
            return nil
        }
        _ = advance()

        // Definitely a definition now — validate and parse the body.
        guard !ReservedNames.contains(name.lowercased()),
              !Parser.isReductionName(name) else {
            throw EngineError.parseError(message: "cannot define '\(name)'", position: namePosition)
        }
        let paramNames = parameters.map { $0.name.lowercased() }
        guard Set(paramNames).count == paramNames.count else {
            throw EngineError.parseError(message: "duplicate parameter name", position: namePosition)
        }
        return .functionDefinition(name: name, parameters: parameters, body: try comparison())
    }

    /// The operator symbols that can name an overload definition — the six
    /// arithmetic binary operators. Comparisons/equality are not overloadable.
    static func operatorDefinitionName(for kind: Token.Kind) -> String? {
        switch kind {
        case .plus: return "+"
        case .minus: return "-"
        case .star: return "*"
        case .slash: return "/"
        case .caret: return "^"
        default: return nil // `%` is postfix percent, not an overloadable operator
        }
    }

    private mutating func additive() throws(EngineError) -> Expression {
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

    private mutating func term() throws(EngineError) -> Expression {
        var lhs = try unary()
        while true {
            switch current.kind {
            case .star:
                _ = advance()
                lhs = .binary(.multiply, lhs, try unary())
            case .slash:
                _ = advance()
                lhs = .binary(.divide, lhs, try unary())
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

    private mutating func unary() throws(EngineError) -> Expression {
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
        return try power()
    }

    private mutating func power() throws(EngineError) -> Expression {
        let base = try postfix()
        guard case .caret = current.kind else { return base }
        _ = advance()
        // Right-associative; the exponent may carry its own unary minus (2^-1).
        return .binary(.power, base, try unary())
    }

    /// Postfix accessors, binding tighter than `^`: `arr[0]`, `m.name`,
    /// chained freely (`people[0].age`, `grid[1][2]`).
    private mutating func postfix() throws(EngineError) -> Expression {
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
            case .percent:
                // Postfix percent: `3%` → 0.03. Always percent (modulo is mod());
                // chains like other postfixes (`A:1%`, `arr[0]%`).
                _ = advance()
                expr = .percent(expr)
            default:
                break loop
            }
        }
        return expr
    }

    private mutating func primary() throws(EngineError) -> Expression {
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

            // man(name) / help(name): the argument is a name, not a value —
            // parse it before the normal (evaluating) argument list would.
            if lowered == "man" || lowered == "help" {
                guard case .leftParen = current.kind else {
                    throw EngineError.parseError(
                        message: "usage: \(name)(functionName) — e.g. \(name)(pmt)",
                        position: token.position)
                }
                _ = advance()
                guard case .identifier(let subject) = current.kind else {
                    throw EngineError.parseError(
                        message: "\(name)() needs a function name — e.g. \(name)(pmt)",
                        position: current.position)
                }
                _ = advance()
                guard case .rightParen = current.kind else {
                    throw EngineError.parseError(message: "expected ')'", position: current.position)
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
    private mutating func arrayLiteral() throws(EngineError) -> Expression {
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
    private mutating func mapLiteral(at position: Int) throws(EngineError) -> Expression {
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
    private mutating func mathReduction(_ operation: ReductionOperation,
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
    private mutating func signedPrimary() throws(EngineError) -> Expression {
        if case .minus = current.kind {
            _ = advance()
            return .unaryMinus(try boundPrimary())
        }
        return try boundPrimary()
    }

    private mutating func boundPrimary() throws(EngineError) -> Expression {
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

    private func isBang(_ kind: Token.Kind) -> Bool {
        if case .bang = kind { return true }
        return false
    }

    /// After a sheet name: `!` then a cell reference, range, or 'named cell'.
    private mutating func qualifiedReference(sheet: String,
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
    private mutating func cellReferenceOrRange(sheet: String?, column: String,
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
    private mutating func argumentList() throws(EngineError) -> [Expression] {
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
    private func isNamedArgumentStart() -> Bool {
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
    private mutating func namedArguments() throws(EngineError) -> Expression {
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

/// Identifiers that cannot be assigned to (or defined as functions).
/// `sigma` is the special summation form, so a user function would be
/// uncallable anyway.
let ReservedNames: Set<String> = ["ans", "pi", "e", "tau", "π", "τ", "sigma", "if", "man", "help",
                                  "true", "false", "json"]

extension Parser {
    /// `sigma_x`/`product_x` spellings are reserved for the indexed
    /// reduction forms.
    static func isReductionName(_ name: String) -> Bool {
        let lowered = name.lowercased()
        return lowered.hasPrefix("sigma_") || lowered.hasPrefix("product_")
    }
}
