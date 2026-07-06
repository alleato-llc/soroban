/// Arbitrary-precision signed integer — the backing store for `BigDecimal`'s
/// significand.
///
/// Sign-magnitude, mirroring `num-bigint`'s layout so the Rust and Swift engines
/// stay bit-for-bit result-identical. Two representations:
///
/// - `.small(Int)` — any value that fits in a machine word. The common calculator
///   operand (`fib`, `∑ i²`, small literals) lives here with **no heap allocation
///   and no ARC**, which is the whole point: `attaswift/BigInt` pays array + ARC +
///   copy-on-write on every operation, even for the value `2`.
/// - `.big(negative:magnitude:)` — a little-endian `[UInt32]` magnitude (base 2³²,
///   normalized: no high zero limb, non-empty), reached only when a value exceeds
///   `Int`. Limb loops run tight over a Swift array; base 2³² keeps every partial
///   product/quotient within `UInt64`, so the arithmetic is simple and exact.
///
/// Every value is canonical: anything representable in `Int` is `.small`, so
/// `.big` magnitudes are always > `Int.max` (or, negative, > `2⁶³`). Construction
/// funnels through `pack(negative:_:)`, which enforces this.
package enum Integer: Sendable {
    case small(Int)
    case big(negative: Bool, magnitude: [UInt])

    /// Sign as a two-case value mirroring `attaswift`/`num-bigint` (zero reports
    /// `.plus`), so `x.sign == .minus` reads the same at the call sites.
    package enum Sign: Sendable, Equatable {
        case minus
        case plus
    }
}

// MARK: - Canonical construction

extension Integer {
    /// Packs a sign + normalized-ish magnitude into the canonical representation:
    /// drops high zero limbs, collapses to `.small` whenever the value fits in
    /// `Int` (including `Int.min`), else keeps `.big`. This is the ONLY path that
    /// builds a `.big`, so canonicity is guaranteed.
    static func pack(negative: Bool, _ magnitude: [UInt]) -> Integer {
        var mag = magnitude
        while mag.last == 0 { mag.removeLast() }
        if mag.isEmpty { return .small(0) }

        if mag.count == 1 {
            let value = mag[0]
            if !negative {
                if value <= UInt(Int.max) { return .small(Int(value)) }
            } else {
                if value <= UInt(Int.max) { return .small(-Int(value)) }
                if value == UInt(Int.max) + 1 { return .small(Int.min) } // -2⁶³
            }
        }
        return .big(negative: negative, magnitude: mag)
    }

    /// The magnitude (absolute value) as normalized little-endian base-2⁶⁴ limbs;
    /// empty for zero.
    var magnitudeLimbs: [UInt] {
        switch self {
        case .small(let value):
            let m = value.magnitude // UInt: |Int.min| == 2⁶³ is representable
            return m == 0 ? [] : [m]
        case .big(_, let magnitude):
            return magnitude
        }
    }
}

// MARK: - Initializers

extension Integer {
    init(_ value: Int) {
        self = .small(value)
    }

    /// Parses an optionally-signed base-10 integer (`"123"`, `"-45"`, `"+9"`).
    /// Returns nil on any non-digit content (matches `BigInt(String)`).
    init?(_ text: String) {
        var negative = false
        var digits = Substring(text)
        if let first = digits.first, first == "-" || first == "+" {
            negative = first == "-"
            digits = digits.dropFirst()
        }
        guard !digits.isEmpty, digits.allSatisfy({ $0.isASCII && $0.isNumber }) else {
            return nil
        }
        self = Integer.parseDecimal(digits, negative: negative)
    }
}

extension Integer: ExpressibleByIntegerLiteral {
    package init(integerLiteral value: Int) {
        self = .small(value)
    }
}

// MARK: - Sign & magnitude queries

extension Integer {
    var isZero: Bool {
        if case .small(0) = self { return true }
        return false
    }

    var isNegative: Bool {
        switch self {
        case .small(let value): return value < 0
        case .big(let negative, _): return negative
        }
    }

    var sign: Sign {
        isNegative ? .minus : .plus
    }

