import BigInt

/// A currency amount — the payload of `Value.money`, a first-class tagged type
/// alongside `FixedInt` (`Int32(…)`) and `FixedDecimal` (`Decimal(…)`). Written
/// as a finance-mode literal (`$10`, `€10`) or the mode-agnostic constructor
/// `Money(10, "USD")`. See docs/MODES.md.
///
/// The currency propagates through arithmetic the way `FixedDecimal`'s type
/// does: money in, money out, with a plain `Number` (or a grouped number)
/// absorbed. That is what makes `$10 * 5%` answer `$0.50` — `5%` has already
/// evaluated to a plain `0.05` by the time the multiply sees it. Two different
/// currencies are refused: there is no exchange rate to apply.
public struct Money: Sendable, Equatable {
    public let value: BigDecimal
    public let currency: Currency

    public init(value: BigDecimal, currency: Currency) {
        self.value = value
        self.currency = currency
    }

    /// "USD amount" — for `kindName` and error messages.
    public var typeName: String { "\(currency.code) amount" }

    /// The display form: grouped, 2 decimals, symbol outside the sign
    /// (`-$1,234.50`, `CHF 10.00`) — matching the sheet's currency format.
    public var text: String {
        let magnitude = value.isNegative ? -value : value
        return (value.isNegative ? "-" : "") + currency.symbol
            + magnitude.groupedText(decimals: 2)
    }

    /// Canonical, re-parseable spelling — the constructor call `Money(10, "USD")`,
    /// which restores by evaluation (like `Decimal(…)` / a record) in ANY mode.
    /// The value is EXACT (not rounded to 2dp), so the round trip is lossless.
    public var description: String { "Money(\(value.description), \"\(currency.code)\")" }
}

// MARK: - Typed arithmetic (the mixing matrix)

extension Money {
    /// True when money arithmetic applies — either operand is a currency amount.
    static func isInvolved(_ lhs: Value, _ rhs: Value) -> Bool {
        if case .money = lhs { return true }
        if case .money = rhs { return true }
        return false
    }

    /// `+ − × ÷` on money operands (docs/MODES.md). The currency propagates
    /// through all four, so a money input always reads back as money — the tag
    /// is a display contract, not a unit system, so it never models
    /// dimensionality (`$10 * $2` is `$20.00`). Two DIFFERENT currencies are
    /// refused. `^` and modulo refuse a currency (convert to a Number first).
    static func applyBinary(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        let currency = try resolvedCurrency(lhs, rhs)
        let a = try operand(lhs)
        let b = try operand(rhs)
        switch op {
        case .add: return .money(Money(value: a + b, currency: currency))
        case .subtract: return .money(Money(value: a - b, currency: currency))
        case .multiply: return .money(Money(value: a * b, currency: currency))
        case .divide: return .money(Money(value: try a / b, currency: currency)) // throws on /0
        case .modulo, .power:
            throw EngineError.domainError(
                message: "a currency amount doesn't support \(op == .power ? "^ (power)" : "modulo") — convert it to a Number first")
        }
    }

    /// The surviving currency. Two different currencies are a hard error; a
    /// plain Number or grouped number yields to the money operand's currency.
    private static func resolvedCurrency(_ lhs: Value, _ rhs: Value) throws -> Currency {
        switch (lhs, rhs) {
        case (.money(let a), .money(let b)):
            guard a.currency == b.currency else {
                throw EngineError.domainError(
                    message: "can't mix currencies (\(a.currency.code) and \(b.currency.code)) — convert one first")
            }
            return a.currency
        case (.money(let a), _): return a.currency
        case (_, .money(let b)): return b.currency
        default:
            throw EngineError.domainError(message: "money arithmetic with no currency operand")
        }
    }

    /// An operand as an exact BigDecimal. Money uses its value; a plain Number
    /// or a grouped number is absorbed. Anything else — a fixed-width int or
    /// decimal — errors (cross-family; convert explicitly).
    private static func operand(_ value: Value) throws -> BigDecimal {
        switch value {
        case .money(let m): return m.value
        case .number(let n): return n
        case .grouped(let n): return n
        default:
            throw EngineError.domainError(
                message: "can't combine \(value.kindName) with a currency amount — convert it first")
        }
    }
}
