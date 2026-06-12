/// Margin/markup conventions:
///   markup is relative to COST, margin is relative to PRICE.
let accountingFunctions: [BuiltinFunction] = [
    // markup(cost, pct) → selling price after marking cost up by pct percent.
    BuiltinFunction(name: "markup", category: .accounting,
                    signature: "markup(cost, pct)",
                    summary: "Selling price after marking cost up by pct percent (markup is relative to cost).",
                    examples: ["markup(80, 25)"],
                    arity: 2...2) { args in
        try args[0] * (BigDecimal.one + args[1] / BigDecimal(100))
    },

    // margin(price, cost) → gross margin as a percent of price.
    BuiltinFunction(name: "margin", category: .accounting,
                    signature: "margin(price, cost)",
                    summary: "Gross margin as a percent of price (margin is relative to price).",
                    examples: ["margin(100, 80)"],
                    arity: 2...2) { args in
        guard !args[0].isZero else {
            throw EngineError.domainError(message: "margin: price cannot be 0")
        }
        return try (args[0] - args[1]) / args[0] * BigDecimal(100)
    },

    // percentOf(part, whole) → part as a percent of whole.
    BuiltinFunction(name: "percentOf", category: .accounting,
                    signature: "percentOf(part, whole)",
                    summary: "part as a percentage of whole.",
                    examples: ["percentOf(30, 120)"],
                    arity: 2...2) { args in
        try args[0] / args[1] * BigDecimal(100)
    },

    // percentChange(old, new) → relative change as a percent.
    BuiltinFunction(name: "percentChange", category: .accounting,
                    signature: "percentChange(old, new)",
                    summary: "Relative change from old to new, as a percent.",
                    examples: ["percentChange(80, 100)"],
                    arity: 2...2) { args in
        guard !args[0].isZero else {
            throw EngineError.domainError(message: "percentChange: old value cannot be 0")
        }
        return try (args[1] - args[0]) / args[0] * BigDecimal(100)
    },
]
