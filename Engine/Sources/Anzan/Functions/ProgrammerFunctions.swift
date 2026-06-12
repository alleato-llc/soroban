import BigInt

/// Base conversion and bitwise operations — for the CLI crowd. Everything
/// here is exact: bases ride BigInt (no 2^53 ceiling like a float-backed
/// HEX2DEC), and the bit operations work at arbitrary width.
let programmerFunctions: [BuiltinFunction] = [
    BuiltinFunction(
        name: "toBase",
        category: .programmer,
        signature: "toBase(n, base)",
        summary: "An integer rendered in another base (2–36), as a string — hex is toBase(n, 16), binary toBase(n, 2); uppercase digits, exact at any size. Want a toHex? Define it: toHex(n) = toBase(n, 16).",
        examples: ["toBase(255, 16)", "toBase(10, 2)"],
        arity: 2...2,
        applyValues: { arguments in
            let n = try arguments[0].asNumber(for: "toBase's number")
            let base = try requireInt(arguments[1].asNumber(for: "toBase's base"), "toBase base")
            guard (2...36).contains(base) else {
                throw EngineError.domainError(message: "toBase's base must be 2–36")
            }
            guard let value = n.bigIntValue else {
                throw EngineError.domainError(message: "toBase needs an integer")
            }
            return .string(String(value, radix: base, uppercase: true))
        }),

    BuiltinFunction(
        name: "fromBase",
        category: .programmer,
        signature: "fromBase(text, base)",
        summary: "Parses digits in another base (2–36) into a decimal number — hex to decimal is fromBase(\"ff\", 16) → 255, binary fromBase(\"1010\", 2) → 10. toBase's inverse, exact at any size.",
        examples: ["fromBase(\"ff\", 16)", "fromBase(\"1010\", 2)"],
        arity: 2...2,
        applyValues: { arguments in
            guard case .string(let text) = arguments[0] else {
                throw EngineError.domainError(
                    message: "fromBase wants digits as a string, got \(arguments[0].kindName)")
            }
            let base = try requireInt(arguments[1].asNumber(for: "fromBase's base"), "fromBase base")
            guard (2...36).contains(base) else {
                throw EngineError.domainError(message: "fromBase's base must be 2–36")
            }
            // Hand-rolled digit walk: BigInt's own parser preconditions
            // radix < 36 (crashing on exactly 36), and we want a typed
            // error for bad digits anyway.
            var digits = Substring(text)
            let negative = digits.hasPrefix("-")
            if negative { digits = digits.dropFirst() }
            guard !digits.isEmpty else {
                throw EngineError.domainError(message: "fromBase needs at least one digit")
            }
            var value = BigInt(0)
            for character in digits {
                guard let digit = character.hexDigitValue ?? letterDigit(character),
                      digit < base else {
                    throw EngineError.domainError(
                        message: "\"\(text)\" is not a base-\(base) number")
                }
                value = value * BigInt(base) + BigInt(digit)
            }
            return .number(BigDecimal(significand: negative ? -value : value, exponent: 0))
        }),

    bitFn("bitAnd", "&", "Bitwise AND of non-negative integers — at any width.",
          ["bitAnd(12, 10)", "bitAnd(255, 51, 15)"]) { $0 & $1 },
    bitFn("bitOr", "|", "Bitwise OR of non-negative integers.",
          ["bitOr(12, 10)"]) { $0 | $1 },
    bitFn("bitXor", "⊕", "Bitwise XOR of non-negative integers.",
          ["bitXor(12, 10)"]) { $0 ^ $1 },

    BuiltinFunction(
        name: "bitShift",
        category: .programmer,
        signature: "bitShift(n, by)",
        summary: "Shifts a non-negative integer's bits left (positive) or right (negative) — exact, so bitShift(1, 100) is the full 31-digit power of two.",
        examples: ["bitShift(1, 8)", "bitShift(256, -4)"],
        arity: 2...2) { args in
        let n = try requireBits(args[0], "bitShift")
        let by = try requireInt(args[1], "bitShift amount")
        guard abs(by) <= 10_000 else {
            throw EngineError.domainError(message: "bitShift amount is too large")
        }
        let result = by >= 0 ? n << by : n >> (-by)
        return BigDecimal(significand: result, exponent: 0)
    },
]

/// A variadic bitwise reduction over non-negative integers.
private func bitFn(_ name: String, _ symbol: String, _ summary: String,
                   _ examples: [String],
                   _ op: @escaping @Sendable (BigInt, BigInt) -> BigInt) -> BuiltinFunction {
    BuiltinFunction(name: name, category: .programmer,
                    signature: "\(name)(a, b, …)",
                    summary: summary, examples: examples,
                    arity: 2...Int.max) { args in
        let bits = try args.map { try requireBits($0, name) }
        return BigDecimal(significand: bits.dropFirst().reduce(bits[0], op), exponent: 0)
    }
}

/// Digit value for g–z (hexDigitValue covers 0–9 and a–f).
private func letterDigit(_ character: Character) -> Int? {
    guard let scalar = character.lowercased().unicodeScalars.first,
          ("a"..."z").contains(Character(scalar)) else { return nil }
    return Int(scalar.value - UnicodeScalar("a").value) + 10
}

/// Bit operations are defined here for non-negative integers (no width to
/// two's-complement against).
private func requireBits(_ value: BigDecimal, _ name: String) throws -> BigInt {
    guard let bits = value.bigIntValue, bits.sign == .plus || bits.isZero else {
        throw EngineError.domainError(message: "\(name) needs non-negative integers")
    }
    return bits
}

extension BigDecimal {
    /// "0xC3" / "-0xFF", uppercase — nil when the value isn't an integer.
    /// Public: the hex cell format and both hosts' hex echo (CLI + app log)
    /// render through this.
    public var hexText: String? {
        guard let bits = bigIntValue else { return nil }
        return (bits.sign == .minus ? "-0x" : "0x")
            + String(bits.magnitude, radix: 16, uppercase: true)
    }

    /// "0b1100_0011" — nibble-grouped from the right; nil for non-integers.
    package var binaryText: String? {
        guard let bits = bigIntValue else { return nil }
        let raw = Array(String(bits.magnitude, radix: 2))
        var grouped: [Character] = []
        for (offset, bit) in raw.reversed().enumerated() {
            if offset > 0, offset.isMultiple(of: 4) { grouped.append("_") }
            grouped.append(bit)
        }
        return (bits.sign == .minus ? "-0b" : "0b") + String(grouped.reversed())
    }

    /// The exact BigInt value of an integer — unlike `intValue`, no 2^63
    /// ceiling (a normalized 1e40 has significand 1, exponent 40).
    var bigIntValue: BigInt? {
        guard isInteger else { return nil }
        guard exponent >= 0 else { return nil }
        guard exponent <= 10_000 else { return nil } // refuse absurd widths
        return significand * BigInt(10).power(exponent)
    }
}
