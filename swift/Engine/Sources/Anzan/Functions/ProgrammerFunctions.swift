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
            let signed = negative ? -value : value
            return .number(BigDecimal(significand: Integer(signed.description)!, exponent: 0))
        }),

    BuiltinFunction(
        name: "Int",
        category: .programmer,
        signature: "Int(value, bits)",
        summary: "A signed fixed-width integer of `bits` bits (8/16/32/64/128/256). Checked, not modular: arithmetic that overflows the range is an error, never a wraparound. The per-width forms Int8…Int256 take just the value: Int32(x) ≡ Int(x, 32).",
        examples: ["Int(27374, 32)", "Int(-1, 8)"],
        arity: 2...2,
        applyValues: { try makeFixedInt($0, signed: true) }),

    BuiltinFunction(
        name: "UInt",
        category: .programmer,
        signature: "UInt(value, bits)",
        summary: "An unsigned fixed-width integer of `bits` bits (8/16/32/64/128/256). Checked: overflow or a negative value is an error, never a wraparound. Per-width forms: UInt8…UInt256, e.g. UInt8(x) ≡ UInt(x, 8).",
        examples: ["UInt(255, 8)", "UInt(1000, 16)"],
        arity: 2...2,
        applyValues: { try makeFixedInt($0, signed: false) }),

    bitFn("bitAnd", "&", "Bitwise AND of non-negative integers — at any width. In Programmer mode, the `&` operator (a & b).",
          ["bitAnd(12, 10)", "bitAnd(255, 51, 15)"]) { $0 & $1 },
    bitFn("bitOr", "|", "Bitwise OR of non-negative integers. In Programmer mode, the `|` operator (a | b).",
          ["bitOr(12, 10)"]) { $0 | $1 },
    bitFn("bitXor", "⊕", "Bitwise XOR of non-negative integers. In Programmer mode, the `^` operator (a ^ b) — there ^ is XOR, not power.",
          ["bitXor(12, 10)"]) { $0 ^ $1 },

    BuiltinFunction(
        name: "bitNot",
        category: .programmer,
        signature: "bitNot(x)",
        summary: "Bitwise NOT of a fixed-width integer, two's-complement over its width — bitNot(UInt(0, 8)) is UInt(255, 8), bitNot(Int(0, 8)) is Int(-1, 8). Needs a width, so it's defined on Int()/UInt() values (also the ~ operator in Programmer mode).",
        examples: ["bitNot(UInt8(0))", "bitNot(Int8(0))"],
        arity: 1...1,
        applyValues: { args in
            guard case .fixedInt(let f) = args[0] else {
                throw EngineError.domainError(
                    message: "bitNot needs a fixed-width integer (Int()/UInt()) — its width defines the complement")
            }
            return .fixedInt(try f.bitwiseNot())
        }),

    BuiltinFunction(
        name: "bitShift",
        category: .programmer,
        signature: "bitShift(n, by)",
        summary: "Shifts left (positive `by`) or right (negative). A plain integer shifts exactly at any width (bitShift(1, 100) is the full power of two); a fixed-width int is checked — a left shift whose bits leave the width is an overflow error. In Programmer mode, the `<<` / `>>` operators (a << n, a >> n).",
        examples: ["bitShift(1, 8)", "bitShift(256, -4)"],
        arity: 2...2,
        applyValues: { args in
            let by = try requireInt(args[1].asNumber(for: "bitShift amount"), "bitShift amount")
            guard abs(by) <= 10_000 else {
                throw EngineError.domainError(message: "bitShift amount is too large")
            }
            // Fixed-width: shift within the width, range-checked (left overflow → error).
            if case .fixedInt(let f) = args[0] {
                let shifted = by >= 0 ? f.value << by : f.value >> (-by)
                return .fixedInt(try FixedInt(value: shifted, bits: f.bits, signed: f.signed))
            }
            let n = try requireBits(args[0].asNumber(for: "bitShift"), "bitShift")
            let result = by >= 0 ? n << by : n >> (-by)
            return .number(BigDecimal(significand: Integer(result.description)!, exponent: 0))
        }),
] + integerWidthConstructors

