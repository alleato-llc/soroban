/// Expression tree produced by the parser.
public indirect enum Expression: Equatable, Sendable {
    case number(BigDecimal)
    /// A finance-mode currency literal — `$10`, `€10`, `$10,000`. The currency
    /// propagates through arithmetic (see `Money`), so it is part of the value.
    case money(BigDecimal, currency: Currency)
    /// A finance-mode grouped plain number — `138,561`. Presentation only; the
    /// grouping echoes through a calculation (see `Grouped`).
    case grouped(BigDecimal)
    case variable(String)
    /// `A:1` or `Budget!A:1` / `'Q1 Budget'!A:1` — nil sheet means the sheet
    /// that owns the formula (or the active sheet, from the log).
    case cellReference(sheet: String?, column: String, row: Int)
    /// A:1..B:9 — expands to the rectangle's numeric values; valid only as a
    /// function argument (sum(A:1..A:9)). May be sheet-qualified.
    case cellRange(sheet: String?, fromColumn: String, fromRow: Int,
                   toColumn: String, toRow: Int)
    case unaryMinus(Expression)
    /// `3%` — postfix percent: the operand divided by 100 (3% → 0.03), exact.
    /// Binds tighter than `^` (like indexing); modulo is the `mod(x, y)` function.
    case percent(Expression)
    case binary(BinaryOperator, Expression, Expression)
    case call(name: String, arguments: [Expression])
    case assignment(name: String, value: Expression)
    /// `f(x) = …` / `dist(p: Point) = …` / `+(a: Point, b: Point) = …`.
    /// Parameters may carry a type annotation; the same `name` can have several
    /// definitions distinguished by their annotations (typed dispatch). The
    /// `name` may be an operator symbol (`+`), which overloads that operator.
    case functionDefinition(name: String, parameters: [Parameter], body: Expression)
    /// ∑_i=1^10(term) / ∏_i=1^5(term) — binding forms: `term` is re-evaluated
    /// with `index` bound to each integer in lower...upper, accumulated by
    /// the operation.
    case reduction(operation: ReductionOperation, index: String,
                   lower: Expression, upper: Expression, body: Expression)
    /// `a < b` etc. — evaluates to 1 (true) or 0 (false).
    case comparison(ComparisonOperator, Expression, Expression)
    /// `if(cond, then, else)` — a special form: only the taken branch is
    /// evaluated, so the other may divide by zero or recurse.
    case conditional(condition: Expression, then: Expression, else: Expression)
    /// `man pmt` / `manual pmt` / `help pmt` — prints documentation; the
    /// argument is a NAME, never evaluated, space-separated (no parentheses).
    case helpRequest(name: String)
    /// `"…"` — a string value.
    case stringLiteral(String)
    /// `[1, 2, 3]` — elements are full expressions; nests freely.
    case arrayLiteral([Expression])
    /// `{name: "Ada", age: 36}` — keys unique and case-sensitive.
    case mapLiteral([MapLiteralEntry])
    /// `arr[0]` / `m["key"]` — 0-based for arrays and strings; string keys
    /// for maps.
    case index(base: Expression, index: Expression)
    /// `m.name` — map member access with a literal key.
    case member(base: Expression, name: String)
    /// `worksheet.cell("A", 2)` — a method call on a host value. Distinct from
    /// member access (no parens) and from a free `name(args)` call.
    case methodCall(base: Expression, name: String, arguments: [Expression])
    /// `x -> x * 2` / `(a, b) -> a + b` — an anonymous function value.
    /// Locals in scope at evaluation are captured by value (closure).
    case lambda(parameters: [String], body: Expression)
    /// `'Projected Rate'` / `Budget!'Projected Rate'` — a NAMED CELL
    /// reference. Single quotes are Soroban's name-of-a-thing syntax (sheets
    /// already use them); nil sheet = the owning sheet (active, from the log).
    case nameReference(sheet: String?, name: String)
    /// `data Person { name: String, age: Number, active: Boolean }` —
    /// declares a record type whose name becomes its constructor.
    case dataDefinition(name: String, fields: [DataField])
    /// `namespace Bits { data BitField { … }  data BitFormat { … } }` — groups
    /// declarations under a name; members are reached as `Bits::BitField`. In
    /// 2a-i the members are data declarations (see docs/MODULES.md).
    case namespaceDefinition(name: String, members: [Expression])
    /// `import Bits` — brings a namespace's members into scope unqualified
    /// (docs/MODULES.md 2b).
    case importDirective(name: String)
}

