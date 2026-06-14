/// The reference window's data model. Built-in functions document themselves
/// (the fields are required at registration and DocumentationTests evaluates
/// every example); special forms, operators, and constants are curated here;
/// user-defined functions are generated live from the environment.
public struct FunctionDoc: Identifiable, Hashable, Sendable {
    public let name: String
    public let signature: String
    public let summary: String
    public let examples: [String]

    public var id: String { name }

    public init(name: String, signature: String, summary: String, examples: [String]) {
        self.name = name
        self.signature = signature
        self.summary = summary
        self.examples = examples
    }
}

public struct DocCategory: Identifiable, Hashable, Sendable {
    public let title: String
    public let entries: [FunctionDoc]

    public var id: String { title }

    public init(title: String, entries: [FunctionDoc]) {
        self.title = title
        self.entries = entries
    }
}

extension Calculator {
    /// Everything the reference window shows, in display order.
    /// Instance method because "Your Functions" reads the live environment.
    public func documentation() -> [DocCategory] {
        var categories: [DocCategory] = []

        let userFunctions = environment.userFunctions.values
            .sorted { $0.name.lowercased() < $1.name.lowercased() }
        if !userFunctions.isEmpty {
            categories.append(DocCategory(
                title: "Your Functions",
                entries: userFunctions.map(Self.doc(for:))))
        }

        let userDataTypes = environment.userDataTypes.values
            .sorted { $0.name.lowercased() < $1.name.lowercased() }
        if !userDataTypes.isEmpty {
            categories.append(DocCategory(
                title: "Your Data Types",
                entries: userDataTypes.map(Self.doc(for:))))
        }

        categories.append(contentsOf: Self.builtinDocumentation)
        return categories
    }

    /// One function's documentation, for the autocomplete hint footer.
    /// Covers built-ins, special forms, the user's own functions, and
    /// sheet-scoped λ cells (their `# doc comment` rides in the source).
    public func documentation(for name: String) -> FunctionDoc? {
        if let builtin = FunctionRegistry.standard.function(named: name) {
            return FunctionDoc(name: builtin.name, signature: builtin.signature,
                               summary: builtin.summary, examples: builtin.examples)
        }
        if let special = Self.specialForms.first(where: { $0.name.lowercased() == name.lowercased() }) {
            return special
        }
        if let constant = Self.constants.first(where: { $0.name.lowercased() == name.lowercased() }) {
            return constant // man pi, man Json, …
        }
        if let op = Self.operators.first(where: { $0.name.lowercased() == name.lowercased() }) {
            return op // man modes, man arithmetic, man comparisons, …
        }
        if let scoped = scopedFunctionResolver?(name) {
            return Self.doc(for: scoped)
        }
        if let user = environment.function(named: name) {
            return Self.doc(for: user)
        }
        if let scopedType = scopedDataTypeResolver?(name) {
            return Self.doc(for: scopedType)
        }
        if let type = environment.dataType(named: name) {
            return Self.doc(for: type)
        }
        return nil
    }

    /// A user function's docs: its own `# doc comment` when present, with the
    /// definition line as a clickable example (clicking it lets you edit and
    /// redefine).
    private static func doc(for function: UserFunction) -> FunctionDoc {
        FunctionDoc(name: function.name,
                    signature: function.signature,
                    summary: function.documentation
                        ?? "Defined in this workbook. Add documentation with a trailing comment: \(function.name)(…) = … # what it does",
                    examples: [function.source])
    }

    /// A data type's docs — same `# doc comment` contract as functions; the
    /// declaration line is the clickable example.
    private static func doc(for type: DataType) -> FunctionDoc {
        FunctionDoc(name: type.name,
                    signature: type.declaration,
                    summary: type.documentation
                        ?? "Declared in this workbook. Construct with \(type.name)(\(type.fields.map { "\($0.name): …" }.joined(separator: ", "))).",
                    examples: [type.source])
    }

