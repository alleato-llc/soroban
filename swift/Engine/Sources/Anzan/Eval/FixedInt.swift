import BigInt

/// A bounded, checked integer value — the payload of `Value.fixedInt`, built by
/// the `Int8…Int256` / `UInt8…UInt256` per-width constructors (or `Int(value, bits)`
/// / `UInt(value, bits)`). Exact like the rest
/// of the engine, but with a declared width: arithmetic that leaves the range is
/// an ERROR, never a wraparound. See `docs/FIXED-WIDTH.md`.
public struct FixedInt: Sendable, Equatable {
    public let value: BigInt
    public let bits: Int
    public let signed: Bool

    /// Allowed widths. Cheap underneath (everything is `BigInt`); the set is a
    /// deliberate, documented choice rather than a representational limit.
    public static let allowedWidths = [8, 16, 32, 64, 128, 256]

    /// Validating constructor: rejects a non-allowed width or an out-of-range
    /// value (the checked-range contract — there is no silent truncation).
    public init(value: BigInt, bits: Int, signed: Bool) throws(EngineError) {
        guard Self.allowedWidths.contains(bits) else {
            throw EngineError.domainError(
                message: "fixed-width needs a width of 8, 16, 32, 64, 128, or 256 — got \(bits)")
        }
        let lo = Self.minValue(bits: bits, signed: signed)
        let hi = Self.maxValue(bits: bits, signed: signed)
        guard value >= lo, value <= hi else {
            throw EngineError.domainError(
                message: "\(value) is out of range for \(Self.typeName(bits: bits, signed: signed)) "
                    + "(\(lo) … \(hi))")
        }
        self.value = value
        self.bits = bits
        self.signed = signed
    }

    static func minValue(bits: Int, signed: Bool) -> BigInt {
        signed ? -(BigInt(1) << (bits - 1)) : BigInt(0)
    }
    static func maxValue(bits: Int, signed: Bool) -> BigInt {
        signed ? (BigInt(1) << (bits - 1)) - 1 : (BigInt(1) << bits) - 1
    }

    static func typeName(bits: Int, signed: Bool) -> String {
        "\(signed ? "Int" : "UInt")\(bits)"
    }
    /// e.g. "Int32", "UInt8" — for error messages and `kindName`.
    public var typeName: String { Self.typeName(bits: bits, signed: signed) }

    /// Canonical, re-parseable constructor spelling — the per-width form
    /// `Int32(27374)` / `UInt8(255)`. Restores by *evaluation* (like a
    /// record); the parameterized `Int(v, bits)` form re-parses to it too.
    public var description: String {
        "\(typeName)(\(value))"
    }

    /// The plain decimal value — for comparison, truthiness, and numeric
    /// coercion *outside* typed arithmetic (typed arithmetic stays in `apply`).
    public var decimal: BigDecimal {
        BigDecimal(significand: Integer(value.description)!, exponent: 0)
    }
}

// MARK: - Typed arithmetic (the mixing matrix + checked overflow)

extension FixedInt {
    /// True when fixed-width arithmetic applies — either operand is a fixedInt.
    /// The evaluator routes to `applyBinary` in that case, before numeric coercion.
    static func isInvolved(_ lhs: Value, _ rhs: Value) -> Bool {
        if case .fixedInt = lhs { return true }
        if case .fixedInt = rhs { return true }
        return false
    }

    /// `+ − × ÷` and `^`-power on fixed-width operands (docs/FIXED-WIDTH.md):
    /// width promotes toward the widest type present; sign never promotes
    /// (mismatch → error); a `decimal` non-integer never mixes; every result is
    /// range-checked and **errors rather than wraps**. Precondition:
    /// `isInvolved(lhs, rhs)`.
    static func applyBinary(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        // Power is special: the exponent is a COUNT (exempt from the matrix); the
        // result follows the base. A numeric base with a fixed-width exponent is
        // just ordinary numeric power.
        if op == .power { return try applyPower(lhs, rhs) }

        let type = try resolvedType(lhs, rhs)
        let a = try operand(lhs, as: type)
        let b = try operand(rhs, as: type)
        let raw: BigInt
        switch op {
        case .add: raw = a + b
        case .subtract: raw = a - b
        case .multiply: raw = a * b
        case .divide:
            guard !b.isZero else { throw EngineError.divisionByZero }
            raw = a / b                 // truncating toward zero, like C/Rust
        case .modulo:
            guard !b.isZero else { throw EngineError.divisionByZero }
            raw = a % b
        case .power:
            raw = a                     // unreachable — handled above
        }
        return .fixedInt(try FixedInt(value: raw, bits: type.bits, signed: type.signed))
    }

