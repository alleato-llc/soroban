/// Unsigned magnitude arithmetic for `Integer`, on little-endian base-2⁶⁴ limbs
/// (`[UInt]`, least-significant first, normalized: no high zero limb, empty = 0).
///
/// Matching the machine word (and `num-bigint`'s limb size) halves the limb count
/// of a 50-digit decimal versus a 32-bit base, so schoolbook multiply and Knuth
/// Algorithm D division do a quarter of the partial-product work. Swift's
/// `UInt128`/`Int128` (macOS 15+) carry the products and quotient estimates
/// exactly, so the arithmetic stays simple and bit-for-bit identical to
/// `num-bigint`/`attaswift`. `Integer` layers sign handling on top.
enum Magnitude {
    /// Drops high zero limbs so a magnitude is canonical.
    static func normalized(_ limbs: [UInt]) -> [UInt] {
        var limbs = limbs
        while limbs.last == 0 { limbs.removeLast() }
        return limbs
    }

    /// Sign of `2·a − b` (both normalized) WITHOUT allocating `2·a`: -1 / 0 / 1.
    /// Rounding compares the doubled remainder against the divisor on every
    /// divide; computing `2·a`'s limbs on the fly avoids two array temporaries.
    static func compareDoubled(_ a: [UInt], _ b: [UInt]) -> Int {
        if a.isEmpty { return b.isEmpty ? 0 : -1 }
        // 2·a gains a top limb iff a's high bit is set.
        let doubledCount = a.count + ((a[a.count - 1] >> 63) != 0 ? 1 : 0)
        if doubledCount != b.count { return doubledCount < b.count ? -1 : 1 }
        var i = doubledCount - 1
        while i >= 0 {
            let low = i < a.count ? (a[i] << 1) : 0
            let carry = i - 1 >= 0 ? (a[i - 1] >> 63) : 0
            let twoAi = low | carry
            if twoAi != b[i] { return twoAi < b[i] ? -1 : 1 }
            i -= 1
        }
        return 0
    }

    /// Three-way compare: -1 if a < b, 0 if equal, 1 if a > b (inputs normalized).
    static func compare(_ a: [UInt], _ b: [UInt]) -> Int {
        if a.count != b.count { return a.count < b.count ? -1 : 1 }
        var i = a.count - 1
        while i >= 0 {
            if a[i] != b[i] { return a[i] < b[i] ? -1 : 1 }
            i -= 1
        }
        return 0
    }

    // MARK: Add / subtract

    /// a + b. Result normalized.
    static func add(_ a: [UInt], _ b: [UInt]) -> [UInt] {
        let (long, short) = a.count >= b.count ? (a, b) : (b, a)
        var result = [UInt](repeating: 0, count: long.count + 1)
        var carry: UInt128 = 0
        result.withUnsafeMutableBufferPointer { r in
            long.withUnsafeBufferPointer { long in
                short.withUnsafeBufferPointer { short in
                    for i in 0..<long.count {
                        var sum = UInt128(long[i]) + carry
                        if i < short.count { sum += UInt128(short[i]) }
                        r[i] = UInt(truncatingIfNeeded: sum)
                        carry = sum >> 64
                    }
                    r[long.count] = UInt(truncatingIfNeeded: carry)
                }
            }
        }
        return normalized(result)
    }

    /// a - b, requiring a >= b. Result normalized.
    static func subtract(_ a: [UInt], _ b: [UInt]) -> [UInt] {
        var result = [UInt](repeating: 0, count: a.count)
        var borrow: Int128 = 0
        result.withUnsafeMutableBufferPointer { r in
            a.withUnsafeBufferPointer { a in
                b.withUnsafeBufferPointer { b in
                    for i in 0..<a.count {
                        var diff = Int128(a[i]) - borrow
                        if i < b.count { diff -= Int128(b[i]) }
                        if diff < 0 { diff += Int128(1) << 64; borrow = 1 } else { borrow = 0 }
                        r[i] = UInt(truncatingIfNeeded: diff)
                    }
                }
            }
        }
        return normalized(result)
    }

    // MARK: Multiply

    /// Below this limb count (for the smaller operand) schoolbook wins; at or
    /// above it Karatsuba's fewer sub-multiplies pay for their overhead. ~32
    /// limbs ≈ 600 decimal digits — a 50-digit significand (≈3 limbs) never
    /// reaches it, so the common path stays schoolbook.
    static let karatsubaThreshold = 32

