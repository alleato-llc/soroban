/// Division, remainder, and integer power for `Integer`. Truncated toward zero —
/// the quotient's sign is the operands' sign product; the remainder follows the
/// dividend's sign (matching `attaswift`/`num-bigint`).

extension Integer {
    /// Truncated quotient and remainder. Traps on divide-by-zero (the caller —
    /// `BigDecimal` — guards zero divisors and raises a typed error first).
    func quotientAndRemainder(dividingBy divisor: Integer) -> (quotient: Integer, remainder: Integer) {
        precondition(!divisor.isZero, "Integer division by zero")
        // Fast path: both fit in Int. (Int.min / -1 overflows — fall through to limbs.)
        if case .small(let a) = self, case .small(let b) = divisor,
           !(a == Int.min && b == -1) {
            return (.small(a / b), .small(a % b))
        }
        let (q, r) = Magnitude.divMod(magnitudeLimbs, divisor.magnitudeLimbs)
        let quotient = Integer.pack(negative: isNegative != divisor.isNegative, q)
        let remainder = Integer.pack(negative: isNegative, r)
        return (quotient, remainder)
    }

    static func / (lhs: Integer, rhs: Integer) -> Integer {
        lhs.quotientAndRemainder(dividingBy: rhs).quotient
    }

    static func % (lhs: Integer, rhs: Integer) -> Integer {
        lhs.quotientAndRemainder(dividingBy: rhs).remainder
    }

    static func /= (lhs: inout Integer, rhs: Integer) { lhs = lhs / rhs }
    static func %= (lhs: inout Integer, rhs: Integer) { lhs = lhs % rhs }

    /// Raises to a non-negative integer power by binary exponentiation (exact).
    /// `n >= 0` — negative exponents are handled at the `BigDecimal` level.
    func power(_ n: Int) -> Integer {
        precondition(n >= 0, "Integer.power requires a non-negative exponent")
        if n == 0 { return .small(1) }
        if case .small(1) = self { return .small(1) }
        if isZero { return .small(0) }

        var result = Integer.small(1)
        var base = self
        var exponent = n
        while exponent > 0 {
            if exponent & 1 == 1 { result = result * base }
            exponent >>= 1
            if exponent > 0 { base = base * base }
        }
        return result
    }
}
