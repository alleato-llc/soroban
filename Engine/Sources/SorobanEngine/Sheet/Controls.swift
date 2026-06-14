import Anzan
/// What-if sliders, the first CONTROL EXPRESSION: a cell whose expression is
/// a literal-argument `slider(value, min, max[, step])` call renders as a
/// draggable control, and dragging rewrites the value literal in place.
/// Combined with 𝑖 definitions (`rate = slider(0.08, 0, 0.2)`) the control
/// is named, sheet-scoped, and immutable from the log — all inherited.
///
/// The pattern is deliberately general: checkbox/stepper/dropdown can follow
/// it later (a control call with a literal "storage" argument, rewritten by
/// interaction).

/// Everything the grid needs to draw and drag one slider.
public struct SliderInfo: Equatable, Sendable {
    /// The 𝑖 name when the slider is a definition; nil for an anonymous
    /// `=slider(…)` cell (read it by address instead).
    public let name: String?
    /// Clamped into minimum...maximum.
    public let value: BigDecimal
    public let minimum: BigDecimal
    public let maximum: BigDecimal
    /// Explicit 4th argument, or (max−min)/100 — an exact exponent shift.
    public let step: BigDecimal

    /// Builds from a `slider(…)`/`stepper(…)` call whose arguments are all
    /// numeric LITERALS (the value argument IS the storage — it can't be an
    /// expression). Returns nil for any other shape; invalid ranges fall
    /// through to normal evaluation, which reports the error.
    /// Default step: (max−min)/100 for sliders, 1 for steppers.
    static func extract(from expression: Expression, name: String?,
                        function: String = "slider") -> SliderInfo? {
        guard case .call(let callName, let arguments) = expression,
              callName.lowercased() == function,
              (3...4).contains(arguments.count) else { return nil }

        var literals: [BigDecimal] = []
        for argument in arguments {
            guard case .number(let literal)? = Control.literalValue(argument) else { return nil }
            literals.append(literal)
        }
        let minimum = literals[1], maximum = literals[2]
        guard minimum < maximum else { return nil } // evaluation reports this
        if literals.count == 4, !(literals[3] > .zero) { return nil }

        let span = maximum - minimum
        let step: BigDecimal
        if literals.count == 4 {
            step = literals[3]
        } else if function == "stepper" {
            step = .one
        } else {
            step = BigDecimal(significand: span.significand, exponent: span.exponent - 2)
        }
        return SliderInfo(name: name,
                          value: min(max(literals[0], minimum), maximum),
                          minimum: minimum, maximum: maximum, step: step)
    }

    /// The value `fraction` (0...1) of the way along the track, quantized to
    /// `step` (from the minimum) and clamped — what a drag position means.
    public func value(atFraction fraction: Double) -> BigDecimal {
        let clamped = Swift.min(Swift.max(fraction, 0), 1)
        let span = maximum - minimum
        // steps = round(fraction × span / step); Double is fine HERE because
        // it only picks which step — the returned value is exact step math.
        guard let spanValue = Double(span.description),
              let stepValue = Double(step.description), stepValue > 0 else { return minimum }
        let steps = (clamped * spanValue / stepValue).rounded()
        let candidate = minimum + step * BigDecimal(Int(steps))
        return Swift.min(Swift.max(candidate, minimum), maximum)
    }

    /// Where the knob sits, 0...1.
    public var fraction: Double {
        let span = maximum - minimum
        guard let spanValue = Double(span.description), spanValue > 0,
              let offset = Double((value - minimum).description) else { return 0 }
        return Swift.min(Swift.max(offset / spanValue, 0), 1)
    }

    /// The widest value label this slider can show in `format` — the grid
    /// reserves this width so the track never resizes mid-drag. The worst case
    /// of integer digits × step decimals is one of min/max and their one-step
    /// neighbors; the current value covers a long resting literal.
    public func widestValueText(format: NumberFormat) -> String {
        [minimum, minimum + step, maximum - step, maximum, value]
            .map(format.rendered).max { $0.count < $1.count } ?? ""
    }
}

/// A checkbox cell: `flag = checkbox(true)`. Clicking flips the literal.
public struct CheckboxInfo: Equatable, Sendable {
    public let name: String?
    public let isOn: Bool