    /// a × b, dispatching to Karatsuba for large operands and schoolbook otherwise.
    static func multiply(_ a: [UInt], _ b: [UInt]) -> [UInt] {
        if a.isEmpty || b.isEmpty { return [] }
        if Swift.min(a.count, b.count) >= karatsubaThreshold {
            return multiplyKaratsuba(a, b)
        }
        return multiplySchoolbook(a, b)
    }

    /// Schoolbook a × b, accumulating each column in a `UInt128` so the product +
    /// running limb + carry never overflows (max is exactly 2¹²⁸−1).
    static func multiplySchoolbook(_ a: [UInt], _ b: [UInt]) -> [UInt] {
        if a.isEmpty || b.isEmpty { return [] }
        var result = [UInt](repeating: 0, count: a.count + b.count)
        result.withUnsafeMutableBufferPointer { r in
            a.withUnsafeBufferPointer { a in
                b.withUnsafeBufferPointer { b in
                    for i in 0..<a.count {
                        var carry: UInt128 = 0
                        let ai = UInt128(a[i])
                        for j in 0..<b.count {
                            let sum = ai * UInt128(b[j]) + UInt128(r[i + j]) + carry
                            r[i + j] = UInt(truncatingIfNeeded: sum)
                            carry = sum >> 64
                        }
                        r[i + b.count] = UInt(truncatingIfNeeded: carry)
                    }
                }
            }
        }
        return normalized(result)
    }

    /// Karatsuba a × b. Split each at `m` limbs (`x = x1·Bᵐ + x0`, `B = 2⁶⁴`):
    /// `z0 = x0·y0`, `z2 = x1·y1`, `z1 = (x0+x1)(y0+y1) − z0 − z2`, and the
    /// product is `z2·B²ᵐ + z1·Bᵐ + z0`. Three sub-multiplies instead of four,
    /// recursing through `multiply` (so sub-products below the threshold fall
    /// back to schoolbook).
    private static func multiplyKaratsuba(_ a: [UInt], _ b: [UInt]) -> [UInt] {
        let m = Swift.min(a.count, b.count) / 2
        let (a0, a1) = splitLimbs(a, at: m)
        let (b0, b1) = splitLimbs(b, at: m)

        let z0 = multiply(a0, b0)
        let z2 = multiply(a1, b1)
        // z1 = (a0+a1)(b0+b1) − z0 − z2, which is ≥ 0 (equals a0·b1 + a1·b0).
        let z1 = subtract(subtract(multiply(add(a0, a1), add(b0, b1)), z0), z2)

        var result = [UInt](repeating: 0, count: a.count + b.count + 1)
        addInto(&result, z0, offset: 0)
        addInto(&result, z1, offset: m)
        addInto(&result, z2, offset: 2 * m)
        return normalized(result)
    }

    /// Splits normalized limbs into (low `at` limbs, the rest), each normalized.
    private static func splitLimbs(_ x: [UInt], at m: Int) -> (low: [UInt], high: [UInt]) {
        if x.count <= m { return (normalized(x), []) }
        return (normalized(Array(x[0..<m])), normalized(Array(x[m...])))
    }

    /// Adds `addend` into `result` starting at limb `offset`, propagating carry.
    /// `result` must have room (callers size it to fit the full product + carry).
    private static func addInto(_ result: inout [UInt], _ addend: [UInt], offset: Int) {
        if addend.isEmpty { return }
        result.withUnsafeMutableBufferPointer { r in
            addend.withUnsafeBufferPointer { addend in
                var carry: UInt128 = 0
                var i = 0
                while i < addend.count || carry != 0 {
                    let idx = offset + i
                    let value = i < addend.count ? UInt128(addend[i]) : 0
                    let sum = UInt128(r[idx]) + value + carry
                    r[idx] = UInt(truncatingIfNeeded: sum)
                    carry = sum >> 64
                    i += 1
                }
            }
        }
    }

    // MARK: Shift

    /// a << bits (multiply by 2^bits).
    static func shiftLeft(_ a: [UInt], _ bits: Int) -> [UInt] {
        if a.isEmpty || bits == 0 { return a }
        let limbShift = bits / 64
        let bitShift = bits % 64
        var result = [UInt](repeating: 0, count: a.count + limbShift + 1)
        for i in 0..<a.count {
            let v = UInt128(a[i]) << bitShift
            result[i + limbShift] |= UInt(truncatingIfNeeded: v)
            result[i + limbShift + 1] |= UInt(truncatingIfNeeded: v >> 64)
        }
        return normalized(result)
    }

