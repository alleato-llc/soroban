import BigInt

/// Arbitrary-precision base-10 number: `significand × 10^exponent`.
///
/// Addition, subtraction, and multiplication are exact. Division and roots are
/// computed to `PrecisionContext.current` significant digits. Values are kept
/// normalized (no trailing zeros in the significand) so equality is structural.
public struct BigDecimal: Sendable {
    public private(set) var significand: BigInt
    public private(set) var exponent: Int

    public init(significand: BigInt, exponent: Int) {
        self.significand = significand
        self.exponent = exponent
        normalize()
    }

    public init(_ value: Int) {
        self.init(significand: BigInt(value), exponent: 0)
    }

    public static let zero = BigDecimal(0)
    public static let one = BigDecimal(1)

    public var isZero: Bool { significand.isZero }
    public var isNegative: Bool { significand.sign == .minus }

    /// True when the value has no fractional part.
    public var isInteger: Bool { exponent >= 0 }

    /// Strips trailing zeros from the significand into the exponent; zero gets exponent 0.
    private mutating func normalize() {
        if significand.isZero {
            exponent = 0
            return
        }
        while significand.isMultiple(of: 10) {
            significand /= 10
            exponent += 1
        }
    }

    /// Number of significant decimal digits in the significand.
    var digitCount: Int {
        if significand.isZero { return 1 }
        return String(significand.magnitude).count
    }
}

// MARK: - Precision context

/// Working precision for inexact operations (division, roots, transcendentals).
public enum PrecisionContext {
    /// Significant digits carried by inexact operations.
    @TaskLocal public static var current: Int = 50
}

// MARK: - Parsing

extension BigDecimal {
    /// Parses a literal: `123`, `-1.5`, `1_000`, `2.5e-3`.
    public init?(string: String) {
        var mantissa = string
        var exp10 = 0

        // Split exponent part.
        if let eIndex = mantissa.firstIndex(where: { $0 == "e" || $0 == "E" }) {
            let expPart = mantissa[mantissa.index(after: eIndex)...]
            guard let parsed = Int(expPart) else { return nil }
            exp10 = parsed
            mantissa = String(mantissa[..<eIndex])
        }

        mantissa.removeAll { $0 == "_" }
        guard !mantissa.isEmpty else { return nil }

        // Split fractional part.
        if let dotIndex = mantissa.firstIndex(of: ".") {
            let fraction = mantissa[mantissa.index(after: dotIndex)...]
            guard !fraction.contains(".") else { return nil }
            exp10 -= fraction.count
            mantissa.remove(at: dotIndex)
        }

        guard !mantissa.isEmpty, mantissa != "-", mantissa != "+",
              let sig = BigInt(mantissa) else { return nil }
        self.init(significand: sig, exponent: exp10)
    }
}

// MARK: - Comparison

extension BigDecimal: Equatable, Comparable, Hashable {
    // Normalization makes structural equality correct.

    public static func < (lhs: BigDecimal, rhs: BigDecimal) -> Bool {
        let (l, r) = aligned(lhs, rhs)
        return l < r
    }

    /// Rescales both values to a common exponent and returns the significands.
    static func aligned(_ lhs: BigDecimal, _ rhs: BigDecimal) -> (BigInt, BigInt) {
        let common = Swift.min(lhs.exponent, rhs.exponent)
        let l = lhs.significand * BigInt(10).power(lhs.exponent - common)
        let r = rhs.significand * BigInt(10).power(rhs.exponent - common)
        return (l, r)
    }
}

// MARK: - Exact arithmetic

extension BigDecimal {
    public static func + (lhs: BigDecimal, rhs: BigDecimal) -> BigDecimal {
        let common = Swift.min(lhs.exponent, rhs.exponent)
        let (l, r) = aligned(lhs, rhs)
        return BigDecimal(significand: l + r, exponent: common)
    }

    public static func - (lhs: BigDecimal, rhs: BigDecimal) -> BigDecimal {
        lhs + (-rhs)
    }

    public static prefix func - (value: BigDecimal) -> BigDecimal {
        BigDecimal(significand: -value.significand, exponent: value.exponent)
    }

    public static func * (lhs: BigDecimal, rhs: BigDecimal) -> BigDecimal {
        BigDecimal(significand: lhs.significand * rhs.significand,
                   exponent: lhs.exponent + rhs.exponent)
    }
}

// MARK: - Rounding & division

extension BigDecimal {
    /// Rounds so that at most `digits` significant digits remain (banker's rounding).
    public func rounded(toSignificantDigits digits: Int) -> BigDecimal {
        let excess = digitCount - digits
        guard excess > 0 else { return self }
        let scale = BigInt(10).power(excess)
        let (q, r) = significand.quotientAndRemainder(dividingBy: scale)
        return BigDecimal(significand: Self.roundHalfEven(quotient: q, remainder: r, divisor: scale),
                          exponent: exponent + excess)
    }

    /// Rounds to `places` decimal places (banker's rounding). Negative `places`
    /// rounds left of the decimal point (`round(1234, -2)` → `1200`).
    public func rounded(toPlaces places: Int) -> BigDecimal {
        guard exponent < -places else { return self }
        let scale = BigInt(10).power(-places - exponent)
        let (q, r) = significand.quotientAndRemainder(dividingBy: scale)
        return BigDecimal(significand: Self.roundHalfEven(quotient: q, remainder: r, divisor: scale),
                          exponent: -places)
    }

    /// Banker's rounding of `quotient` given the discarded `remainder`.
    private static func roundHalfEven(quotient: BigInt, remainder: BigInt, divisor: BigInt) -> BigInt {
        if remainder.isZero { return quotient }
        let twice = remainder.magnitude * 2
        let bump: Bool
        if twice > divisor.magnitude {
            bump = true
        } else if twice < divisor.magnitude {
            bump = false
        } else {
            bump = !quotient.isMultiple(of: 2) // exactly half: round to even
        }
        guard bump else { return quotient }
        return quotient + (remainder.sign == .minus ? -1 : 1)
    }

    /// Division to `PrecisionContext.current` significant digits.
    /// Exact when the quotient terminates within the working precision.
    public static func / (lhs: BigDecimal, rhs: BigDecimal) throws(EngineError) -> BigDecimal {
        guard !rhs.isZero else { throw EngineError.divisionByZero }
        if lhs.isZero { return .zero }

        let precision = PrecisionContext.current
        // Scale the dividend so the integer quotient carries `precision` + guard digits.
        let shift = rhs.digitCount - lhs.digitCount + precision + 2
        var numerator = lhs.significand
        var exponent = lhs.exponent - rhs.exponent
        if shift > 0 {
            numerator *= BigInt(10).power(shift)
            exponent -= shift
        }
        let (q, r) = numerator.quotientAndRemainder(dividingBy: rhs.significand)
        let quotient = roundHalfEven(quotient: q, remainder: r, divisor: rhs.significand)
        return BigDecimal(significand: quotient, exponent: exponent)
            .rounded(toSignificantDigits: precision)
    }

    /// Truncated integer division remainder, matching the sign of the dividend.
    public static func % (lhs: BigDecimal, rhs: BigDecimal) throws(EngineError) -> BigDecimal {
        guard !rhs.isZero else { throw EngineError.divisionByZero }
        let common = Swift.min(lhs.exponent, rhs.exponent)
        let (l, r) = aligned(lhs, rhs)
        return BigDecimal(significand: l % r, exponent: common)
    }
}
