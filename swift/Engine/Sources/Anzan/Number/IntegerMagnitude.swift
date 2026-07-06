/// Unsigned magnitude arithmetic for `Integer`, on little-endian base-2³² limbs
/// (`[UInt32]`, least-significant first, normalized: no high zero limb, empty = 0).
///
/// Base 2³² keeps every partial product and quotient digit inside `UInt64`, so the
/// classic schoolbook multiply and Knuth Algorithm D division are exact with no
/// 128-bit intermediates — the simplest path to results that match `num-bigint`
/// (and `attaswift`) bit-for-bit. `Integer` layers sign handling on top.
enum Magnitude {
    /// Drops high zero limbs so a magnitude is canonical.
    static func normalized(_ limbs: [UInt32]) -> [UInt32] {
        var limbs = limbs
        while limbs.last == 0 { limbs.removeLast() }
        return limbs
    }

    /// Three-way compare: -1 if a < b, 0 if equal, 1 if a > b (inputs normalized).
    static func compare(_ a: [UInt32], _ b: [UInt32]) -> Int {
        if a.count != b.count { return a.count < b.count ? -1 : 1 }
        var i = a.count - 1
        while i >= 0 {
            if a[i] != b[i] { return a[i] < b[i] ? -1 : 1 }
            i -= 1
        }
        return 0
    }

    // MARK: Add / subtract

    /// a + b.
    static func add(_ a: [UInt32], _ b: [UInt32]) -> [UInt32] {
        let (long, short) = a.count >= b.count ? (a, b) : (b, a)
        var result = [UInt32]()
        result.reserveCapacity(long.count + 1)
        var carry: UInt64 = 0
        for i in 0..<long.count {
            var sum = UInt64(long[i]) + carry
            if i < short.count { sum += UInt64(short[i]) }
            result.append(UInt32(truncatingIfNeeded: sum))
            carry = sum >> 32
        }
        if carry != 0 { result.append(UInt32(carry)) }
        return result
    }

    /// a - b, requiring a >= b. Result normalized.
    static func subtract(_ a: [UInt32], _ b: [UInt32]) -> [UInt32] {
        var result = [UInt32]()
        result.reserveCapacity(a.count)
        var borrow: Int64 = 0
        for i in 0..<a.count {
            var diff = Int64(a[i]) - borrow
            if i < b.count { diff -= Int64(b[i]) }
            if diff < 0 { diff += 1 << 32; borrow = 1 } else { borrow = 0 }
            result.append(UInt32(truncatingIfNeeded: diff))
        }
        return normalized(result)
    }

    // MARK: Multiply

    /// Schoolbook a × b. (Operands here are tiny — ≤ a few limbs for 50-digit
    /// decimals — so schoolbook beats Karatsuba; add Karatsuba only if the bench
    /// later shows a large-operand workload that needs it.)
    static func multiply(_ a: [UInt32], _ b: [UInt32]) -> [UInt32] {
        if a.isEmpty || b.isEmpty { return [] }
        var result = [UInt32](repeating: 0, count: a.count + b.count)
        for i in 0..<a.count {
            var carry: UInt64 = 0
            let ai = UInt64(a[i])
            for j in 0..<b.count {
                let sum = ai * UInt64(b[j]) + UInt64(result[i + j]) + carry
                result[i + j] = UInt32(truncatingIfNeeded: sum)
                carry = sum >> 32
            }
            result[i + b.count] = UInt32(truncatingIfNeeded: carry)
        }
        return normalized(result)
    }

    // MARK: Shift

    /// a << bits (multiply by 2^bits).
    static func shiftLeft(_ a: [UInt32], _ bits: Int) -> [UInt32] {
        if a.isEmpty || bits == 0 { return a }
        let limbShift = bits / 32
        let bitShift = bits % 32
        var result = [UInt32](repeating: 0, count: a.count + limbShift + 1)
        for i in 0..<a.count {
            let v = UInt64(a[i]) << bitShift
            result[i + limbShift] |= UInt32(truncatingIfNeeded: v)
            result[i + limbShift + 1] |= UInt32(truncatingIfNeeded: v >> 32)
        }
        return normalized(result)
    }

    // MARK: Divide

    /// Quotient and remainder of a ÷ b (b non-empty). Both normalized. Truncated
    /// (magnitudes only; the caller applies signs).
    static func divMod(_ a: [UInt32], _ b: [UInt32]) -> (quotient: [UInt32], remainder: [UInt32]) {
        precondition(!b.isEmpty, "division by zero magnitude")
        if compare(a, b) < 0 { return ([], a) }
        if b.count == 1 { return divModSingle(a, b[0]) }
        return divModKnuth(a, b)
    }

    /// Fast path: divide by a single limb.
    private static func divModSingle(_ a: [UInt32], _ divisor: UInt32) -> ([UInt32], [UInt32]) {
        var quotient = [UInt32](repeating: 0, count: a.count)
        var rem: UInt64 = 0
        let d = UInt64(divisor)
        var i = a.count - 1
        while i >= 0 {
            let cur = (rem << 32) | UInt64(a[i])
            quotient[i] = UInt32(cur / d)
            rem = cur % d
            i -= 1
        }
        return (normalized(quotient), rem == 0 ? [] : [UInt32(rem)])
    }

