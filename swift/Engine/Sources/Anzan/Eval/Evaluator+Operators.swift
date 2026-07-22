// Operator and subscript application: subscripting (arrays/strings/maps/records/
// host handles), binary arithmetic (with the fixed-width int / fixed-decimal
// hooks and string concatenation), and comparison.

extension Evaluator {
    /// `arr[0]` (0-based), `"abc"[0]`, `m["key"]`.
    func subscriptValue(_ base: Value, by index: Value) throws -> Value {
        switch base {
        case .array(let items):
            let position = try requireInt(index.asNumber(for: "an array index"), "array index")
            guard items.indices.contains(position) else {
                throw EngineError.domainError(
                    message: "index \(position) is out of range (array has \(items.count) element\(items.count == 1 ? "" : "s"))")
            }
            return items[position]

        case .string(let text):
            let position = try requireInt(index.asNumber(for: "a string index"), "string index")
            guard position >= 0, position < text.count else {
                throw EngineError.domainError(
                    message: "index \(position) is out of range (string has \(text.count) character\(text.count == 1 ? "" : "s"))")
            }
            return .string(String(text[text.index(text.startIndex, offsetBy: position)]))

        case .map, .record:
            guard case .string(let key) = index else {
                throw EngineError.domainError(
                    message: "map keys are strings — e.g. m[\"name\"], got \(index.kindName)")
            }
            guard let value = base.mapValue(forKey: key) else {
                if case .record(let record) = base {
                    throw EngineError.domainError(
                        message: "\(record.typeName) has no field '\(key)' — it has "
                            + record.entries.map(\.key).joined(separator: ", "))
                }
                throw EngineError.domainError(message: "no key '\(key)' in map")
            }
            return value

        case .host(let object):
            // Host handles define their own indexing (Worksheets[0] / ["Budget"]).
            guard let value = object.index(index) else {
                throw EngineError.domainError(
                    message: "\(object.typeName) can't be indexed by \(index.kindName)")
            }
            return value

        case .number, .fixedInt, .fixedDecimal, .money, .grouped, .function:
            throw EngineError.domainError(message: "\(base.kindName) can't be indexed")
        }
    }

    func apply(_ op: BinaryOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        // `+` concatenates as soon as either side is a string — "Q" + 1 is "Q1".
        if op == .add, isString(lhs) || isString(rhs) {
            return .string(lhs.displayText + rhs.displayText)
        }
        // Fixed-width integer arithmetic: the mixing matrix + checked overflow
        // (docs/FIXED-WIDTH.md). Numeric (non-fixedInt) operands skip this and
        // take the exact-decimal path below, unchanged.
        if FixedInt.isInvolved(lhs, rhs) {
            return try FixedInt.applyBinary(op, lhs, rhs)
        }
        // Fixed-precision decimal arithmetic — the money-type mixing matrix.
        if FixedDecimal.isInvolved(lhs, rhs) {
            return try FixedDecimal.applyBinary(op, lhs, rhs)
        }
        // Finance-mode currency (docs/MODES.md) — the currency propagates and a
        // plain (or grouped) Number is absorbed, so `$10 * 5%` is `$0.50`.
        // Money runs before Grouped, so money + grouped → money.
        if Money.isInvolved(lhs, rhs) {
            return try Money.applyBinary(op, lhs, rhs)
        }
        // Grouped plain numbers — formatting only; the grouping echoes through.
        if Grouped.isInvolved(lhs, rhs) {
            return try Grouped.applyBinary(op, lhs, rhs)
        }
        let a = try lhs.asNumber(for: op.rawValue)
        let b = try rhs.asNumber(for: op.rawValue)
        switch op {
        case .add: return .number(a + b)
        case .subtract: return .number(a - b)
        case .multiply: return .number(a * b)
        case .divide: return .number(try a / b)
        case .modulo: return .number(try a % b)
        case .power: return .number(try Functions.pow(a, b))
        }
    }

    func isString(_ value: Value) -> Bool {
        if case .string = value { return true }
        return false
    }

    /// `==`/`!=` are deep equality on any values; ordering needs numbers.
    func compare(_ op: ComparisonOperator, _ lhs: Value, _ rhs: Value) throws -> Value {
        switch op {
        case .equal: return .bool(lhs == rhs)
        case .notEqual: return .bool(lhs != rhs)
        case .less, .greater, .lessOrEqual, .greaterOrEqual:
            let a = try lhs.asNumber(for: op.rawValue)
            let b = try rhs.asNumber(for: op.rawValue)
            switch op {
            case .less: return .bool(a < b)
            case .greater: return .bool(a > b)
            case .lessOrEqual: return .bool(a <= b)
            case .greaterOrEqual: return .bool(a >= b)
            case .equal, .notEqual: fatalError("handled above")
            }
        }
    }
}
