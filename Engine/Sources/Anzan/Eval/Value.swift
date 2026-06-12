/// What an expression evaluates to. Numbers are the historical core; strings,
/// arrays, and maps arrived with structure support. Values are immutable —
/// there is no element assignment, only whole-variable rebinding — and they
/// nest freely (arrays of maps of arrays…).
///
/// The canonical `description` re-parses to an equal value, which is how
/// structured variables persist in workbooks (the same string mechanism
/// numbers always used).
public enum Value: Sendable {
    case number(BigDecimal)
    case string(String)
    case array([Value])
    /// Insertion-ordered key/value pairs. Keys are unique (the parser
    /// rejects duplicates) and case-sensitive, like variables.
    case map([MapEntry])
    /// A function as a value — a bare name (`map(double, arr)`) or a lambda
    /// (`map(x -> x * 2, arr)`). Applied by the higher-order builtins.
    case function(FunctionValue)
    /// An instance of a user-declared `data` type — map-shaped (member
    /// access, keys/values, HOFs all work) but tagged with its type and
    /// canonicalized to declaration order by the constructor.
    case record(RecordValue)

    /// An opaque, HOST-implemented handle navigated through a uniform protocol
    /// (`.member`/`[…]`/`.method(…)`). Anzan never knows what it is — the host
    /// (e.g. the spreadsheet's Workbook/Worksheet/Cell reflection) provides the
    /// implementations. Absent in hosts that don't inject any (the CLI).
    case host(any HostObject)

    /// The payload of `.record`. Carries what rendering/serialization needs
    /// (no back-reference to the full DataType — instances outlive
    /// redefinitions).
    public struct RecordValue: Equatable, Sendable {
        /// The declaring type's name, as declared ("Person").
        public let typeName: String
        /// Field values in declaration order.
        public let entries: [MapEntry]
        /// Fields declared Boolean — held as 1/0, rendered and serialized
        /// as true/false.
        public let booleanFields: Set<String>

        public init(typeName: String, entries: [MapEntry], booleanFields: Set<String>) {
            self.typeName = typeName
            self.entries = entries
            self.booleanFields = booleanFields
        }

        /// "true"/"false" for Boolean fields, canonical text otherwise.
        func fieldText(_ entry: MapEntry) -> String {
            if booleanFields.contains(entry.key), case .number(let flag) = entry.value {
                return flag.isZero ? "false" : "true"
            }
            return entry.value.description
        }
    }

    public struct MapEntry: Equatable, Sendable {
        public let key: String
        public let value: Value

        public init(key: String, value: Value) {
            self.key = key
            self.value = value
        }
    }

    /// Comparison results and `true`/`false` are numbers (1/0), matching the
    /// engine's long-standing truthiness convention.
    public static func bool(_ holds: Bool) -> Value {
        .number(holds ? .one : .zero)
    }

    /// "a number", "an array", … for error messages.
    public var kindName: String {
        switch self {
        case .number: return "a number"
        case .string: return "a string"
        case .array: return "an array"
        case .map: return "a map"
        case .function: return "a function"
        case .record(let record): return "a \(record.typeName)"
        case .host(let object): return "a \(object.typeName)"
        }
    }

    /// True for a `data` record instance — the trigger for operator-overload
    /// lookup (plain numeric/string math skips it).
    public var isRecord: Bool {
        if case .record = self { return true }
        return false
    }

    /// The numeric payload, or a type error naming the context:
    /// "expected a number for ^, got an array".
    public func asNumber(for context: String) throws -> BigDecimal {
        guard case .number(let value) = self else {
            throw EngineError.domainError(
                message: "expected a number for \(context), got \(kindName)")
        }
        return value
    }

    /// Numbers carried by this value, arrays flattened recursively — how
    /// numeric functions consume structured arguments (`sum(arr)` behaves
    /// like `sum(A:1..A:9)`). Strings and maps don't coerce.
    public func flattenedNumbers(for function: String) throws -> [BigDecimal] {
        switch self {
        case .number(let value):
            return [value]
        case .array(let items):
            var numbers: [BigDecimal] = []
            numbers.reserveCapacity(items.count)
            for item in items {
                numbers.append(contentsOf: try item.flattenedNumbers(for: function))
            }
            return numbers
        case .string, .map, .function, .record, .host:
            throw EngineError.domainError(
                message: "\(function)() works on numbers — got \(kindName)")
        }
    }

