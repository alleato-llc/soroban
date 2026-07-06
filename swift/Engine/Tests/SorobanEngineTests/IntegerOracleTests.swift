import BigInt
import Testing
@testable import Anzan

/// Differential fuzz oracle for the custom `Integer` bignum: every operation must
/// produce the SAME result as `attaswift/BigInt` (the reference), across small,
/// boundary, and big magnitudes of both signs. Because the shared spec compares
/// results as exact strings, any limb/carry/rounding bug here would surface as a
/// spec failure — this catches it first, in isolation, with a reproducible seed.
@Suite("Integer ⇄ BigInt differential oracle")
struct IntegerOracleTests {
    /// Deterministic SplitMix64 so a failure reproduces exactly.
    struct SeededRNG: RandomNumberGenerator {
        var state: UInt64
        init(seed: UInt64) { state = seed }
        mutating func next() -> UInt64 {
            state &+= 0x9E37_79B9_7F4A_7C15
            var z = state
            z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
            z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
            return z ^ (z >> 31)
        }
    }

    /// Boundary values that stress the `.small`⇄`.big` transition and carry/borrow.
    static let edgeStrings = [
        "0", "1", "-1", "2", "-2", "10", "-10",
        "9223372036854775807",   // Int.max
        "9223372036854775808",   // Int.max + 1  → first .big positive
        "-9223372036854775808",  // Int.min  (still .small)
        "-9223372036854775809",  // Int.min - 1  → first .big negative
        "4294967295", "4294967296", "18446744073709551615", "18446744073709551616",
        "99999999999999999999999999999999999999999999999999", // 50 nines
        "-100000000000000000000000000000000000000000000000000",
    ]

    /// Random reference value: a random-length run of decimal digits, random sign.
    static func randomRef(_ rng: inout SeededRNG) -> BigInt {
        let digitCount = Int.random(in: 1...60, using: &rng)
        var s = ""
        for i in 0..<digitCount {
            let d = Int.random(in: (i == 0 ? 1 : 0)...9, using: &rng)
            s += String(d)
        }
        if Bool.random(using: &rng) { s = "-" + s }
        return BigInt(s)!
    }

    static func toInteger(_ b: BigInt) -> Integer { Integer(b.description)! }

    /// The full sample set: every edge value plus many random pairs, seeded.
    static func samples() -> [BigInt] {
        var rng = SeededRNG(seed: 0xA11CE)
        var values = edgeStrings.map { BigInt($0)! }
        for _ in 0..<4000 { values.append(randomRef(&rng)) }
        return values
    }

    @Test func parseAndPrintRoundTrip() {
        for b in Self.samples() {
            #expect(Self.toInteger(b).description == b.description)
        }
        // Malformed input is rejected, matching BigInt(String).
        for bad in ["", "-", "+", "1.2", "0x10", "abc", "1_000", " 12"] {
            #expect(Integer(bad) == nil, "\(bad) should not parse")
        }
    }

    @Test func addSubtractMultiply() {
        var rng = SeededRNG(seed: 0xADD)
        let pool = Self.samples()
        for _ in 0..<20000 {
            let rb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let sb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let a = Self.toInteger(rb), b = Self.toInteger(sb)
            #expect((a + b).description == (rb + sb).description)
            #expect((a - b).description == (rb - sb).description)
            #expect((a * b).description == (rb * sb).description)
            #expect((-a).description == (-rb).description)
        }
    }

    @Test func divideRemainderQuotient() {
        var rng = SeededRNG(seed: 0xD10)
        let pool = Self.samples().filter { !$0.isZero }
        let all = Self.samples()
        for _ in 0..<20000 {
            let rb = all[Int.random(in: 0..<all.count, using: &rng)]
            let sb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let a = Self.toInteger(rb), b = Self.toInteger(sb)
            let (q, r) = rb.quotientAndRemainder(dividingBy: sb)
            let (iq, ir) = a.quotientAndRemainder(dividingBy: b)
            #expect(iq.description == q.description, "q of \(rb) / \(sb)")
            #expect(ir.description == r.description, "r of \(rb) % \(sb)")
            #expect((a / b).description == (rb / sb).description)
            #expect((a % b).description == (rb % sb).description)
        }
    }

    @Test func powerMatches() {
        var rng = SeededRNG(seed: 0x500)
        let pool = Self.samples()
        for _ in 0..<3000 {
            let rb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let n = Int.random(in: 0...8, using: &rng)
            #expect(Self.toInteger(rb).power(n).description == rb.power(n).description,
                    "\(rb)^\(n)")
        }
    }

    @Test func shiftLeftMatches() {
        var rng = SeededRNG(seed: 0x51F7)
        let pool = Self.samples()
        for _ in 0..<5000 {
            let rb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let bits = Int.random(in: 0...80, using: &rng)
            #expect((Self.toInteger(rb) << bits).description == (rb << bits).description,
                    "\(rb) << \(bits)")
        }
    }

    @Test func comparisonAndQueries() {
        var rng = SeededRNG(seed: 0xC0FFEE)
        let pool = Self.samples()
        for _ in 0..<20000 {
            let rb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let sb = pool[Int.random(in: 0..<pool.count, using: &rng)]
            let a = Self.toInteger(rb), b = Self.toInteger(sb)
            #expect((a < b) == (rb < sb))
            #expect((a == b) == (rb == sb))
            #expect(a.isZero == rb.isZero)
            #expect((a.sign == .minus) == (rb.sign == .minus))
            #expect(a.magnitude.description == rb.magnitude.description)
            if !sb.isZero {
                #expect(a.isMultiple(of: b) == rb.isMultiple(of: sb))
            }
        }
    }

    /// Guard for the stringify-free `decimalDigitCount` (drives `BigDecimal.digitCount`,
    /// which feeds division guard-digits — an off-by-one here would shift results and
    /// diverge the two engines). Assert it equals the base-10 string length across the
    /// fuzz corpus PLUS every power-of-ten boundary (`10^k − 1`, `10^k`, `10^k + 1`),
    /// where the digit count increments and the bit-length estimate must correct.
    @Test func decimalDigitCountMatchesStringLength() {
        var values = Self.samples()
        for k in 0...200 {
            let ten = BigInt(10).power(k)
            values.append(contentsOf: [ten - 1, ten, ten + 1, -ten, -(ten + 1)])
        }
        for b in values {
            let n = Self.toInteger(b)
            #expect(n.decimalDigitCount == n.magnitude.description.count,
                    "digitCount of \(b)")
        }
    }

    /// `bitWidth` isn't compared to attaswift (its sign-bit convention differs);
    /// instead assert the property the integer-sqrt seed relies on:
    /// `2^bitWidth > |value| ≥ 2^(bitWidth-1)` for nonzero values.
    @Test func bitWidthBoundsMagnitude() {
        for b in Self.samples() where !b.isZero {
            let width = Self.toInteger(b).bitWidth
            let mag = Self.toInteger(b).magnitude
            #expect(mag < (Integer(1) << width))
            #expect(!(mag < (Integer(1) << (width - 1))))
        }
    }
}
