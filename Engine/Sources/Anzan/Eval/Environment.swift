/// Session state: user variables plus the implicit `ans`.
/// Built-in constants live here too, shadowed from assignment by the parser's
/// reserved-name check.
///
/// A reference type on purpose: evaluation can re-enter the calculator
/// (a cell formula resolving `A:1` evaluates another formula against the same
/// session), and a shared class avoids overlapping-inout exclusivity traps.
///
/// Named to avoid colliding with SwiftUI's `@Environment` in files that
/// import both frameworks.
public final class EvaluationEnvironment {
    private var variables: [String: Value] = [:]

    /// Result of the most recent successful evaluation.
    public internal(set) var ans: Value = .number(.zero)

    /// Bumped on every variable/function mutation (not on `ans`). Lets
    /// callers detect "did this evaluation change session state?" by
    /// comparing two Ints instead of snapshotting dictionaries.
    public private(set) var changeCount = 0

    public init() {}

    /// The namespace whose body is currently evaluating, so a namespaced member
    /// resolves its siblings unqualified (`Bits::area` calls `perimeter`, finds
    /// `Bits::perimeter`). A stack: nested/cross-namespace calls push their own
    /// home, and a plain global function pushes `nil` so it can't see a caller's
    /// siblings. Transient — empty outside function-body evaluation. Mirrors the
    /// cell `ResolutionContext`'s current-sheet stack.
    private var namespaceContext: [String?] = []
    var currentNamespace: String? { namespaceContext.last ?? nil }
    func enterNamespace(_ namespace: String?) { namespaceContext.append(namespace) }
    func leaveNamespace() { if !namespaceContext.isEmpty { namespaceContext.removeLast() } }

    public subscript(name: String) -> Value? {
        get {
            switch name.lowercased() {
            case "ans": return ans
            case "pi", "π": return .number(Constants.pi)
            case "tau", "τ": return .number(Constants.tau)
            case "e": return .number(Constants.e)
            case "true": return .number(.one)
            case "false": return .number(.zero)
            case "json": return Constants.json
            case "rounding": return Constants.rounding
            default: return variables[name]
            }
        }
        set {
            if variables[name] != newValue {
                variables[name] = newValue
                changeCount += 1
            }
        }
    }

    /// User-defined variables, for display in the UI (v2 sidebar).
    public var userVariables: [String: Value] { variables }

    /// Replaces all user variables wholesale — used when opening a workbook
    /// (variables otherwise only enter one at a time through assignment).
    public func replaceUserVariables(_ newVariables: [String: Value]) {
        variables = newVariables
        changeCount += 1
    }

    // MARK: User-defined functions

    /// Keyed by lowercased name — function calls are case-insensitive,
    /// matching the built-in registry. Each name holds a list of OVERLOADS:
    /// at most one fully-untyped definition (redefinition replaces it, as
    /// before) plus any number of typed definitions, distinguished by their
    /// parameter type signature (typed dispatch).
    private var functions: [String: [UserFunction]] = [:]

    /// A single representative definition for the name (the first) — for
    /// "does a function named X exist", signature display, man(), etc. Typed
    /// dispatch uses `overloads(named:)`.
    public func function(named name: String) -> UserFunction? {
        functions[name.lowercased()]?.first
    }

    /// All overloads for a name, in definition order — used by call dispatch.
    public func overloads(named name: String) -> [UserFunction] {
        functions[name.lowercased()] ?? []
    }

    /// Defining over your own function is allowed (iteration); collisions
    /// with built-ins are rejected upstream in the evaluator. A new definition
    /// replaces any existing one with the SAME dispatch signature (untyped
    /// defs share one slot — redefinition replaces, preserving prior
    /// single-definition behavior); differing typed signatures coexist.
    func define(_ function: UserFunction) {
        let key = function.name.lowercased()
        var list = functions[key] ?? []
        list.removeAll { $0.dispatchSignature == function.dispatchSignature }
        list.append(function)
        functions[key] = list
        changeCount += 1
    }

    /// Records the original input line for the MOST RECENTLY defined overload
    /// of the name (the one just defined) — for workbook serialization.
    func setFunctionSource(_ source: String, for name: String) {
        let key = name.lowercased()
        guard var list = functions[key], !list.isEmpty else { return }
        list[list.count - 1].source = source
        functions[key] = list
    }

    /// Name → a representative definition (the first overload). Lossy for
    /// names with several typed overloads; `allUserFunctions` is the complete
    /// list.
    public var userFunctions: [String: UserFunction] {
        functions.compactMapValues { $0.first }
    }

    /// Every user function, all overloads — the complete set.
    public var allUserFunctions: [UserFunction] {
        functions.values.flatMap { $0 }
    }

    public func replaceUserFunctions(_ newFunctions: [String: UserFunction]) {
        functions = newFunctions.reduce(into: [:]) { result, entry in
            result[entry.key.lowercased(), default: []].append(entry.value)
        }
        changeCount += 1
    }

    // MARK: User-declared data types

    /// Keyed by lowercased name — constructor calls are case-insensitive,
    /// like functions (with which they share the call namespace; the
    /// evaluator rejects cross-collisions).
    private var dataTypes: [String: DataType] = [:]

    public func dataType(named name: String) -> DataType? {
        dataTypes[name.lowercased()]
    }

    /// Redeclaring your own type is allowed (iteration); collisions with
    /// built-ins and functions are rejected upstream in the evaluator.
    func define(_ dataType: DataType) {
        dataTypes[dataType.name.lowercased()] = dataType
        changeCount += 1
    }