    static func extract(from expression: Expression, name: String?) -> CheckboxInfo? {
        guard case .call(let callName, let arguments) = expression,
              callName.lowercased() == "checkbox", arguments.count == 1,
              case .number(let state)? = Control.literalValue(arguments[0]) else { return nil }
        return CheckboxInfo(name: name, isOn: !state.isZero)
    }
}

/// A dropdown cell: `region = dropdown("EU", ["EU", "US", "APAC"])`. The
/// cell's value IS the selected option; choosing rewrites the literal.
/// Options are literals too — strings or numbers.
public struct DropdownInfo: Equatable, Sendable {
    public let name: String?
    public let value: Value
    public let options: [Value]

    static func extract(from expression: Expression, name: String?) -> DropdownInfo? {
        guard case .call(let callName, let arguments) = expression,
              callName.lowercased() == "dropdown", arguments.count == 2,
              let value = Control.literalValue(arguments[0]),
              case .arrayLiteral(let items) = arguments[1], !items.isEmpty else { return nil }
        var options: [Value] = []
        for item in items {
            guard let option = Control.literalValue(item) else { return nil }
            options.append(option)
        }
        return DropdownInfo(name: name, value: value, options: options)
    }
}

public enum Control {
    static let names: Set<String> = ["slider", "stepper", "checkbox", "dropdown"]

    /// The literal forms a control's storage argument may take: numbers
    /// (optionally signed), true/false, and "strings".
    static func literalValue(_ expression: Expression) -> Value? {
        switch expression {
        case .number(let value): return .number(value)
        case .unaryMinus(.number(let value)): return .number(-value)
        case .stringLiteral(let text): return .string(text)
        case .variable(let name) where name.lowercased() == "true": return .number(.one)
        case .variable(let name) where name.lowercased() == "false": return .number(.zero)
        default: return nil
        }
    }

    /// The cell's control, if its content is a control expression: either a
    /// 𝑖 definition whose body is a control call (named) or a plain/`=`
    /// formula that IS one (anonymous).
    static func display(for cell: Cell) -> CellDisplay? {
        let expression: Expression
        let name: String?
        switch cell.content {
        case .definition(let definition):
            guard case .variable(let body) = definition.kind else { return nil }
            expression = body
            name = definition.name
        case .explicitFormula(.success(let body)), .candidate(let body):
            expression = body
            name = nil
        default:
            return nil
        }

        guard case .call(let callName, _) = expression else { return nil }
        switch callName.lowercased() {
        case "slider":
            return SliderInfo.extract(from: expression, name: name).map(CellDisplay.slider)
        case "stepper":
            return SliderInfo.extract(from: expression, name: name, function: "stepper")
                .map(CellDisplay.stepper)
        case "checkbox":
            return CheckboxInfo.extract(from: expression, name: name).map(CellDisplay.checkbox)
        case "dropdown":
            return DropdownInfo.extract(from: expression, name: name).map(CellDisplay.dropdown)
        default:
            return nil
        }
    }

    /// Rewrites a control's STORAGE argument literal inside the raw cell
    /// text, leaving everything else — spacing, the 𝑖 name, trailing
    /// `# comments` — intact. `literal` is the replacement source text
    /// (`0.11`, `true`, `"US"`). Token-precise via the lexer's ranges.
    public static func rewriting(_ raw: String, toLiteral literal: String) -> String? {
        guard let tokens = try? Lexer.tokenize(raw) else { return nil }
        for (index, token) in tokens.enumerated() {
            guard case .identifier(let name) = token.kind, names.contains(name.lowercased()),
                  index + 2 < tokens.count,
                  case .leftParen = tokens[index + 1].kind else { continue }

            var start = index + 2
            let range: Range<Int>
            switch tokens[start].kind {
            case .number, .string:
                range = tokens[start].range
            case .identifier(let word) where ["true", "false"].contains(word.lowercased()):
                range = tokens[start].range
            case .minus:
                start += 1
                guard start < tokens.count, case .number = tokens[start].kind else { return nil }
                range = tokens[start - 1].range.lowerBound..<tokens[start].range.upperBound
            default:
                return nil
            }

            var characters = Array(raw)
            characters.replaceSubrange(range, with: literal)
            return String(characters)
        }
        return nil
    }
}

public enum Slider {
    /// Numeric convenience kept for slider drags (and their tests).
    public static func rewriting(_ raw: String, to newValue: BigDecimal) -> String? {
        Control.rewriting(raw, toLiteral: newValue.description)
    }
}