/// A function parameter: a name and an optional type annotation. An
/// un-annotated parameter (type == nil) matches an argument of any type; an
/// annotated one participates in typed dispatch.
public struct Parameter: Equatable, Sendable {
    public let name: String
    public let type: TypeAnnotation?

    public init(name: String, type: TypeAnnotation? = nil) {
        self.name = name
        self.type = type
    }

    /// Source/display form: `p: Point` when typed, else `p`.
    public var rendered: String {
        type.map { "\(name): \($0.label)" } ?? name
    }
}

/// A parameter's declared type: a built-in scalar or a named `data` type.
public enum TypeAnnotation: Equatable, Sendable {
    case number
    case string
    case boolean
    case named(String) // a declared data type, e.g. Point

    /// Maps a written type name to an annotation. The three scalars match
    /// case-insensitively (like `data` field types); anything else is a named
    /// data type, spelling preserved (existence checked at dispatch time).
    public init(parsing name: String) {
        switch name.lowercased() {
        case "number": self = .number
        case "string": self = .string
        case "boolean": self = .boolean
        default: self = .named(name)
        }
    }

    /// As written in source — `Number` / `Point`.
    public var label: String {
        switch self {
        case .number: return "Number"
        case .string: return "String"
        case .boolean: return "Boolean"
        case .named(let name): return name
        }
    }

    /// Within a namespace, qualify a type annotation (`p: Point` →
    /// `p: Bits::Point`) so typed dispatch matches the qualified instances.
    /// `scope` maps a simple type name (lowercased) to its qualified form,
    /// accumulated from the enclosing namespaces — so a nested member can name
    /// a parent's type unqualified. An already-qualified or out-of-scope name
    /// is left alone.
    func qualified(using scope: [String: String]) -> TypeAnnotation {
        guard case .named(let name) = self, !name.contains("::"),
              let qualified = scope[name.lowercased()] else { return self }
        return .named(qualified)
    }
}

/// One `key: value` pair of a map literal.
public struct MapLiteralEntry: Equatable, Sendable {
    public let key: String
    public let value: Expression

    public init(key: String, value: Expression) {
        self.key = key
        self.value = value
    }
}

public enum ComparisonOperator: String, Equatable, Sendable {
    case less = "<"
    case greater = ">"
    case lessOrEqual = "<="
    case greaterOrEqual = ">="
    case equal = "=="
    case notEqual = "!="
}

public enum ReductionOperation: Equatable, Sendable {
    case sum     // ∑ — starts at 0, accumulates with +
    case product // ∏ — starts at 1, accumulates with ×

    var symbol: String {
        switch self {
        case .sum: return "∑"
        case .product: return "∏"
        }
    }
}

extension Expression {
    /// True if any node references a spreadsheet cell — used by the grid's
    /// formula auto-detection.
    public var containsCellReference: Bool {
        switch self {
        case .cellReference, .cellRange:
            return true
        case .number, .money, .grouped, .variable:
            return false
        case .unaryMinus(let inner), .percent(let inner):
            return inner.containsCellReference
        case .binary(_, let lhs, let rhs):
            return lhs.containsCellReference || rhs.containsCellReference
        case .call(_, let arguments):
            return arguments.contains(where: \.containsCellReference)
        case .assignment(_, let value):
            return value.containsCellReference
        case .functionDefinition(_, _, let body):
            return body.containsCellReference
        case .reduction(_, _, let lower, let upper, let body):
            return lower.containsCellReference || upper.containsCellReference
                || body.containsCellReference
        case .comparison(_, let lhs, let rhs):
            return lhs.containsCellReference || rhs.containsCellReference
        case .conditional(let condition, let then, let other):
            return condition.containsCellReference || then.containsCellReference
                || other.containsCellReference
        case .helpRequest, .stringLiteral:
            return false
        case .arrayLiteral(let items):
            return items.contains(where: \.containsCellReference)
        case .mapLiteral(let entries):
            return entries.contains(where: \.value.containsCellReference)
        case .index(let base, let indexExpr):
            return base.containsCellReference || indexExpr.containsCellReference
        case .member(let base, _):
            return base.containsCellReference
        case .methodCall(let base, _, let arguments):
            return base.containsCellReference || arguments.contains(where: \.containsCellReference)
        case .lambda(_, let body):
            return body.containsCellReference
        case .nameReference:
            return true // a named cell IS a cell reference
        case .dataDefinition, .namespaceDefinition, .importDirective:
            return false
        }
    }
}

public enum BinaryOperator: String, Equatable, Sendable {
    case add = "+"
    case subtract = "-"
    case multiply = "*"
    case divide = "/"
    case modulo = "%"
    case power = "^"
}
