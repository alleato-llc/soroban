/// A re-parseable rendering of an expression. Used by lambda values for
/// display AND workbook persistence (`Value.description` → `Value(parsing:)`),
/// so the contract is round-tripping, not prettiness: compound subexpressions
/// are parenthesized conservatively rather than by precedence analysis.
extension Expression {
    /// Canonical (Normal-mode) rendering. External callers — `Value.description`,
    /// workbook persistence — get this, and it must round-trip via `Parser.parse`.
    public var sourceText: String { sourceText(mode: .normal) }

    /// Renders under a display dialect. Only the overloaded glyphs differ between
    /// modes (`^ % & | << >>`); everything else is identical. In `.programmer`
    /// the canonical bitwise/mod calls and the power/percent nodes render with
    /// their infix glyphs; in `.normal`/`.scientific` they render as the canonical
    /// function / `^` / `%`. The result re-parses *under the same mode*. See
    /// `docs/MODES.md`.
    public func sourceText(mode: LanguageMode = .normal) -> String {
        func sub(_ e: Expression) -> String { e.sourceText(mode: mode) }
        switch self {
        case .number(let value):
            return value.description
        case .money(let value, let currency):
            // The currency literal is core grammar — it re-parses in any mode;
            // the symbol is part of the value, so it always renders.
            let magnitude = value.isNegative ? -value : value
            return (value.isNegative ? "-" : "") + currency.symbol + magnitude.description
        case .grouped(let value):
            // Grouping is presentation; source renders the plain number.
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
            return "(-\(sub(inner)))"
        case .percent(let inner):
            // Programmer mode has no `%`-percent (it's modulo) → fall back to ×0.01.
            if mode == .programmer { return "(\(sub(inner)) * 0.01)" }
            // Postfix; compound inner expressions self-parenthesize, so `3%`,
            // `x%`, `(a + b)%` all re-parse to the same percent.
            return "\(sub(inner))%"
        case .degrees(let inner):
            // `°` is mode-agnostic (no dialect owns another meaning), so it
            // renders — and re-parses — identically everywhere.
            return "\(sub(inner))°"
        case .binary(let op, let lhs, let rhs):
            // Power has no `^` glyph in Programmer mode (`^` is XOR) → pow(...).
            if mode == .programmer, case .power = op {
                return "pow(\(sub(lhs)), \(sub(rhs)))"
            }
            return "(\(sub(lhs)) \(op.rawValue) \(sub(rhs)))"
        case .comparison(let op, let lhs, let rhs):
            return "(\(sub(lhs)) \(op.rawValue) \(sub(rhs)))"
        case .call(let name, let arguments):
            // Programmer mode re-spells the canonical bitwise/mod calls as infix.
            if mode == .programmer, name.lowercased() == "bitnot", arguments.count == 1 {
                return "~\(sub(arguments[0]))"
            }
            if mode == .programmer, let infix = Self.programmerInfix(name, arguments, sub) {
                return infix
            }
            return "\(name)(\(arguments.map(sub).joined(separator: ", ")))"
        case .conditional(let condition, let then, let otherwise):
            return "if(\(sub(condition)), \(sub(then)), \(sub(otherwise)))"
        case .assignment(let name, let value):
            return "\(name) = \(sub(value))"
        case .functionDefinition(let name, let parameters, let body):
            let params = parameters.map { param in
                param.type.map { "\(param.name): \($0.label)" } ?? param.name
            }
            return "\(name)(\(params.joined(separator: ", "))) = \(sub(body))"
        case .reduction(let operation, let index, let lower, let upper, let body):
            // The typed spelling; bounds parenthesized (boundPrimary takes those).
            let keyword = operation == .sum ? "sigma" : "product"
            return "\(keyword)_\(index)=(\(sub(lower)))^(\(sub(upper)))(\(sub(body)))"
        case .helpRequest(let name):
            return "man \(name)"
        case .arrayLiteral(let items):
            return "[\(items.map(sub).joined(separator: ", "))]"
        case .mapLiteral(let entries):
            let body = entries.map { "\(Value.keyLiteral($0.key)): \(sub($0.value))" }
                .joined(separator: ", ")
            return "{\(body)}"
        case .index(let base, let indexExpr):
            return "\(sub(base))[\(sub(indexExpr))]"
        case .member(let base, let name):
            return "\(sub(base)).\(name)"
        case .methodCall(let base, let name, let arguments):
            return "\(sub(base)).\(name)(\(arguments.map(sub).joined(separator: ", ")))"
        case .lambda(let parameters, let body):
            return "(\(parameters.joined(separator: ", "))) -> \(sub(body))"
        case .nameReference(let sheet, let name):
            return Self.qualified(sheet) + "'\(name)'"
        case .dataDefinition(let name, let fields):
            return "data \(name) { "
                + fields.map { "\($0.name): \($0.type.label)" }.joined(separator: ", ")
                + " }"
        case .namespaceDefinition(let name, let members):
            return "namespace \(name) { "
                + members.map { sub($0) }.joined(separator: "; ")
                + " }"
        case .importDirective(let name):
            return "import \(name)"
        }
    }

    /// In Programmer mode the canonical 2-arg bitwise/mod calls render with their
    /// infix glyphs (parenthesized, like `.binary`, for safe re-parsing). Returns
    /// nil when the call isn't one of these (render as an ordinary call). `>>` is
    /// recovered from a negated shift count (`bitShift(a, -n)` ≡ `a >> n`).
    private static func programmerInfix(_ name: String, _ args: [Expression],
                                        _ sub: (Expression) -> String) -> String? {
        guard args.count == 2 else { return nil }
        let lhs = sub(args[0])
        switch name.lowercased() {
        case "bitxor": return "(\(lhs) ^ \(sub(args[1])))"
        case "bitand": return "(\(lhs) & \(sub(args[1])))"
        case "bitor":  return "(\(lhs) | \(sub(args[1])))"
        case "mod":    return "(\(lhs) % \(sub(args[1])))"
        case "bitshift":
            if case .unaryMinus(let inner) = args[1] { return "(\(lhs) >> \(sub(inner)))" }
            return "(\(lhs) << \(sub(args[1])))"
        default: return nil
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