    /// Records the original input line for workbook serialization (and the
    /// trailing `# doc comment` riding on it).
    func setDataTypeSource(_ source: String, for name: String) {
        dataTypes[name.lowercased()]?.source = source
    }

    public var userDataTypes: [String: DataType] {
        dataTypes
    }

    public func replaceUserDataTypes(_ newTypes: [String: DataType]) {
        dataTypes = newTypes
        changeCount += 1
    }

    // MARK: Namespace imports (docs/MODULES.md 2b)

    /// Imported namespaces, in import order — their members are reachable
    /// unqualified. Persisted in the workbook (restored after namespaces).
    private var imports: [String] = []
    public var importedNamespaces: [String] { imports }

    /// The source line of each `namespace … { … }` evaluated, in order —
    /// replayed on workbook open to re-register the namespace's members (which
    /// otherwise persist as empty-source qualified entries). Reopening appends,
    /// so replay reconstructs the accumulated namespace.
    private var namespaceSourceLines: [String] = []
    public var namespaceSources: [String] { namespaceSourceLines }
    func recordNamespaceSource(_ source: String) { namespaceSourceLines.append(source); changeCount += 1 }
    public func clearNamespaceSources() { namespaceSourceLines.removeAll() }

    /// The simple member names (types + functions) declared in a namespace —
    /// for the import conflict check.
    func memberNames(ofNamespace namespace: String) -> [String] {
        let prefix = namespace.lowercased() + "::"
        // Derive from the values' names (original case), not the lowercased keys.
        var names: [String] = []
        for type in dataTypes.values where type.name.lowercased().hasPrefix(prefix) {
            names.append(String(type.name.dropFirst(prefix.count)))
        }
        for list in functions.values {
            for function in list where function.name.lowercased().hasPrefix(prefix) {
                names.append(String(function.name.dropFirst(prefix.count)))
            }
        }
        return names
    }

    /// Record an import (idempotent — re-importing is a no-op).
    func addImport(_ namespace: String) {
        guard !imports.contains(where: { $0.lowercased() == namespace.lowercased() }) else { return }
        imports.append(namespace)
        changeCount += 1
    }

    public func clearImports() { imports.removeAll() }

    /// Resolve an unqualified name through the imports → its qualified form when
    /// an import provides it as a function or type. The import conflict check
    /// keeps this unambiguous (no two imports, and no global, share a name).
    func importedName(_ name: String) -> String? {
        guard !name.contains("::") else { return nil }
        for namespace in imports {
            let qualified = "\(namespace)::\(name)"
            if function(named: qualified) != nil || dataType(named: qualified) != nil {
                return qualified
            }
        }
        return nil
    }
}

/// A user-defined function: `f(x, y) = body`. The body is the parsed AST
/// (composable — it may call other user functions, resolved at call time);
/// `source` keeps the original definition line for saving into workbooks —
/// including any trailing `# doc comment`, which is how user documentation
/// persists with zero extra storage.
public struct UserFunction: Equatable, Sendable {
    public let name: String
    public let parameters: [Parameter]
    public let body: Expression
    public internal(set) var source: String

    /// Package, not public: only the evaluator and the hosting layer's λ
    /// cells (Spreadsheet.definedFunction) build these.
    package init(name: String, parameters: [Parameter], body: Expression, source: String) {
        self.name = name
        self.parameters = parameters
        self.body = body
        self.source = source
    }

    /// The trailing `# …` comment of the definition, if any — the user's own
    /// documentation, shown by man()/the reference window.
    public var documentation: String? {
        guard let hash = source.firstIndex(of: "#") else { return nil }
        let text = source[source.index(after: hash)...].trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : text
    }

    /// Display form: `f(x, y)` — or `dist(p: Point)` when params are typed.
    public var signature: String {
        "\(name)(\(parameters.map(\.rendered).joined(separator: ", ")))"
    }

    /// The overload's dispatch key: `nil` when every parameter is untyped (all
    /// such definitions of a name share one slot, so redefinition replaces);
    /// otherwise the parameter type sequence, so differing typed signatures
    /// coexist as overloads.
    var dispatchSignature: [TypeAnnotation?]? {
        parameters.contains { $0.type != nil } ? parameters.map(\.type) : nil
    }

    /// True if this definition participates in typed dispatch (any param typed).
    public var isTyped: Bool {
        parameters.contains { $0.type != nil }
    }
}

/// Constants at well past working precision (60 digits).
enum Constants {
    static let pi = BigDecimal(
        string: "3.14159265358979323846264338327950288419716939937510582097494")!
    static let tau = BigDecimal(
        string: "6.28318530717958647692528676655900576839433879875021164194989")!
    static let e = BigDecimal(
        string: "2.71828182845904523536028747135266249775724709369995957496697")!

    /// toJson's options namespace: `Json.Pretty` / `Json.Compact` — named
    /// constants instead of a magic boolean (user decision). They're plain
    /// string values riding in a constant map, so `toJson(x, "pretty")`
    /// works too and the map needs no new machinery.
    static let json: Value = .map([
        Value.MapEntry(key: "Pretty", value: .string("pretty")),
        Value.MapEntry(key: "Compact", value: .string("compact")),
    ])

    /// Decimal()'s rounding namespace: `Rounding.Bankers` / `Rounding.HalfUp` —
    /// named constants instead of a magic string (same pattern as `Json`). Plain
    /// string values in a constant map, so `Decimal(x, p, s, "halfUp")` works too.
    static let rounding: Value = .map([
        Value.MapEntry(key: "Bankers", value: .string("bankers")),
        Value.MapEntry(key: "HalfUp", value: .string("halfUp")),
    ])
}