    // MARK: Divide

    /// Quotient and remainder of a ÷ b (b non-empty). Both normalized. Truncated
    /// (magnitudes only; the caller applies signs).
    static func divMod(_ a: [UInt], _ b: [UInt]) -> (quotient: [UInt], remainder: [UInt]) {
        precondition(!b.isEmpty, "division by zero magnitude")
        if compare(a, b) < 0 { return ([], a) }
        if b.count == 1 { return divModSingle(a, b[0]) }
        return divModKnuth(a, b)
    }

    /// Fast path: divide by a single limb via hardware 128÷64 (`dividingFullWidth`),
    /// whose quotient can't overflow because the running remainder stays < divisor.
    private static func divModSingle(_ a: [UInt], _ divisor: UInt) -> ([UInt], [UInt]) {
        var quotient = [UInt](repeating: 0, count: a.count)
        var rem: UInt = 0
        a.withUnsafeBufferPointer { a in
            quotient.withUnsafeMutableBufferPointer { q in
                var i = a.count - 1
                while i >= 0 {
                    let (quo, r) = divisor.dividingFullWidth((high: rem, low: a[i]))
                    q[i] = quo
                    rem = r
                    i -= 1
                }
            }
        }
        return (normalized(quotient), rem == 0 ? [] : [rem])
    }

    /// Knuth Algorithm D (TAOCP vol. 2, 4.3.1), base 2⁶⁴, with `UInt128`/`Int128`
    /// intermediates. `n = b.count >= 2`. Swift's smart `>>`/`<<` yield 0 when the
    /// shift ≥ the width, so the classic `>> (64 - s)` needs no `s == 0` case.
    private static func divModKnuth(_ a: [UInt], _ b: [UInt]) -> ([UInt], [UInt]) {
        let n = b.count
        let m = a.count - n

        // D1. Normalize so the divisor's top limb has its high bit set.
        let s = b[n - 1].leadingZeroBitCount
        var vn = [UInt](repeating: 0, count: n)
        for i in stride(from: n - 1, through: 1, by: -1) {
            vn[i] = (b[i] << s) | (b[i - 1] >> (64 - s))
        }
        vn[0] = b[0] << s

        var un = [UInt](repeating: 0, count: a.count + 1)
        un[a.count] = a[a.count - 1] >> (64 - s)
        for i in stride(from: a.count - 1, through: 1, by: -1) {
            un[i] = (a[i] << s) | (a[i - 1] >> (64 - s))
        }
        un[0] = a[0] << s

        var quotient = [UInt](repeating: 0, count: m + 1)
        let base = UInt128(1) << 64
        let vTop = UInt128(vn[n - 1])
        let vSecond = UInt128(vn[n - 2])

        un.withUnsafeMutableBufferPointer { un in
            vn.withUnsafeBufferPointer { vn in
                quotient.withUnsafeMutableBufferPointer { quotient in
                    var j = m
                    while j >= 0 {
                        // D3. Estimate the quotient digit qhat (at most 2 too large).
                        let numer = (UInt128(un[j + n]) << 64) | UInt128(un[j + n - 1])
                        var qhat = numer / vTop
                        var rhat = numer % vTop
                        while qhat >= base || qhat * vSecond > (rhat << 64) + UInt128(un[j + n - 2]) {
                            qhat -= 1
                            rhat += vTop
                            if rhat >= base { break }
                        }

                        // D4. Multiply and subtract qhat · v from the running dividend.
                        var borrow: Int128 = 0
                        var carry: UInt128 = 0
                        for i in 0..<n {
                            let product = qhat * UInt128(vn[i]) + carry
                            carry = product >> 64
                            let sub = Int128(un[j + i]) - borrow - Int128(UInt(truncatingIfNeeded: product))
                            un[j + i] = UInt(truncatingIfNeeded: sub)
                            borrow = sub < 0 ? 1 : 0
                        }
                        let sub = Int128(un[j + n]) - borrow - Int128(carry)
                        un[j + n] = UInt(truncatingIfNeeded: sub)

                        // D5/D6. If we oversubtracted, qhat was one too big — add v back.
                        if sub < 0 {
                            qhat -= 1
                            var addCarry: UInt128 = 0
                            for i in 0..<n {
                                let sum = UInt128(un[j + i]) + UInt128(vn[i]) + addCarry
                                un[j + i] = UInt(truncatingIfNeeded: sum)
                                addCarry = sum >> 64
                            }
                            un[j + n] = UInt(truncatingIfNeeded: UInt128(un[j + n]) + addCarry)
                        }
                        quotient[j] = UInt(truncatingIfNeeded: qhat)
                        j -= 1
                    }
                }
            }
        }

        // D8. Denormalize the remainder (shift right by s).
        var remainder = [UInt](repeating: 0, count: n)
        for i in 0..<n {
            let low = un[i] >> s
            let high = i + 1 < un.count ? (un[i + 1] << (64 - s)) : 0
            remainder[i] = low | high
        }
        return (normalized(quotient), normalized(remainder))
    }