    /// Absolute value (never negative).
    package var magnitude: Integer {
        switch self {
        case .small(let value):
            // |Int.min| overflows Int → promote to a single-limb .big (2⁶³).
            if value == Int.min { return .pack(negative: false, [0x8000_0000_0000_0000]) }
            return .small(Swift.abs(value))
        case .big(_, let magnitude):
            return .big(negative: false, magnitude: magnitude)
        }
    }

    /// Minimal number of bits to represent the magnitude (0 for zero). Used only
    /// for the integer-sqrt initial guess, where any value ≥ log2(|self|) works.
    var bitWidth: Int {
        let limbs = magnitudeLimbs
        guard let top = limbs.last else { return 0 }
        return (limbs.count - 1) * 64 + (64 - top.leadingZeroBitCount)
    }

    /// Number of decimal digits in the magnitude (zero counts as one), computed
    /// from the bit width **without allocating a base-10 string**. Exact: the
    /// `1233/4096 ≈ log10(2)` estimate is corrected against cached powers of ten
    /// (`10^(d-1) ≤ |self| < 10^d`). Drives `BigDecimal.digitCount`, which the
    /// division/rounding paths hit several times per op.
    var decimalDigitCount: Int {
        let bits = bitWidth
        if bits == 0 { return 1 } // zero
        let mag = magnitude
        var d = bits * 1233 / 4096 + 1
        while mag >= Integer.powerOfTen(d) { d += 1 }
        while d > 1 && mag < Integer.powerOfTen(d - 1) { d -= 1 }
        return d
    }

    var isEven: Bool {
        switch self {
        case .small(let value): return value % 2 == 0
        case .big(_, let magnitude): return (magnitude.first ?? 0) % 2 == 0
        }
    }

    func isMultiple(of other: Integer) -> Bool {
        if other.isZero { return isZero }
        if case .small(2) = other { return isEven }
        return quotientAndRemainder(dividingBy: other).remainder.isZero
    }
}

// MARK: - Equatable, Comparable, Hashable

extension Integer: Equatable, Comparable, Hashable {
    package static func == (lhs: Integer, rhs: Integer) -> Bool {
        if case .small(let a) = lhs, case .small(let b) = rhs { return a == b }
        return lhs.isNegative == rhs.isNegative
            && Magnitude.compare(lhs.magnitudeLimbs, rhs.magnitudeLimbs) == 0
    }

    package static func < (lhs: Integer, rhs: Integer) -> Bool {
        if case .small(let a) = lhs, case .small(let b) = rhs { return a < b }
        switch (lhs.isNegative, rhs.isNegative) {
        case (false, true): return false
        case (true, false): return true
        case (false, false):
            return Magnitude.compare(lhs.magnitudeLimbs, rhs.magnitudeLimbs) < 0
        case (true, true):
            return Magnitude.compare(lhs.magnitudeLimbs, rhs.magnitudeLimbs) > 0
        }
    }

    package func hash(into hasher: inout Hasher) {
        hasher.combine(isNegative)
        hasher.combine(magnitudeLimbs)
    }
}

// MARK: - Cached powers of ten

extension Integer {
    /// Powers of ten `0...128`, precomputed once. `10^k` is rebuilt constantly by
    /// `BigDecimal` (aligning addends, scaling the dividend, rounding), so caching
    /// the common range turns those into an O(1) array read instead of a fresh
    /// binary-exponentiation each time.
    private static let tenLadder: [Integer] = {
        var ladder = [Integer]()
        ladder.reserveCapacity(129)
        var value = Integer(1)
        for _ in 0...128 {
            ladder.append(value)
            value = value * Integer(10)
        }
        return ladder
    }()

    /// `10^k` (k ≥ 0), served from the cache when in range.
    static func powerOfTen(_ k: Int) -> Integer {
        k < tenLadder.count ? tenLadder[k] : Integer(10).power(k)
    }
}

// MARK: - Base-10 string

extension Integer: CustomStringConvertible {
    package var description: String {
        switch self {
        case .small(let value): return String(value)
        case .big(let negative, let magnitude):
            return (negative ? "-" : "") + Magnitude.decimalString(magnitude)
        }
    }
}
