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

    BuiltinFunction(
        name: "Decimal", category: .accounting,
        signature: "Decimal(value[, [precision,] scale[, rounding]])",
        summary: "A fixed-precision decimal — SQL DECIMAL(p,s): at most `precision` significant digits, exactly `scale` fractional. Rounds to scale; exceeding the precision is an error (never silent). Decimal(10.5, 5, 2) → 10.50. Short forms: Decimal(value) captures the value exactly at max precision (1000), and Decimal(value, scale) pins the scale with precision defaulting to max. The optional last arg of the full form is the rounding mode: Rounding.Bankers (default) or Rounding.HalfUp.",
        examples: ["Decimal(0.5)", "Decimal(0.5, 2)", "Decimal(10.5, 5, 2)", "Decimal(1.005, 5, 2, Rounding.HalfUp)"],
        arity: 1...4,
        applyValues: { try makeFixedDecimal($0) }),
]

/// Builds a `Value.fixedDecimal` for the `Decimal` constructor. The arity drives
/// the shape:
///   Decimal(value)                         — scale from the value, precision = max
///   Decimal(value, scale)                  — that scale, precision = max
///   Decimal(value, precision, scale)       — both declared
///   Decimal(value, precision, scale, mode) — + rounding mode
private func makeFixedDecimal(_ arguments: [Value]) throws -> Value {
    let value = try arguments[0].asNumber(for: "Decimal's value")
    let precision: Int
    let scale: Int
    var rounding: DecimalRounding = .bankers
    switch arguments.count {
    case 1:
        // Default: capture the value exactly (its own decimal places) with the
        // max precision — lossless, and roomy enough that ordinary arithmetic
        // won't overflow. The big precision is hidden when it recalls.
        precision = FixedDecimal.maxPrecision
        scale = max(0, -value.exponent)
    case 2:
        scale = try requireInt(arguments[1].asNumber(for: "Decimal scale"), "Decimal scale")
        precision = FixedDecimal.maxPrecision
    default: // 3 or 4
        precision = try requireInt(arguments[1].asNumber(for: "Decimal precision"), "Decimal precision")
        scale = try requireInt(arguments[2].asNumber(for: "Decimal scale"), "Decimal scale")
        if arguments.count == 4 {
            guard case .string(let mode) = arguments[3] else {
                throw EngineError.domainError(
                    message: "Decimal's 4th argument is the rounding mode — Rounding.Bankers or Rounding.HalfUp")
            }
            switch mode.lowercased() {
            case "bankers": rounding = .bankers
            case "halfup": rounding = .halfUp
            default:
                throw EngineError.domainError(
                    message: "unknown rounding '\(mode)' — use Rounding.Bankers or Rounding.HalfUp")
            }
        }
    }
    return .fixedDecimal(try FixedDecimal(value: value, precision: precision, scale: scale, rounding: rounding))
}