    // MARK: Decimal conversion

    /// Largest power of ten below 2⁶⁴, and its digit width — the chunk size for
    /// base-10 conversion (one `UInt` division per 19 decimal digits).
    static let decimalChunk: UInt = 10_000_000_000_000_000_000 // 10¹⁹
    static let decimalChunkDigits = 19

    /// Base-10 string of a magnitude (empty magnitude → "0").
    ///
    /// Divide-and-conquer: split the value by the largest cached power
    /// `10^(19·2^i)` below it, recurse on each half, and zero-pad the low half to
    /// that power's digit width. Each divisor is ~half the value's size, so this
    /// is ~O(M(n)·log n) instead of the O(n²) of stripping one 10¹⁹ chunk at a
    /// time — the win shows on large operands (factorials, high powers).
    static func decimalString(_ limbs: [UInt]) -> String {
        if limbs.isEmpty { return "0" }
        if limbs.count == 1 { return String(limbs[0]) }
        // Squaring ladder 10^(19·2^i), built once, up to just below the value.
        var powers: [[UInt]] = [[decimalChunk]]
        var digits = [decimalChunkDigits]
        while true {
            let squared = multiply(powers[powers.count - 1], powers[powers.count - 1])
            if squared.count >= limbs.count { break }
            powers.append(squared)
            digits.append(digits[digits.count - 1] * 2)
        }
        return convertToDecimal(limbs, powers: powers, digits: digits)
    }

    /// Recursive base-10 conversion over the prebuilt `10^(19·2^i)` ladder.
    private static func convertToDecimal(_ v: [UInt], powers: [[UInt]], digits: [Int]) -> String {
        if v.count <= 1 { return String(v.first ?? 0) }
        // Largest ladder level strictly smaller (in limbs) than v — so the high
        // half is non-empty and both halves shrink.
        var level = powers.count - 1
        while level > 0 && powers[level].count >= v.count { level -= 1 }
        let (hi, lo) = divMod(v, powers[level])
        let hiStr = convertToDecimal(hi, powers: powers, digits: digits)
        let loStr = convertToDecimal(lo, powers: powers, digits: digits)
        let pad = digits[level] - loStr.count
        return hiStr + (pad > 0 ? String(repeating: "0", count: pad) + loStr : loStr)
    }
}

// MARK: - Decimal parsing (used by Integer)

extension Integer {
    /// Builds an `Integer` from a run of ASCII digits (no sign), applying `negative`.
    /// Accumulates 19 digits at a time into a `UInt` chunk, then folds via
    /// `value = value * 10¹⁹ + chunk` on the magnitude limbs.
    static func parseDecimal(_ digits: Substring, negative: Bool) -> Integer {
        var limbs = [UInt]()
        let chars = Array(digits)
        var index = 0
        let group = Magnitude.decimalChunkDigits
        // Lead with the shortest chunk so the rest align to full groups.
        let firstLen = chars.count % group == 0 ? group : chars.count % group
        while index < chars.count {
            let len = index == 0 ? firstLen : group
            var chunk: UInt = 0
            for _ in 0..<len {
                chunk = chunk * 10 + UInt(chars[index].wholeNumberValue!)
                index += 1
            }
            let scale: UInt = len == group ? Magnitude.decimalChunk : pow10(len)
            limbs = Magnitude.add(Magnitude.multiply(limbs, [scale]), chunk == 0 ? [] : [chunk])
        }
        return pack(negative: negative, limbs)
    }

    private static func pow10(_ n: Int) -> UInt {
        var result: UInt = 1
        for _ in 0..<n { result *= 10 }
        return result
    }
}
