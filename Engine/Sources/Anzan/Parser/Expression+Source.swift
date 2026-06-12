/// A re-parseable rendering of an expression. Used by lambda values for
/// display AND workbook persistence (`Value.description` → `Value(parsing:)`),
/// so the contract is round-tripping, not prettiness: compound subexpressions
/// are parenthesized conservatively rather than by precedence analysis.
extension Expression {
    public var sourceText: String {
        switch self {
        case .number(let value):
            return value.description
        case .stringLiteral(let text):
            return Value.quoted(text)
        case .variable(let name):
            return name
        case .cellReference(let sheet, let column, let row):
            return Self.qualified(sheet) + "\(column):\(row)"
        case .cellRange(let sheet, let fromColumn, let fromRow, let toColumn, let toRow):
            return Self.qualified(sheet) + "\(fromColumn):\(fromRow)..\(toColumn):\(toRow)"
        case .unaryMinus(let inner):
            return "(-\(inner.sourceText))"
        case .percent(let inner):
            // Postfix; compound inner expressions self-parenthesize, so `3%`,
            // `x%`, `(a + b)%` all re-parse to the same percent.
            return "\(inner.sourceText)%"
        case .binary(let op, let lhs, let rhs):
            return "(\(lhs.sourceText) \(op.rawValue) \(rhs.sourceText))"
        case .comparison(let op, let lhs, let rhs):
            return "(\(lhs.sourceText) \(op.rawValue) \(rhs.sourceText))"
        case .call(let name, let arguments):
            return "\(name)(\(arguments.map(\.sourceText).joined(separator: ", ")))"
        case .conditional(let condition, let then, let otherwise):
            return "if(\(condition.sourceText), \(then.sourceText), \(otherwise.sourceText))"
        case .assignment(let name, let value):
            return "\(name) = \(value.sourceText)"
        case .functionDefinition(let name, let parameters, let body):
            let params = parameters.map { param in
                param.type.map { "\(param.name): \($0.label)" } ?? param.name
            }
            return "\(name)(\(params.joined(separator: ", "))) = \(body.sourceText)"
        case .reduction(let operation, let index, let lower, let upper, let body):
            // The typed spelling; bounds parenthesized (boundPrimary takes those).
            let keyword = operation == .sum ? "sigma" : "product"
            return "\(keyword)_\(index)=(\(lower.sourceText))^(\(upper.sourceText))(\(body.sourceText))"
        case .helpRequest(let name):
            return "man(\(name))"
        case .arrayLiteral(let items):
            return "[\(items.map(\.sourceText).joined(separator: ", "))]"
        case .mapLiteral(let entries):
            let body = entries.map { "\(Value.keyLiteral($0.key)): \($0.value.sourceText)" }
                .joined(separator: ", ")
            return "{\(body)}"
        case .index(let base, let indexExpr):
            return "\(base.sourceText)[\(indexExpr.sourceText)]"
        case .member(let base, let name):
            return "\(base.sourceText).\(name)"
        case .methodCall(let base, let name, let arguments):
            return "\(base.sourceText).\(name)(\(arguments.map(\.sourceText).joined(separator: ", ")))"
        case .lambda(let parameters, let body):
            return "(\(parameters.joined(separator: ", "))) -> \(body.sourceText)"
        case .nameReference(let sheet, let name):
            return Self.qualified(sheet) + "'\(name)'"
        case .dataDefinition(let name, let fields):
            return "data \(name) { "
                + fields.map { "\($0.name): \($0.type.label)" }.joined(separator: ", ")
                + " }"
        }
    }

    private static func qualified(_ sheet: String?) -> String {
        guard let sheet else { return "" }
        // Quote unless the name is a plain identifier.
        let plain = !sheet.isEmpty && sheet.first!.isLetter
            && sheet.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
        return plain ? "\(sheet)!" : "'\(sheet)'!"
    }
}
