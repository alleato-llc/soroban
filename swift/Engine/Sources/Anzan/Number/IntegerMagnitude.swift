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

    /// Schoolbook a × b, accumulating each column in a `UInt128` so the product +
    /// running limb + carry never overflows (max is exactly 2¹²⁸−1). Operands here
    /// are tiny — a few limbs for 50-digit decimals — so schoolbook beats Karatsuba;
    /// add Karatsuba only if a large-operand workload ever needs it.
    static func multiply(_ a: [UInt], _ b: [UInt]) -> [UInt] {
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
    static func decimalString(_ limbs: [UInt]) -> String {
        if limbs.isEmpty { return "0" }
        var value = limbs
        var chunks = [UInt]()
        while !value.isEmpty {
            let (q, r) = divModSingle(value, decimalChunk)
            chunks.append(r.first ?? 0)
            value = q
        }
        var result = String(chunks.last!)
        for chunk in chunks.dropLast().reversed() {
            let s = String(chunk)
            result += String(repeating: "0", count: decimalChunkDigits - s.count) + s
        }
        return result
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
