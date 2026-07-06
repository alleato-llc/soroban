import BigInt

/// How a `decimal` rounds its value to scale — a constructor option, carried
/// with the value so it governs every later rounding too. See docs/DECIMAL.md.
public enum DecimalRounding: String, Sendable {
    case bankers   // round half to even (the engine's standard, the default)
    case halfUp    // round half away from zero (Java HALF_UP)
}

/// A bounded, checked fixed-precision decimal — the payload of
/// `Value.fixedDecimal`, built by `Decimal(value, precision, scale[, Rounding.X])`.
/// SQL `DECIMAL(p, s)`: at most `precision` significant digits, exactly `scale`
/// fractional digits. The value is rounded to `scale`; exceeding `precision` is
/// an overflow error (the checked-range contract, like `int`/`uint`).
public struct FixedDecimal: Sendable, Equatable {
    /// The largest precision a Decimal may declare (matches PostgreSQL's declared
    /// NUMERIC ceiling). Since `scale <= precision`, this is also the maximum scale
    /// (1000, reached at precision 1000 with a pure-fraction value). The cap keeps
    /// the `10^precision` range check bounded and gives a coherent upper limit on
    /// declared digits.
    public static let maxPrecision = 1000

    public let value: BigDecimal   // already rounded to `scale` and within `precision`
    public let precision: Int
    public let scale: Int
    public let rounding: DecimalRounding

    public init(value rawValue: BigDecimal, precision: Int, scale: Int,
                rounding: DecimalRounding) throws(EngineError) {
        guard precision >= 1, precision <= Self.maxPrecision else {
            throw EngineError.domainError(
                message: "Decimal precision must be between 1 and \(Self.maxPrecision), got \(precision)")
        }
        guard scale >= 0, scale <= precision else {
            throw EngineError.domainError(
                message: "Decimal scale must be between 0 and the precision (\(precision)), got \(scale)")
        }
        let rounded = rounding == .halfUp
            ? rawValue.roundedHalfUp(toPlaces: scale)
            : rawValue.rounded(toPlaces: scale)
        // unscaled = rounded × 10^scale (an integer); must fit `precision` digits.
        let unscaled = rounded.significand * Integer.powerOfTen(rounded.exponent + scale)
        guard unscaled.magnitude < Integer.powerOfTen(precision) else {
            throw EngineError.domainError(
                message: "\(Self.text(rounded, scale: scale)) exceeds Decimal(\(precision), \(scale)) — more than \(precision) digits")
        }
        self.value = rounded
        self.precision = precision
        self.scale = scale
        self.rounding = rounding
    }

    /// e.g. "Decimal(5, 2)" — for `kindName` / error messages.
    public var typeName: String { "Decimal(\(precision), \(scale))" }

    /// The value padded to exactly `scale` fractional digits — "10.50", "0.05".
    public var text: String { Self.text(value, scale: scale) }

    private static func text(_ value: BigDecimal, scale: Int) -> String {
        let unscaled = value.significand * Integer.powerOfTen(value.exponent + scale)
        let negative = unscaled.sign == .minus
        var digits = unscaled.magnitude.description
        if scale > 0 {
            if digits.count <= scale {
                digits = String(repeating: "0", count: scale - digits.count + 1) + digits
            }
            let cut = digits.index(digits.endIndex, offsetBy: -scale)
            digits = String(digits[..<cut]) + "." + String(digits[cut...])
        }
        return (negative ? "-" : "") + digits
    }

    /// Canonical, re-parseable constructor spelling (restores by evaluation,
    /// like a record) — the SHORTEST form that round-trips. A max-precision,
    /// banker's-rounded value hides the precision (it's the default): the 1-arg
    /// `Decimal(0.5)` when the scale is the value's own, else the 2-arg
    /// `Decimal(0.50, 2)`. Everything else is the full form; the rounding arg
    /// appears only when non-default.
    public var description: String {
        if precision == Self.maxPrecision, rounding == .bankers {
            // The value's own number of decimal places (scale can only be ≥ this,
            // since the value was rounded to scale on construction).
            let naturalScale = max(0, -value.exponent)
            if scale == naturalScale { return "Decimal(\(text))" }
            return "Decimal(\(text), \(scale))"
        }
        let mode = rounding == .halfUp ? ", Rounding.HalfUp" : ""
        return "Decimal(\(text), \(precision), \(scale)\(mode))"
    }
}

// MARK: - Typed arithmetic (the mixing matrix + checked overflow)

extension FixedDecimal {
    /// True when fixed-precision arithmetic applies — either operand is a decimal.
    static func isInvolved(_ lhs: Value, _ rhs: Value) -> Bool {
        if case .fixedDecimal = lhs { return true }
        if case .fixedDecimal = rhs { return true }
        return false
    }

    /// `+ − × ÷` on fixed-precision operands (docs/DECIMAL.md): scale and
    /// precision promote to the widest; rounding never reconciles (mismatch →
    /// error); a plain `Number` is absorbed and rounded to the decimal's scale;
    /// the result is range-checked and **errors rather than wraps**.
    static func applyBinary(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        let type = try resolvedType(lhs, rhs)
        let a = try operand(lhs)
        let b = try operand(rhs)
        let raw: BigDecimal
        switch op {
        case .add: raw = a + b
        case .subtract: raw = a - b
        case .multiply: raw = a * b
        case .divide: raw = try a / b // BigDecimal division throws on a zero divisor
        case .modulo, .power:
            throw EngineError.domainError(
                message: "a fixed-precision decimal doesn't support \(op == .power ? "^ (power)" : "modulo") — convert it to a Number first")
        }
        return .fixedDecimal(try FixedDecimal(value: raw, precision: type.precision,
                                              scale: type.scale, rounding: type.rounding))
    }

    private static func resolvedType(_ lhs: Value, _ rhs: Value)
        throws -> (precision: Int, scale: Int, rounding: DecimalRounding) {
        switch (lhs, rhs) {
        case (.fixedDecimal(let a), .fixedDecimal(let b)):
            guard a.rounding == b.rounding else {
                throw EngineError.domainError(
                    message: "can't mix decimals with different rounding (\(a.rounding) and \(b.rounding)) — cast one")
            }
            return (max(a.precision, b.precision), max(a.scale, b.scale), a.rounding)
        case (.fixedDecimal(let a), _): return (a.precision, a.scale, a.rounding)
        case (_, .fixedDecimal(let b)): return (b.precision, b.scale, b.rounding)
        default:
            throw EngineError.domainError(message: "fixed-precision arithmetic with no decimal operand")
        }
    }

    /// An operand as an exact BigDecimal. A decimal uses its value; a plain
    /// Number is absorbed exactly (rounded to scale on the result). Anything else
    /// — including a fixed-width int — errors (cross-family; cast explicitly).
    private static func operand(_ value: Value) throws -> BigDecimal {
        switch value {
        case .fixedDecimal(let d): return d.value
        case .number(let n): return n
        default:
            throw EngineError.domainError(
                message: "can't combine \(value.kindName) with a fixed-precision decimal — cast it (e.g. Decimal(…))")
        }
    }
}