    /// Registry + curated entries, cached (registry content is static).
    public static let builtinDocumentation: [DocCategory] = {
        var categories: [DocCategory] = [
            DocCategory(title: "Special Forms", entries: specialForms),
        ]
        for kind in FunctionCategory.allCases {
            let entries = FunctionRegistry.standard.all
                .filter { $0.category == kind }
                .map { FunctionDoc(name: $0.name, signature: $0.signature,
                                   summary: $0.summary, examples: $0.examples) }
            if !entries.isEmpty {
                categories.append(DocCategory(title: kind.rawValue, entries: entries))
            }
        }
        categories.append(DocCategory(title: "Operators & Syntax", entries: operators))
        categories.append(DocCategory(title: "Constants", entries: constants))
        return categories
    }()

    static let specialForms: [FunctionDoc] = [
        FunctionDoc(
            name: "if",
            signature: "if(condition, then, else)",
            summary: "Returns `then` when the condition is nonzero, otherwise `else`. Only the taken branch is evaluated, so the other may divide by zero — or recurse: fact(n) = if(n <= 1, 1, n * fact(n - 1)).",
            examples: ["if(1 < 2, 10, 20)", "if(0, 1/0, 7)"]),
        FunctionDoc(
            name: "sigma",
            signature: "∑_i=1^10(term)   ·   ∑(x, y, …)",
            summary: "Summation. The subscript form re-evaluates the term with the index bound to each integer (type sigma_i=1^10(…) if ∑ is out of reach; compound bounds need parens: ∑_i=(n-1)^10(…)). A plain ∑(…) call simply sums its arguments.",
            examples: ["∑_i=1^10(i^2)", "∑(1, 2, 3)"]),
        FunctionDoc(
            name: "productForm",
            signature: "∏_i=1^5(term)   ·   ∏(x, y, …)",
            summary: "Product — ∑'s multiplicative sibling (type product_i=1^5(…)). ∏_i=1^n(i) is an exact factorial; ∏_i=1^n(1 + r) is compound growth.",
            examples: ["∏_i=1^5(i)", "∏(2, 3, 4)"]),
    ]

