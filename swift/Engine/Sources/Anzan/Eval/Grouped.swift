import BigInt

/// Thousands-grouped plain numbers (`138,561`) — the payload of `Value.grouped`.
/// Unlike `Money`, grouping is **pure presentation**: `138,561` IS `138561`,
/// with no currency, no unit, no arithmetic rules. It carries only so the
/// grouping ECHOES through a calculation (`138,561 * 9%` → `12,470.49`).
///
/// The tag therefore yields to anything with real meaning: a currency operand
/// absorbs it (Money dispatches first), and `^`/modulo drop it. It survives the
/// four ordinary operators, negation, and percent so the echo stays consistent.
public enum Grouped {
    /// True when either operand is a grouped number — checked AFTER the money /
    /// fixed-width hooks, so a "real" type always wins the dispatch.
    static func isInvolved(_ lhs: Value, _ rhs: Value) -> Bool {
        if case .grouped = lhs { return true }
        if case .grouped = rhs { return true }
        return false
    }

    /// `+ − × ÷` keep the grouping (the result is grouped); `^` and modulo drop
    /// it (the value survives as a plain number).
    static func applyBinary(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        let a = try operand(lhs)
        let b = try operand(rhs)
        switch op {
        case .add: return .grouped(a + b)
        case .subtract: return .grouped(a - b)
        case .multiply: return .grouped(a * b)
        case .divide: return .grouped(try a / b) // throws on /0
        case .power: return .number(try Functions.pow(a, b))
        case .modulo: return .number(try a % b)
        }
    }

    /// A grouped or plain number as its exact value; anything else errors (but a
    /// typed operand never reaches here — it dispatches first).
    private static func operand(_ value: Value) throws -> BigDecimal {
        switch value {
        case .grouped(let n): return n
        case .number(let n): return n
        default:
            throw EngineError.domainError(
                message: "can't combine \(value.kindName) with a grouped number")
        }
    }
}
