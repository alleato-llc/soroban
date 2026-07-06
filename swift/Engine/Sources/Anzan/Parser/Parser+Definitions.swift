// Statement-level definitions: `namespace`, `data`, and `f(x) = …` function
// definitions (all contextual keywords committed only by their exact token
// shape), plus the field-type and operator-overload-name helpers.

extension Parser {
    /// `namespace Bits { data BitField { … }  data BitFormat { … } }`. Like
    /// `data`, a CONTEXTUAL keyword — committed only by `namespace Ident {`.
    /// In 2a-i the body holds `data` declarations (docs/MODULES.md); other
    /// members are rejected by the evaluator with a clear message.
    mutating func namespaceDefinition() throws(EngineError) -> Expression? {
        guard case .identifier(let keyword) = current.kind, keyword.lowercased() == "namespace",
              index + 2 < tokens.count,
              case .identifier(let name) = tokens[index + 1].kind,
              case .leftBrace = tokens[index + 2].kind else { return nil }
        let namePosition = tokens[index + 1].position
        index += 3 // past `namespace Name {`

        guard let first = name.first, first.isUppercase else {
            throw EngineError.parseError(
                message: "namespace names start with a capital letter — e.g. namespace Bits { … }",
                position: namePosition)
        }
        guard !ReservedNames.contains(name.lowercased()), !Parser.isReductionName(name) else {
            throw EngineError.parseError(message: "cannot define '\(name)'", position: namePosition)
        }

        // Members are `;`-separated (a function body would otherwise run into
        // the next member via implicit multiplication); a trailing `;` is fine.
        var members: [Expression] = []
        while true {
            if case .rightBrace = current.kind { break }
            if case .end = current.kind {
                throw EngineError.parseError(message: "expected '}' to close namespace \(name)",
                                             position: current.position)
            }
            members.append(try statement())
            switch current.kind {
            case .semicolon: _ = advance()
            case .rightBrace: break
            default:
                throw EngineError.parseError(
                    message: "separate namespace declarations with ';' — e.g. data A { … }; f(x) = …",
                    position: current.position)
            }
        }
        _ = advance() // '}'
        guard !members.isEmpty else {
            throw EngineError.parseError(
                message: "a namespace needs at least one declaration — e.g. namespace \(name) { data Point { x: Number } }",
                position: namePosition)
        }
        return .namespaceDefinition(name: name, members: members)
    }

    /// `data Person { name: String, age: Number, active: Boolean }`.
    /// `data` is a CONTEXTUAL keyword: only the exact shape `data Ident {`
    /// commits (returns nil otherwise), so `data = 5` stays an assignment
    /// and `data` stays a usable variable name. Matched case-insensitively,
    /// like function names.
    mutating func dataDefinition() throws(EngineError) -> Expression? {
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
                _ = advance() // consume ':'
                let type = try parseFieldType(field: fieldName)
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

    /// A field type: a leaf (Number/String/Boolean/a data type), a list `[T]`,
    /// or a string-keyed map `{String: T}` — recursive, so `[[Number]]` and
    /// `{String: [Point]}` work.
    mutating func parseFieldType(field fieldName: String) throws(EngineError) -> DataFieldType {
        switch current.kind {
        case .leftBracket:
            _ = advance()
            let element = try parseFieldType(field: fieldName)
            guard case .rightBracket = current.kind else {
                throw EngineError.parseError(message: "expected ']' to close the list type — e.g. [String]",
                                             position: current.position)
            }
            _ = advance()
            return .list(element)
        case .leftBrace:
            _ = advance()
            guard case .identifier(let key) = current.kind, key.lowercased() == "string" else {
                throw EngineError.parseError(
                    message: "map field keys are String — e.g. {String: Number}", position: current.position)
            }
            _ = advance()
            guard case .colon = current.kind else {
                throw EngineError.parseError(
                    message: "expected ':' in the map type — e.g. {String: Number}", position: current.position)
            }
            _ = advance()
            let valueType = try parseFieldType(field: fieldName)
            guard case .rightBrace = current.kind else {
                throw EngineError.parseError(message: "expected '}' to close the map type — e.g. {String: Number}",
                                             position: current.position)
            }
            _ = advance()
            return .map(valueType)
        case .identifier(let typeName):
            guard let type = DataFieldType(parsing: typeName) else { break }
            _ = advance()
            return type
        default:
            break
        }
        throw EngineError.parseError(
            message: "field types are Number, String, Boolean, a declared data type, or a list/map "
                + "of those ([T], {String: T}) — e.g. \(fieldName): Number",
            position: current.position)
    }

    /// Speculatively parses `ident(p1, p2, …) = expr`. Returns nil — with the
    /// position rewound — when the lookahead isn't exactly that shape, so
    /// `f(x)` stays a call and `f(2) = 1` stays a parse error downstream.
    mutating func functionDefinition() throws(EngineError) -> Expression? {
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
}