    static let operators: [FunctionDoc] = [
        FunctionDoc(
            name: "arithmetic",
            signature: "+  −  ×(*)  ÷(/)  ^  %",
            summary: "Exact decimal arithmetic — 0.1 + 0.2 is exactly 0.3. ^ is power (right-associative); postfix % is percent (3% → 0.03), and mod(x, y) is modulo. Typographic × ÷ − · paste fine. Implicit multiplication works: 2(3 + 4), 2x, 2π. In Programmer mode ^ and % — plus & | << >> ~ — read as bitwise/modulo instead; see man modes.",
            examples: ["0.1 + 0.2", "2^10", "3%", "2π"]),
        FunctionDoc(
            name: "comparisons",
            signature: "<  >  <=  >=  ==  !=   (≤ ≥ ≠)",
            summary: "Comparisons return 1 (true) or 0 (false) — feed them to if(). They can't chain; use and(a < b, b < c).",
            examples: ["2 < 3", "0.1 + 0.2 == 0.3"]),
        FunctionDoc(
            name: "assignment",
            signature: "x = expr   ·   f(a, b) = expr",
            summary: "Variables and custom functions. Functions compose and may recurse (via if); parameters shadow globals; built-in names are protected. Both are saved in workbooks.",
            examples: ["x = 12 * 80.5", "double(n) = n * 2"]),
        FunctionDoc(
            name: "cells",
            signature: "A:1   ·   A:1..B:9",
            summary: "Grid references — column letter, colon, 1-based row — usable in cells AND the calculation log. Ranges (rectangles allowed) expand inside functions; empty and text cells are skipped.",
            examples: ["sum(A:1..B:3)", "count(A:1..A:9)"]),
        FunctionDoc(
            name: "sqrtSign",
            signature: "√x",
            summary: "Prefix square root — binds like unary minus, so √2^2 = √(2²) = 2.",
            examples: ["√16", "√(2 + 2)"]),
        FunctionDoc(
            name: "strings",
            signature: "\"text\"   ·   +",
            summary: "Double-quoted string values (escapes: \\\" \\\\ \\n \\t). + concatenates as soon as either side is a string; == compares. In a cell, a formula that returns a string displays as text.",
            examples: ["\"Q\" + 1", "greeting = \"hello\""]),
        FunctionDoc(
            name: "arrays",
            signature: "[a, b, …]   ·   arr[0]",
            summary: "Array values — elements are any expressions and nest freely. Indexing is 0-based. Numeric functions accept arrays like ranges: sum(arr), max(arr). Arrays live in the log and in formulas; a cell can't display one.",
            examples: ["[1, 2, 3][0]", "sum([1, 2, 3])", "len([[1, 2], [3]])"]),
        FunctionDoc(
            name: "maps",
            signature: "{key: value, …}   ·   m.key   ·   m[\"key\"]",
            summary: "Maps hold named values, nest with arrays, and read via .key or [\"key\"] (keys are case-sensitive). Build records: person = {name: \"Ada\", age: 36}.",
            examples: ["{name: \"Ada\", age: 36}.age", "{a: 1, b: 2}[\"b\"]"]),
        FunctionDoc(
            name: "data types",
            signature: "data Person { name: String, age: Number, active: Boolean }",
            summary: "Declares a typed record (fields: Number, String, Boolean). Construct with named fields or from a map — never positionally. Instances read like maps (p.name), collect into arrays, and work with map/filter/reduce; toJson() keeps Boolean fields honest. In a cell, a plain declaration makes a sheet-scoped 𝑫 type.",
            examples: ["data Pt { x: Number, y: Number }",
                       "p = Pt(x: 3, y: 4)",
                       "sqrt(p.x^2 + p.y^2)"]),
        FunctionDoc(
            name: "lambdas",
            signature: "x -> expr   ·   (a, b) -> expr",
            summary: "Anonymous functions, for map/filter/reduce — or assign one: f = x -> x * 2, then f(3). A bare function name is a value too: map(sqrt, arr). Lambdas close over function parameters by value.",
            examples: ["map(x -> x ^ 2, [1, 2, 3])", "double = x -> x * 2"]),
        FunctionDoc(
            name: "modes",
            signature: ":mode normal · programmer · finance",
            summary: "Input/display DIALECTS for the calculation log. Normal (default): ^ is power, postfix % is percent, and bit ops are functions (bitAnd, bitOr, bitXor, bitShift, bitNot). Programmer: ^ is XOR, & AND, | OR, << >> shifts, % modulo, ~ NOT (Python precedence; power becomes pow). Finance ≈ Normal for now. A dialect only changes which glyphs you type and read — the stored formula is always canonical, so it never means two things. SWITCH: Settings → Mode (⌘,) or the input-bar mode icon, or type :mode programmer (or finance / normal) — the :mode command works in both the app log and the CLI. Grid cells are always Normal.",
            examples: ["5 ^ 3", "pow(2, 10)", "bitAnd(12, 10)"]),
    ]

    static let constants: [FunctionDoc] = [
        FunctionDoc(name: "pi", signature: "pi · π",
                    summary: "The circle constant, to 60 digits.", examples: ["2π", "sin(pi / 2)"]),
        FunctionDoc(name: "tau", signature: "tau · τ",
                    summary: "2π.", examples: ["τ ÷ π"]),
        FunctionDoc(name: "e", signature: "e",
                    summary: "Euler's number, to 60 digits.", examples: ["ln(e)"]),
        FunctionDoc(name: "ans", signature: "ans",
                    summary: "The result of the previous calculation in the log.", examples: ["1 + 1", "ans * 2"]),
        FunctionDoc(name: "true", signature: "true · false",
                    summary: "1 and 0 — the engine's truth values, matching what comparisons return.", examples: ["if(true, 10, 20)", "true == 1"]),
        FunctionDoc(name: "Json", signature: "Json.Pretty · Json.Compact",
                    summary: "Formatting options for toJson() — named constants instead of a magic flag. Pretty is the default; Json.Compact packs to one line. They're plain string values (\"pretty\" / \"compact\") carried in a constant map.",
                    examples: ["toJson({a: 1}, Json.Compact)", "Json.Pretty"]),
        FunctionDoc(name: "Rounding", signature: "Rounding.Bankers · Rounding.HalfUp",
                    summary: "Rounding modes for Decimal() — Bankers (round half to even, the default and the engine's standard) or HalfUp (round half away from zero). Named constants like Json; plain string values in a constant map.",
                    examples: ["Decimal(1.005, 5, 2, Rounding.HalfUp)", "Rounding.Bankers"]),
    ]
}