    /// Knuth Algorithm D (TAOCP vol. 2, 4.3.1), base 2³². `n = b.count >= 2`.
    /// Swift's smart `>>` yields 0 when the shift ≥ the width, so the classic
    /// `>> (32 - s)` needs no `s == 0` special case.
    private static func divModKnuth(_ a: [UInt32], _ b: [UInt32]) -> ([UInt32], [UInt32]) {
        let n = b.count
        let m = a.count - n

        // D1. Normalize so the divisor's top limb has its high bit set.
        let s = b[n - 1].leadingZeroBitCount
        var vn = [UInt32](repeating: 0, count: n)
        for i in stride(from: n - 1, through: 1, by: -1) {
            vn[i] = (b[i] << s) | (UInt32(UInt64(b[i - 1]) >> (32 - s)))
        }
        vn[0] = b[0] << s

        var un = [UInt32](repeating: 0, count: a.count + 1)
        un[a.count] = UInt32(UInt64(a[a.count - 1]) >> (32 - s))
        for i in stride(from: a.count - 1, through: 1, by: -1) {
            un[i] = (a[i] << s) | (UInt32(UInt64(a[i - 1]) >> (32 - s)))
        }
        un[0] = a[0] << s

        var quotient = [UInt32](repeating: 0, count: m + 1)
        let base: UInt64 = 1 << 32
        let vTop = UInt64(vn[n - 1])
        let vSecond = UInt64(vn[n - 2])

        var j = m
        while j >= 0 {
            // D3. Estimate the quotient digit.
            let numer = (UInt64(un[j + n]) << 32) | UInt64(un[j + n - 1])
            var qhat = numer / vTop
            var rhat = numer % vTop
            while qhat >= base || qhat * vSecond > (rhat << 32) + UInt64(un[j + n - 2]) {
                qhat -= 1
                rhat += vTop
                if rhat >= base { break }
            }

            // D4. Multiply and subtract qhat · v from the running dividend.
            var borrow: Int64 = 0
            var carry: UInt64 = 0
            for i in 0..<n {
                let product = qhat * UInt64(vn[i]) + carry
                carry = product >> 32
                let sub = Int64(un[j + i]) - borrow - Int64(product & 0xFFFF_FFFF)
                un[j + i] = UInt32(truncatingIfNeeded: sub)
                borrow = sub < 0 ? 1 : 0
            }
            let sub = Int64(un[j + n]) - borrow - Int64(carry)
            un[j + n] = UInt32(truncatingIfNeeded: sub)

            // D5/D6. If we oversubtracted, qhat was one too big — add v back.
            if sub < 0 {
                qhat -= 1
                var addCarry: UInt64 = 0
                for i in 0..<n {
                    let sum = UInt64(un[j + i]) + UInt64(vn[i]) + addCarry
                    un[j + i] = UInt32(truncatingIfNeeded: sum)
                    addCarry = sum >> 32
                }
                un[j + n] = UInt32(truncatingIfNeeded: UInt64(un[j + n]) + addCarry)
            }
            quotient[j] = UInt32(qhat)
            j -= 1
        }

        // D8. Denormalize the remainder (shift right by s).
        var remainder = [UInt32](repeating: 0, count: n)
        for i in 0..<n {
            let low = UInt32(UInt64(un[i]) >> s)
            let high = i + 1 < un.count ? (un[i + 1] << (32 - s)) : 0
            remainder[i] = low | high
        }
        return (normalized(quotient), normalized(remainder))
    }

    // MARK: Decimal conversion

    /// Base-10 string of a magnitude (empty magnitude → "0"). Chunks by 10⁹ so the
    /// per-limb work is one `UInt64` division, not one per decimal digit.
    static func decimalString(_ limbs: [UInt32]) -> String {
        if limbs.isEmpty { return "0" }
        var value = limbs
        var chunks = [UInt32]()
        let billion: UInt32 = 1_000_000_000
        while !value.isEmpty {
            let (q, r) = divModSingle(value, billion)
            chunks.append(r.first ?? 0)
            value = q
        }
        var result = String(chunks.last!)
        for chunk in chunks.dropLast().reversed() {
            let s = String(chunk)
            result += String(repeating: "0", count: 9 - s.count) + s
        }
        return result
    }
}

// MARK: - Decimal parsing (used by Integer)

extension Integer {
    /// Builds an `Integer` from a run of ASCII digits (no sign), applying `negative`.
    /// Accumulates 9 digits at a time into a `UInt32` chunk, then folds via
    /// `value = value * 10⁹ + chunk` on the magnitude limbs.
    static func parseDecimal(_ digits: Substring, negative: Bool) -> Integer {
        var limbs = [UInt32]()
        let chars = Array(digits)
        var index = 0
        // Lead with the shortest chunk so the rest align to 9-digit groups.
        let firstLen = chars.count % 9 == 0 ? 9 : chars.count % 9
        while index < chars.count {
            let len = index == 0 ? firstLen : 9
            var chunk: UInt32 = 0
            for _ in 0..<len {
                chunk = chunk * 10 + UInt32(chars[index].wholeNumberValue!)
                index += 1
            }
            let scale: UInt32 = len == 9 ? 1_000_000_000 : UInt32(pow10(len))
            limbs = Magnitude.add(Magnitude.multiply(limbs, [scale]), chunk == 0 ? [] : [chunk])
        }
        return pack(negative: negative, limbs)
    }

    private static func pow10(_ n: Int) -> UInt64 {
        var result: UInt64 = 1
        for _ in 0..<n { result *= 10 }
        return result
    }
}
