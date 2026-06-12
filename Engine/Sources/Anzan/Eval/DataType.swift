import Foundation // trimmingCharacters for the trailing # doc comment

/// A user-declared record type: `data Person { name: String, age: Number,
/// active: Boolean }`. Like `UserFunction`, the original `source` line is
/// kept for workbook serialization — including any trailing `# doc comment`,
/// which is the type's documentation.
///
/// Construction goes through the type's CONSTRUCTOR (the type name, called
/// like a function): named fields — `Person(name: "Ada", age: 36, active:
/// true)` — or one map. There is deliberately no positional form (user
/// decision: field names at every call site).
public struct DataType: Equatable, Sendable {
    /// As declared — must start with a capital letter. Constructor calls are
    /// case-insensitive, like every function.
    public let name: String
    /// Declaration order — instances canonicalize their fields to it.
    public let fields: [DataField]
    public internal(set) var source: String

    public init(name: String, fields: [DataField], source: String = "") {
        self.name = name
        self.fields = fields
        self.source = source
    }

    /// The trailing `# …` comment of the declaration, if any — the user's
    /// own documentation, shown by man()/the reference window.
    public var documentation: String? {
        guard let hash = source.firstIndex(of: "#") else { return nil }
        let text = source[source.index(after: hash)...].trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : text
    }

    /// Display form: `data Person { name: String, age: Number }`.
    public var declaration: String {
        "data \(name) { "
            + fields.map { "\($0.name): \($0.type.label)" }.joined(separator: ", ")
            + " }"
    }

    /// "name, age, active" — for error messages.
    var fieldList: String {
        fields.map(\.name).joined(separator: ", ")
    }
}

/// One declared field. Names are case-sensitive (they become map-style keys);
/// duplicates are rejected at parse time.
public struct DataField: Equatable, Sendable {
    public let name: String
    public let type: DataFieldType

    public init(name: String, type: DataFieldType) {
        self.name = name
        self.type = type
    }

    /// Checks a constructor argument against the declared type. Booleans are
    /// the engine's 1/0 truth values, but a Boolean FIELD is strict — exactly
    /// 0 or 1, so `active: 7` is caught instead of silently truthy.
    func validated(_ value: Value, in typeName: String) throws -> Value {
        switch type {
        case .number:
            guard case .number = value else {
                throw EngineError.domainError(
                    message: "'\(name)' of \(typeName) is a Number — got \(value.kindName)")
            }
        case .string:
            guard case .string = value else {
                throw EngineError.domainError(
                    message: "'\(name)' of \(typeName) is a String — got \(value.kindName)")
            }
        case .boolean:
            guard case .number(let flag) = value, flag == .zero || flag == .one else {
                throw EngineError.domainError(
                    message: "'\(name)' of \(typeName) is a Boolean — use true or false")
            }
        case .record(let expected):
            // The value is already a validated instance (records are immutable
            // and only made by their constructor), so a type-name check is
            // enough — no recursive re-validation, no cycle risk.
            guard case .record(let record) = value,
                  record.typeName.lowercased() == expected.lowercased() else {
                throw EngineError.domainError(
                    message: "'\(name)' of \(typeName) is a \(expected) — got \(value.kindName)")
            }
        }
        return value
    }
}

/// A field's type: a built-in scalar (Boolean fields hold the engine's 1/0 but
/// render/serialize as true/false), or `.record(name)` — another declared
/// data type, so records nest (`data Line { a: Point, b: Point }`). No Int
/// refinement and no list types yet.
public enum DataFieldType: Equatable, Sendable {
    case number
    case string
    case boolean
    case record(String) // a declared data type, e.g. Point

    /// Declarations accept any casing for the scalars (`boolean`, `NUMBER`, …);
    /// anything else that starts with a capital names a data type (existence
    /// checked at construction). Returns nil for a non-type token.
    public init?(parsing text: String) {
        switch text.lowercased() {
        case "number": self = .number
        case "string": self = .string
        case "boolean": self = .boolean
        default:
            guard let first = text.first, first.isUppercase else { return nil }
            self = .record(text)
        }
    }

    /// Canonical spelling — `Number` / `Point`.
    public var label: String {
        switch self {
        case .number: return "Number"
        case .string: return "String"
        case .boolean: return "Boolean"
        case .record(let name): return name
        }
    }
}
