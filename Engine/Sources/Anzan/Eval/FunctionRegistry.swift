/// Where a function appears in the reference window.
public enum FunctionCategory: String, CaseIterable, Sendable {
    case core = "Core & Algebra"
    case logic = "Logic"
    case trig = "Trigonometry"
    case finance = "Finance"
    case dates = "Dates"
    case accounting = "Accounting"
    case stats = "Statistics"
    case data = "Data & Text"
    case programmer = "Programmer"
    case controls = "Controls"
}

/// A built-in function: arity contract, implementation, AND documentation.
/// The doc fields are deliberately required — a function cannot be registered
/// without a signature, summary, and at least one example, and
/// DocumentationTests evaluates every example, so the reference window can
/// never drift from the registry.
struct BuiltinFunction: Sendable {
    /// Applies a function value to arguments — how higher-order builtins
    /// call back into the evaluator (which owns environment + depth).
    typealias Applier = (Value, [Value]) throws -> Value

    /// Most builtins are numeric: array arguments flatten in place exactly
    /// like cell ranges (`sum(arr)` ≡ `sum(A:1..A:9)`), and arity is checked
    /// AFTER flattening. Value-level builtins (len, keys, …) see structures
    /// as-is. Higher-order builtins (map, filter, reduce) additionally
    /// receive the applier.
    enum Implementation: Sendable {
        case numeric(@Sendable ([BigDecimal]) throws -> BigDecimal)
        case values(@Sendable ([Value]) throws -> Value)
        case higherOrder(@Sendable ([Value], Applier) throws -> Value)
    }

    let name: String
    let category: FunctionCategory
    let signature: String
    let summary: String
    let examples: [String]
    /// Accepted argument counts, e.g. `1...1`, `1...2`, `2...Int.max` (variadic).
    let arity: ClosedRange<Int>
    let implementation: Implementation

    /// The numeric form — what almost every existing registration uses.
    init(name: String, category: FunctionCategory, signature: String,
         summary: String, examples: [String], arity: ClosedRange<Int>,
         apply: @escaping @Sendable ([BigDecimal]) throws -> BigDecimal) {
        self.name = name
        self.category = category
        self.signature = signature
        self.summary = summary
        self.examples = examples
        self.arity = arity
        self.implementation = .numeric(apply)
    }

    /// The structure-aware form (len, first, keys, concat, …).
    init(name: String, category: FunctionCategory, signature: String,
         summary: String, examples: [String], arity: ClosedRange<Int>,
         applyValues: @escaping @Sendable ([Value]) throws -> Value) {
        self.name = name
        self.category = category
        self.signature = signature
        self.summary = summary
        self.examples = examples
        self.arity = arity
        self.implementation = .values(applyValues)
    }

    /// The higher-order form (map, filter, reduce).
    init(name: String, category: FunctionCategory, signature: String,
         summary: String, examples: [String], arity: ClosedRange<Int>,
         applyHigherOrder: @escaping @Sendable ([Value], Applier) throws -> Value) {
        self.name = name
        self.category = category
        self.signature = signature
        self.summary = summary
        self.examples = examples
        self.arity = arity
        self.implementation = .higherOrder(applyHigherOrder)
    }

    /// Human-readable arity for error messages: "1", "1 to 2", "at least 2".
    var arityDescription: String {
        if arity.lowerBound == arity.upperBound { return "\(arity.lowerBound)" }
        if arity.upperBound == Int.max { return "at least \(arity.lowerBound)" }
        return "\(arity.lowerBound) to \(arity.upperBound)"
    }
}

/// Case-insensitive lookup of every built-in function.
package struct FunctionRegistry: Sendable {
    private var functions: [String: BuiltinFunction] = [:]

    package static let standard: FunctionRegistry = {
        var registry = FunctionRegistry()
        registry.register(coreFunctions)
        registry.register(trigFunctions)
        registry.register(financeFunctions)
        registry.register(accountingFunctions)
        registry.register(statsFunctions)
        registry.register(dateFunctions)
        registry.register(dataFunctions)
        registry.register(programmerFunctions)
        registry.register(controlFunctions)
        return registry
    }()

    private mutating func register(_ list: [BuiltinFunction]) {
        for function in list {
            assert(functions[function.name.lowercased()] == nil,
                   "duplicate function \(function.name)")
            functions[function.name.lowercased()] = function
        }
    }

    package func contains(name: String) -> Bool {
        functions[name.lowercased()] != nil
    }

    /// Looks up, checks arity, and applies. Numeric builtins flatten array
    /// arguments first (the structure analogue of range expansion), so the
    /// arity check sees the flattened count — `max(arr)` works like
    /// `max(A:1..A:9)`. The applier comes from the evaluator; higher-order
    /// builtins use it to invoke their function arguments.
    func call(name: String, arguments: [Value],
              applier: @escaping BuiltinFunction.Applier = { _, _ in
                  throw EngineError.domainError(message: "functions can't be applied here")
              }) throws -> Value {
        guard let function = functions[name.lowercased()] else {
            throw EngineError.unknownFunction(name: name)
        }
        switch function.implementation {
        case .numeric(let apply):
            var numbers: [BigDecimal] = []
            numbers.reserveCapacity(arguments.count)
            for argument in arguments {
                numbers.append(contentsOf: try argument.flattenedNumbers(for: function.name))
            }
            guard function.arity.contains(numbers.count) else {
                throw EngineError.arityMismatch(function: function.name,
                                                expected: function.arityDescription,
                                                got: numbers.count)
            }
            return .number(try apply(numbers))

        case .values(let apply):
            guard function.arity.contains(arguments.count) else {
                throw EngineError.arityMismatch(function: function.name,
                                                expected: function.arityDescription,
                                                got: arguments.count)
            }
            return try apply(arguments)

        case .higherOrder(let apply):
            guard function.arity.contains(arguments.count) else {
                throw EngineError.arityMismatch(function: function.name,
                                                expected: function.arityDescription,
                                                got: arguments.count)
            }
            return try apply(arguments, applier)
        }
    }

    /// All function names, for UI listing/autocomplete.
    var names: [String] {
        functions.values.map(\.name).sorted()
    }

    /// Every registered function, for the reference window.
    var all: [BuiltinFunction] {
        functions.values.sorted { $0.name.lowercased() < $1.name.lowercased() }
    }

    func function(named name: String) -> BuiltinFunction? {
        functions[name.lowercased()]
    }
}