/// Per-width fixed integer constructors — Int8…Int256 / UInt8…UInt256.
/// `Int32(x)` ≡ `Int(x, 32)`. Generated so the width set stays in one place
/// (`FixedInt.allowedWidths`); the per-width spelling is the canonical form.
private let integerWidthConstructors: [BuiltinFunction] = FixedInt.allowedWidths.flatMap { bits in
    [true, false].map { (signed: Bool) -> BuiltinFunction in
        let name = "\(signed ? "Int" : "UInt")\(bits)"
        return BuiltinFunction(
            name: name, category: .programmer,
            signature: "\(name)(value)",
            summary: "A \(signed ? "signed" : "unsigned") \(bits)-bit fixed-width integer — \(name)(x) ≡ \(signed ? "Int" : "UInt")(x, \(bits)). Checked: overflow is an error, never a wraparound.",
            examples: ["\(name)(100)"],
            arity: 1...1,
            applyValues: { args in
                let number = try args[0].asNumber(for: name)
                guard let value = number.bigIntValue else {
                    throw EngineError.domainError(message: "\(name)() needs an integer value, got \(number.description)")
                }
                return .fixedInt(try FixedInt(value: value, bits: bits, signed: signed))
            })
    }
}

/// A variadic bitwise reduction. Plain numbers reduce over non-negative BigInts
/// (exact, any width). When any operand is a fixed-width int the reduction is
/// type-preserving — two's-complement over the (promoted) width, signs must match
/// (docs/FIXED-WIDTH.md).
private func bitFn(_ name: String, _ symbol: String, _ summary: String,
                   _ examples: [String],
                   _ op: @escaping @Sendable (BigInt, BigInt) -> BigInt) -> BuiltinFunction {
    BuiltinFunction(name: name, category: .programmer,
                    signature: "\(name)(a, b, …)",
                    summary: summary, examples: examples,
                    arity: 2...Int.max,
                    applyValues: { rawArguments in
        let arguments = try flattenBitwiseOperands(rawArguments, name)
        if arguments.contains(where: { if case .fixedInt = $0 { return true }; return false }) {
            return try arguments.dropFirst().reduce(arguments[0]) { try FixedInt.applyBitwise($0, $1, op) }
        }
        let bits = try arguments.map { try requireBits($0.asNumber(for: name), name) }
        let combined = bits.dropFirst().reduce(bits[0], op)
        return .number(BigDecimal(significand: Integer(combined.description)!, exponent: 0))
    })
}

/// Flattens array arguments (preserving numbers and fixedInts) so a bitwise
/// reduction accepts `[a, b]` like a range; rejects non-integer kinds.
private func flattenBitwiseOperands(_ values: [Value], _ name: String) throws -> [Value] {
    var out: [Value] = []
    for value in values {
        switch value {
        case .array(let items): out.append(contentsOf: try flattenBitwiseOperands(items, name))
        case .number, .fixedInt: out.append(value)
        default:
            throw EngineError.domainError(message: "\(name)() works on integers, got \(value.kindName)")
        }
    }
    return out
}

/// Digit value for g–z (hexDigitValue covers 0–9 and a–f).
private func letterDigit(_ character: Character) -> Int? {
    guard let scalar = character.lowercased().unicodeScalars.first,
          ("a"..."z").contains(Character(scalar)) else { return nil }
    return Int(scalar.value - UnicodeScalar("a").value) + 10
}

/// Builds a `Value.fixedInt` for the `int`/`uint` constructors — integer value,
/// allowed width, in range (all enforced by `FixedInt.init`).
private func makeFixedInt(_ arguments: [Value], signed: Bool) throws -> Value {
    let name = signed ? "Int" : "UInt"
    let number = try arguments[0].asNumber(for: "\(name)'s value")
    guard let value = number.bigIntValue else {
        throw EngineError.domainError(
            message: "\(name)() needs an integer value, got \(number.description)")
    }
    let bits = try requireInt(arguments[1].asNumber(for: "\(name)'s bit width"), "\(name) bit width")
    return .fixedInt(try FixedInt(value: value, bits: bits, signed: signed))
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
        // Bridge the Integer significand to BigInt for the programmer/bitwise world.
        return BigInt(significand.description)! * BigInt(10).power(exponent)
    }
}
