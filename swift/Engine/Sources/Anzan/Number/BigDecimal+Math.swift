import Foundation

// MARK: - Exact powers & roots

extension BigDecimal {
    /// Raises to an integer power. Exact for positive exponents; negative
    /// exponents divide at working precision.
    public func power(_ n: Int) throws(EngineError) -> BigDecimal {
        if n == 0 {
            guard !isZero else { throw EngineError.domainError(message: "0^0 is undefined") }
            return .one
        }
        if n < 0 {
            return try .one / power(-n)
        }
        // Keep pathological inputs (9^999999999) from hanging the app.
        guard digitCount * n <= 1_000_000 else {
            throw EngineError.domainError(message: "result of ^ is too large")
        }
        return BigDecimal(significand: significand.power(n), exponent: exponent * n)
    }

    /// Square root via Newton iteration, to working precision.
    /// Exact when the root terminates (e.g. sqrt(2.25) == 1.5).
    public func squareRoot() throws(EngineError) -> BigDecimal {
        guard !isNegative else {
            throw EngineError.domainError(message: "sqrt of a negative number")
        }
        if isZero { return .zero }

        let precision = PrecisionContext.current
        // Work on an integer scaled so the integer sqrt carries enough digits:
        // value = sig × 10^exp; choose even shift s ≥ 0 with exp - s even,
        // then sqrt = isqrt(sig × 10^s) × 10^((exp - s) / 2).
        var shift = 2 * (precision + 2 - digitCount / 2)
        if shift < 0 { shift = 0 }
        if (exponent - shift) % 2 != 0 { shift += 1 }

        let scaled = significand * Integer.powerOfTen(shift)
        let root = Self.integerSquareRoot(scaled)
        let exact = root * root == scaled
        let result = BigDecimal(significand: root, exponent: (exponent - shift) / 2)
        return exact ? result : result.rounded(toSignificantDigits: precision)
    }

    /// Integer square root (floor) via Newton's method.
    private static func integerSquareRoot(_ n: Integer) -> Integer {
        precondition(n.sign != .minus)
        if n < 2 { return n }
        // Initial guess: 2^(ceil(bits/2)) ≥ sqrt(n), guaranteeing descent.
        var x = Integer(1) << ((n.bitWidth + 1) / 2)
        while true {
            let y = (x + n / x) / 2
            if y >= x { return x }
            x = y
        }
    }
}

// MARK: - Double bridging (transcendental fallback)

extension BigDecimal {
    /// Lossy conversion for transcendental fallback and UI affordances.
    public var doubleValue: Double {
        Double(String(describing: self)) ?? .nan
    }

    /// Converts a finite Double exactly (binary fractions are finite in base 10),
    /// then trims to a sane width so artifacts of the binary representation
    /// (e.g. 0.1000000000000000055511...) don't leak into results.
    public init?(_ value: Double) {
        guard value.isFinite else { return nil }
        guard let parsed = BigDecimal(string: shortestString(of: value)) else { return nil }
        self = parsed
    }

    /// Applies a Double-domain function, round-tripping through ~15 significant
    /// digits. This is the single seam to replace with true arbitrary-precision
    /// series implementations later — callers won't change.
    static func viaDouble(_ name: String, _ value: BigDecimal,
                          _ f: (Double) -> Double) throws(EngineError) -> BigDecimal {
        let result = f(value.doubleValue)
        guard let converted = BigDecimal(result) else {
            throw EngineError.domainError(message: "\(name) is undefined for \(value)")
        }
        return converted
    }
}

/// Shortest decimal string that round-trips the Double (Swift's default
/// description provides this guarantee).
private func shortestString(of value: Double) -> String {
    value == value.rounded() && abs(value) < 1e15
        ? String(format: "%.0f", value)
        : String(value)
}

// MARK: - Formatting

extension BigDecimal: CustomStringConvertible {
    /// Plain decimal form: `-12.5`, `0.03`. Falls back to scientific notation
    /// when the plain form would be absurdly long.
    public var description: String {
        formatted()
    }

    public func formatted(scientificThreshold: Int = 30) -> String {
        if isZero { return "0" }

        let digits = significand.magnitude.description
        let sign = isNegative ? "-" : ""
        // Position of the decimal point relative to the digit string.
        let pointPosition = digits.count + exponent

        // Too many digits either side → scientific notation.
        if pointPosition > scientificThreshold || pointPosition < -scientificThreshold {
            return scientificDescription(digits: digits, sign: sign)
        }

        if exponent >= 0 {
            return sign + digits + String(repeating: "0", count: exponent)
        }
        if pointPosition <= 0 {
            return sign + "0." + String(repeating: "0", count: -pointPosition) + digits
        }
        let index = digits.index(digits.startIndex, offsetBy: pointPosition)
        return sign + digits[..<index] + "." + digits[index...]
    }

    private func scientificDescription(digits: String, sign: String) -> String {
        let exp = digits.count + exponent - 1
        let head = digits.prefix(1)
        let tail = digits.dropFirst()
        let mantissa = tail.isEmpty ? String(head) : "\(head).\(tail)"
        return "\(sign)\(mantissa)e\(exp >= 0 ? "+" : "")\(exp)"
    }
}