    /// Map/record field lookup (case-sensitive, like variables).
    public func mapValue(forKey key: String) -> Value? {
        switch self {
        case .map(let entries):
            return entries.first(where: { $0.key == key })?.value
        case .record(let record):
            return record.entries.first(where: { $0.key == key })?.value
        default:
            return nil
        }
    }
}

extension Value: Equatable {
    /// Deep equality. Maps compare order-insensitively — `{a: 1, b: 2}`
    /// equals `{b: 2, a: 1}` — because entry order is presentation, not data.
    public static func == (lhs: Value, rhs: Value) -> Bool {
        switch (lhs, rhs) {
        case (.number(let a), .number(let b)): return a == b
        case (.string(let a), .string(let b)): return a == b
        case (.array(let a), .array(let b)): return a == b
        case (.map(let a), .map(let b)):
            guard a.count == b.count else { return false }
            return a.allSatisfy { entry in
                b.first(where: { $0.key == entry.key })?.value == entry.value
            }
        case (.function(let a), .function(let b)):
            return a == b
        case (.record(let a), .record(let b)):
            // Entries compare in order — constructors canonicalize to
            // declaration order, so equal records have equal layouts.
            return a == b
        case (.host(let a), .host(let b)):
            return a.isEqual(to: b)
        default:
            return false
        }
    }
}

/// A host-implemented value that Anzan navigates without understanding: member
/// access (`.name`), indexing (`[0]` / `["Budget"]`), and method calls
/// (`.cell("A", 2)`) all route here. The host returns plain `Value`s (often
/// immutable snapshots), keeping Anzan ignorant of grids/sheets/files. Default
/// implementations make every capability opt-in.
public protocol HostObject: Sendable {
    /// For `kindName` / error messages — e.g. "Worksheet".
    var typeName: String { get }
    /// Canonical display (need not re-parse — host handles aren't literals).
    var description: String { get }
    func isEqual(to other: any HostObject) -> Bool
    func member(_ name: String) -> Value?
    func index(_ key: Value) -> Value?
    func call(_ method: String, _ arguments: [Value]) throws -> Value
}

extension HostObject {
    public func member(_ name: String) -> Value? { nil }
    public func index(_ key: Value) -> Value? { nil }
    public func call(_ method: String, _ arguments: [Value]) throws -> Value {
        throw EngineError.domainError(message: "\(typeName) has no method '\(method)'")
    }
    /// Default: compare by display — host handles are read-only snapshots, so
    /// equal display means equal state.
    public func isEqual(to other: any HostObject) -> Bool {
        typeName == other.typeName && description == other.description
    }
}

extension Value: CustomStringConvertible {
    /// Canonical, re-parseable rendering: `[1, 2]`, `{name: "Ada", age: 36}`.
    public var description: String {
        switch self {
        case .number(let value):
            return value.description
        case .string(let text):
            return Self.quoted(text)
        case .array(let items):
            return "[" + items.map(\.description).joined(separator: ", ") + "]"
        case .map(let entries):
            let body = entries.map { entry in
                "\(Self.keyLiteral(entry.key)): \(entry.value)"
            }.joined(separator: ", ")
            return "{" + body + "}"
        case .function(let function):
            return function.description
        case .record(let record):
            // Constructor-call form — re-parses to an equal record while the
            // type is defined. Field names are identifiers, so keys print
            // bare; Boolean fields print true/false.
            let body = record.entries.map { entry in
                "\(entry.key): \(record.fieldText(entry))"
            }.joined(separator: ", ")
            return "\(record.typeName)(\(body))"
        case .host(let object):
            return object.description
        }
    }

    /// Bare text for concatenation and cell display — strings without their
    /// quotes; everything else canonical.
    public var displayText: String {
        if case .string(let text) = self { return text }
        return description
    }