    private static func applyPower(_ lhs: Value, _ rhs: Value) throws -> Value {
        let exponentValue = try rhs.asNumber(for: "^")
        guard case .fixedInt(let base) = lhs else {
            // Numeric base, fixed-width exponent → ordinary numeric power.
            return .number(try Functions.pow(try lhs.asNumber(for: "^"), exponentValue))
        }
        guard let expBig = exponentValue.bigIntValue, expBig >= BigInt(0),
              let exponent = Int(exactly: expBig) else {
            throw EngineError.domainError(
                message: "a fixed-width base needs a non-negative integer exponent")
        }
        return .fixedInt(try FixedInt(value: base.value.power(exponent),
                                      bits: base.bits, signed: base.signed))
    }

    /// Result type: largest width wins; sign never promotes (mismatch → error).
    private static func resolvedType(_ lhs: Value, _ rhs: Value) throws -> (bits: Int, signed: Bool) {
        switch (lhs, rhs) {
        case (.fixedInt(let a), .fixedInt(let b)):
            guard a.signed == b.signed else {
                throw EngineError.domainError(
                    message: "can't mix \(a.typeName) and \(b.typeName) — signed and unsigned never combine; cast one explicitly")
            }
            return (max(a.bits, b.bits), a.signed)
        case (.fixedInt(let a), _): return (a.bits, a.signed)
        case (_, .fixedInt(let b)): return (b.bits, b.signed)
        default:
            throw EngineError.domainError(message: "fixed-width arithmetic with no fixed-width operand")
        }
    }

    /// An operand as a BigInt in the result type. A fixedInt uses its value (same
    /// sign guaranteed; a smaller width fits the larger). A plain number must be a
    /// whole number and **adopts** the type — range-checked, so an out-of-range or
    /// fractional literal **errors** (no silent truncation, no decimal mixing).
    private static func operand(_ value: Value, as type: (bits: Int, signed: Bool)) throws -> BigInt {
        switch value {
        case .fixedInt(let f):
            return f.value
        case .number(let n):
            guard let i = n.bigIntValue else {
                throw EngineError.domainError(
                    message: "fixed-width arithmetic needs whole numbers — \(n.description) isn't an integer")
            }
            return try FixedInt(value: i, bits: type.bits, signed: type.signed).value
        default:
            throw EngineError.domainError(
                message: "can't combine \(value.kindName) with a fixed-width integer")
        }
    }
}

// MARK: - Bitwise (two's-complement over the width)

extension FixedInt {
    /// AND/OR/XOR on fixed-width operands, type-preserving. Signed values operate
    /// in two's-complement over the (promoted) width; the result is in range by
    /// construction. Shares the arithmetic mixing matrix (largest width, sign must
    /// match, a plain number adopts the type). `op` is the BigInt bit operation.
    static func applyBitwise(_ lhs: Value, _ rhs: Value,
                             _ op: (BigInt, BigInt) -> BigInt) throws -> Value {
        let type = try resolvedType(lhs, rhs)
        let result = op(try pattern(lhs, as: type), try pattern(rhs, as: type))
        return .fixedInt(try FixedInt(value: decode(result, type), bits: type.bits, signed: type.signed))
    }

    /// Bitwise NOT over the width: `~x` flips every bit. `~Int8(0)` → `Int8(-1)`
    /// (= −x−1); `~UInt8(0)` → `UInt8(255)`. In range by construction.
    func bitwiseNot() throws -> FixedInt {
        let width = BigInt(1) << bits
        let pat = value < 0 ? value + width : value
        let complement = (width - 1) ^ pat
        return try FixedInt(value: Self.decode(complement, (bits, signed)), bits: bits, signed: signed)
    }

    /// A value's unsigned two's-complement bit pattern in [0, 2^bits). A negative
    /// signed value wraps into range; a plain number adopts + is range-checked.
    private static func pattern(_ value: Value, as type: (bits: Int, signed: Bool)) throws -> BigInt {
        let raw = try operand(value, as: type)
        return raw < 0 ? raw + (BigInt(1) << type.bits) : raw
    }

    /// Reverses `pattern`: a high bit means negative for a signed type.
    private static func decode(_ pattern: BigInt, _ type: (bits: Int, signed: Bool)) -> BigInt {
        if type.signed, pattern >= (BigInt(1) << (type.bits - 1)) {
            return pattern - (BigInt(1) << type.bits)
        }
        return pattern
    }
}
