/// Structure & text functions — the Value-aware builtins. Unlike numeric
/// functions, these do NOT flatten array arguments (len([1, 2]) must see the
/// array, not two numbers).
let dataFunctions: [BuiltinFunction] = [
    BuiltinFunction(
        name: "len",
        category: .data,
        signature: "len(value)",
        summary: "Number of elements in an array or map, or characters in a string.",
        examples: ["len([1, 2, 3])", "len({name: \"Ada\", age: 36})", "len(\"hello\")"],
        arity: 1...1,
        applyValues: { arguments in
            switch arguments[0] {
            case .array(let items): return .number(BigDecimal(items.count))
            case .map(let entries): return .number(BigDecimal(entries.count))
            case .record(let record): return .number(BigDecimal(record.entries.count))
            case .string(let text): return .number(BigDecimal(text.count))
            case .number, .fixedInt, .fixedDecimal, .money, .grouped, .function, .host:
                throw EngineError.domainError(message: "len() works on arrays, maps, and strings")
            }
        }),

    BuiltinFunction(
        name: "first",
        category: .data,
        signature: "first(array)",
        summary: "The first element of an array (index 0).",
        examples: ["first([5, 6, 7])"],
        arity: 1...1,
        applyValues: { arguments in
            guard case .array(let items) = arguments[0] else {
                throw EngineError.domainError(
                    message: "first() works on arrays, got \(arguments[0].kindName)")
            }
            guard let first = items.first else {
                throw EngineError.domainError(message: "first() of an empty array")
            }
            return first
        }),

    BuiltinFunction(
        name: "last",
        category: .data,
        signature: "last(array)",
        summary: "The last element of an array.",
        examples: ["last([5, 6, 7])"],
        arity: 1...1,
        applyValues: { arguments in
            guard case .array(let items) = arguments[0] else {
                throw EngineError.domainError(
                    message: "last() works on arrays, got \(arguments[0].kindName)")
            }
            guard let last = items.last else {
                throw EngineError.domainError(message: "last() of an empty array")
            }
            return last
        }),

    BuiltinFunction(
        name: "keys",
        category: .data,
        signature: "keys(map)",
        summary: "A map's keys, as an array of strings (insertion order).",
        examples: ["keys({name: \"Ada\", age: 36})"],
        arity: 1...1,
        applyValues: { arguments in
            switch arguments[0] {
            case .map(let entries):
                return .array(entries.map { .string($0.key) })
            case .record(let record):
                return .array(record.entries.map { .string($0.key) })
            default:
                throw EngineError.domainError(
                    message: "keys() works on maps, got \(arguments[0].kindName)")
            }
        }),

    BuiltinFunction(
        name: "values",
        category: .data,
        signature: "values(map)",
        summary: "A map's values, as an array (insertion order).",
        examples: ["values({a: 1, b: 2})", "sum(values({a: 1, b: 2}))"],
        arity: 1...1,
        applyValues: { arguments in
            switch arguments[0] {
            case .map(let entries):
                return .array(entries.map(\.value))
            case .record(let record):
                return .array(record.entries.map(\.value))
            default:
                throw EngineError.domainError(
                    message: "values() works on maps, got \(arguments[0].kindName)")
            }
        }),

    BuiltinFunction(
        name: "map",
        category: .data,
        signature: "map(f, array)",
        summary: "Applies a function to every element: pass a lambda (x -> x * 2) or a function name (yours or a built-in). Returns the transformed array.",
        examples: ["map(x -> x * 2, [1, 2, 3])", "map(sqrt, [1, 4, 9])"],
        arity: 2...2,
        applyHigherOrder: { arguments, apply in
            guard case .array(let items) = arguments[1] else {
                throw EngineError.domainError(
                    message: "map() wants (function, array) — got \(arguments[1].kindName) second")
            }
            return .array(try items.map { try apply(arguments[0], [$0]) })
        }),

    BuiltinFunction(
        name: "filter",
        category: .data,
        signature: "filter(predicate, array)",
        summary: "Keeps the elements where the predicate returns nonzero (comparisons return 1/0, so x -> x > 10 reads naturally).",
        examples: ["filter(x -> x > 1, [1, 2, 3])", "filter(x -> mod(x, 2) == 0, [1, 2, 3, 4])"],
        arity: 2...2,
        applyHigherOrder: { arguments, apply in
            guard case .array(let items) = arguments[1] else {
                throw EngineError.domainError(
                    message: "filter() wants (predicate, array) — got \(arguments[1].kindName) second")
            }
            return .array(try items.filter { item in
                try !apply(arguments[0], [item])
                    .asNumber(for: "the filter() predicate's result").isZero
            })
        }),

    BuiltinFunction(
        name: "reduce",
        category: .data,
        signature: "reduce(f, array, initial)",
        summary: "Folds an array left-to-right: f(accumulator, element), starting from `initial`. reduce((a, b) -> a + b, arr, 0) is sum.",
        examples: ["reduce((a, b) -> a + b, [1, 2, 3], 0)", "reduce((a, b) -> a * b, [1, 2, 3, 4], 1)"],
        arity: 3...3,
        applyHigherOrder: { arguments, apply in
            guard case .array(let items) = arguments[1] else {
                throw EngineError.domainError(
                    message: "reduce() wants (function, array, initial) — got \(arguments[1].kindName) second")
            }
            var accumulator = arguments[2]
            for item in items {
                accumulator = try apply(arguments[0], [accumulator, item])
            }
            return accumulator
        }),

    BuiltinFunction(
        name: "concat",
        category: .data,
        signature: "concat(a, b, …)",
        summary: "Joins values into one string (numbers render plainly) — or joins arrays into one array when every argument is an array.",
        examples: ["concat(\"Q\", 1)", "concat([1, 2], [3])"],
        arity: 2...Int.max,
        applyValues: { arguments in
            // All arrays → array concatenation; otherwise string concatenation.
            var joined: [Value] = []
            var allArrays = true
            for argument in arguments {
                if case .array(let items) = argument {
                    joined.append(contentsOf: items)
                } else {
                    allArrays = false
                    break
                }
            }
            if allArrays { return .array(joined) }
            return .string(arguments.map(\.displayText).joined())
        }),

    BuiltinFunction(
        name: "toJson",
        category: .data,
        signature: "toJson(value, option?)",
        summary: "Renders a value as JSON — pretty-printed by default (you're usually reading it); pass Json.Compact for the one-line interchange form. The options are plain strings, so \"compact\" works too. Boolean fields of data types come out as true/false; numbers keep their full precision.",
        examples: ["toJson({name: \"Ada\", age: 36})", "toJson([1, 2, 3], Json.Compact)"],
        arity: 1...2,
        applyValues: { arguments in
            var pretty = true // reading is the common case; compact is opt-in
            if arguments.count == 2 {
                guard case .string(let option) = arguments[1] else {
                    throw EngineError.domainError(message:
                        "toJson's option is Json.Pretty or Json.Compact — got \(arguments[1].kindName)")
                }
                switch option.lowercased() {
                case "pretty": pretty = true
                case "compact": pretty = false
                default:
                    throw EngineError.domainError(message:
                        "unknown toJson option \"\(option)\" — use Json.Pretty or Json.Compact")
                }
            }
            return .string(try arguments[0].jsonText(pretty: pretty))
        }),

    BuiltinFunction(
        name: "fromJson",
        category: .data,
        signature: "fromJson(text)",
        summary: "Parses JSON text into a value — objects become maps, arrays arrays, true/false 1/0, and numbers EXACT decimals (parsed at full precision, never through floating point). Type the result with a constructor: Person(fromJson(t)). JSON null is refused — Anzan has no null.",
        examples: ["fromJson(\"[1, 2, 3]\")", "fromJson(toJson({a: 1})).a"],
        arity: 1...1,
        applyValues: { arguments in
            guard case .string(let text) = arguments[0] else {
                throw EngineError.domainError(
                    message: "fromJson() wants JSON text, got \(arguments[0].kindName)")
            }
            return try JSONParser.parse(text)
        }),

    // The range→array bridge: ranges expand IN PLACE as arguments, so
    // list(A:1..A:9) collects the expansion into one array — which is what
    // unlocks filter/map/reduce over cells.
    BuiltinFunction(
        name: "list",
        category: .data,
        signature: "list(x, y, …)",
        summary: "Collects its arguments into one array. The reason it exists: ranges expand into arguments, so list(A:1..A:9) turns a range into an array — then map/filter/reduce apply.",
        examples: ["list(1, 2, 3)", "sum(filter(x -> x > 1, list(1, 2, 3)))"],
        arity: 0...Int.max,
        applyValues: { arguments in
            .array(arguments)
        }),

    BuiltinFunction(
        name: "sort",
        category: .data,
        signature: "sort(array)",
        summary: "Sorts an array ascending — all numbers, or all strings (lexicographic).",
        examples: ["sort([3, 1, 2])", "sort([\"pear\", \"fig\"])"],
        arity: 1...1,
        applyValues: { arguments in
            guard case .array(let items) = arguments[0] else {
                throw EngineError.domainError(
                    message: "sort() works on arrays, got \(arguments[0].kindName)")
            }
            var numbers: [BigDecimal] = []
            var texts: [String] = []
            for item in items {
                if case .number(let n) = item { numbers.append(n) }
                if case .string(let s) = item { texts.append(s) }
            }
            if numbers.count == items.count {
                return .array(numbers.sorted().map(Value.number))
            }
            if texts.count == items.count {
                return .array(texts.sorted().map(Value.string))
            }
            throw EngineError.domainError(
                message: "sort() needs all numbers or all strings")
        }),

    BuiltinFunction(
        name: "unique",
        category: .data,
        signature: "unique(array)",
        summary: "Drops duplicate elements (deep equality), keeping first-seen order.",
        examples: ["unique([3, 1, 3, 2, 1])", "len(unique([1, 1, 1]))"],
        arity: 1...1,
        applyValues: { arguments in
            guard case .array(let items) = arguments[0] else {
                throw EngineError.domainError(
                    message: "unique() works on arrays, got \(arguments[0].kindName)")
            }
            var seen: [Value] = []
            for item in items where !seen.contains(item) {
                seen.append(item)
            }
            return .array(seen)
        }),

    BuiltinFunction(
        name: "reverse",
        category: .data,
        signature: "reverse(value)",
        summary: "Reverses an array — or a string, character by character.",
        examples: ["reverse([1, 2, 3])", "reverse(\"abc\")"],
        arity: 1...1,
        applyValues: { arguments in
            switch arguments[0] {
            case .array(let items): return .array(items.reversed())
            case .string(let text): return .string(String(text.reversed()))
            default:
                throw EngineError.domainError(
                    message: "reverse() works on arrays and strings, got \(arguments[0].kindName)")
            }
        }),

    BuiltinFunction(
        name: "seq",
        category: .data,
        signature: "seq(from, to, step = 1)",
        summary: "An array counting from `from` to `to` (inclusive when the step lands on it). Step defaults to 1, or -1 when counting down.",
        examples: ["seq(1, 5)", "seq(10, 0, -2)", "sum(map(x -> x^2, seq(1, 10)))"],
        arity: 2...3,
        applyValues: { arguments in
            let from = try arguments[0].asNumber(for: "seq's start")
            let to = try arguments[1].asNumber(for: "seq's end")
            let step: BigDecimal
            if arguments.count > 2 {
                step = try arguments[2].asNumber(for: "seq's step")
                guard !step.isZero else {
                    throw EngineError.domainError(message: "seq's step can't be 0")
                }
            } else {
                step = from <= to ? .one : -BigDecimal.one
            }
            var values: [Value] = []
            var current = from
            while step.isNegative ? current >= to : current <= to {
                values.append(.number(current))
                guard values.count < 100_000 else {
                    throw EngineError.domainError(message: "seq spans more than 100,000 values")
                }
                current = current + step
            }
            return .array(values)
        }),
]