    /// True if this value embeds a host reflection handle (`Workbook`, a
    /// `History` entry, …) anywhere. Such handles render with NON-re-parseable
    /// descriptions (`Workbook(…)`, `[LogEntry(…)]`), so a result carrying one
    /// is display-only — it must not be recalled or treated as a value (the
    /// same reason cells reject host/array results).
    public var containsHost: Bool {
        switch self {
        case .host: return true
        case .array(let items): return items.contains { $0.containsHost }
        case .map(let entries): return entries.contains { $0.value.containsHost }
        case .number, .string, .function, .record: return false
        }
    }

    /// A string literal with the lexer's escapes applied in reverse.
    static func quoted(_ text: String) -> String {
        var out = "\""
        for ch in text {
            switch ch {
            case "\\": out += "\\\\"
            case "\"": out += "\\\""
            case "\n": out += "\\n"
            case "\t": out += "\\t"
            default: out.append(ch)
            }
        }
        return out + "\""
    }

    /// Keys print bare when they're identifier-shaped, quoted otherwise.
    static func keyLiteral(_ key: String) -> String {
        let identifierShaped = !key.isEmpty
            && (key.first!.isLetter || key.first! == "_")
            && key.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
        return identifierShaped ? key : quoted(key)
    }
}

extension Value {
    /// Parses a persisted variable value back into a Value: the fast numeric
    /// path first (every pre-structures workbook), then literal folding for
    /// `[…]`/`{…}`/`"…"` forms. Returns nil for anything that isn't a pure
    /// literal — persisted values never contain references or calls.
    public init?(parsing text: String) {
        if let number = BigDecimal(string: text) {
            self = .number(number)
            return
        }
        guard let expression = try? Parser.parse(text),
              let value = Value.literal(expression) else { return nil }
        self = value
    }

    /// Folds an AST that consists only of literals (numbers, strings, arrays,
    /// maps, and negated numbers); nil if anything needs evaluation.
    static func literal(_ expression: Expression) -> Value? {
        switch expression {
        case .number(let value):
            return .number(value)
        case .stringLiteral(let text):
            return .string(text)
        case .unaryMinus(.number(let value)):
            return .number(-value)
        case .arrayLiteral(let items):
            var values: [Value] = []
            values.reserveCapacity(items.count)
            for item in items {
                guard let value = literal(item) else { return nil }
                values.append(value)
            }
            return .array(values)
        case .mapLiteral(let entries):
            var folded: [MapEntry] = []
            folded.reserveCapacity(entries.count)
            for entry in entries {
                guard let value = literal(entry.value) else { return nil }
                folded.append(MapEntry(key: entry.key, value: value))
            }
            return .map(folded)
        case .lambda(let parameters, let body):
            // Persisted lambdas come back capture-free (captured locals
            // can't serialize); globals keep resolving at call time.
            return .function(FunctionValue(kind: .lambda(parameters: parameters, body: body),
                                           captures: [:]))
        case .variable(let name) where FunctionRegistry.standard.contains(name: name):
            // A persisted builtin reference ("f = abs" saved as "abs").
            // References to USER functions can't fold here — they load
            // separately — and are dropped; lambdas cover that need.
            return .function(FunctionValue(kind: .builtin(name)))
        default:
            return nil
        }
    }
}

/// A callable value. Bare names stay symbolic (re-resolved at call time, so
/// `f = double` then redefining `double` follows the new definition); lambdas
/// carry their AST plus captured locals.
public struct FunctionValue: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        /// A registry builtin, by name.
        case builtin(String)
        /// A user-defined function (resolved snapshot for display; calls
        /// re-resolve by name).
        case user(name: String)
        /// `x -> x * 2` — parameters + body, with captured locals.
        case lambda(parameters: [String], body: Expression)
    }

    public let kind: Kind
    /// Locals visible where a lambda was created (closure-by-value).
    /// Always empty for named references.
    public let captures: [String: Value]

    init(kind: Kind, captures: [String: Value] = [:]) {
        self.kind = kind
        self.captures = captures
    }
}

extension FunctionValue: CustomStringConvertible {
    /// Named references print as the name (re-parses to the same reference);
    /// lambdas print re-parseable source.
    public var description: String {
        switch kind {
        case .builtin(let name), .user(let name):
            return name
        case .lambda(let parameters, let body):
            return "(\(parameters.joined(separator: ", "))) -> \(body.sourceText)"
        }
    }
}
