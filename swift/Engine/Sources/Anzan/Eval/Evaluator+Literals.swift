// Evaluating collection literals (arrays/maps), the argument list (ranges
// expand in place), and the indexed ∑/∏ reduction.

extension Evaluator {
    func arrayValue(_ items: [Expression], in environment: EvaluationEnvironment,
                    locals: [String: Value], depth: Int) throws -> Value {
        var values: [Value] = []
        values.reserveCapacity(items.count)
        for item in items {
            values.append(try evaluate(item, in: environment, locals: locals, depth: depth))
        }
        return .array(values)
    }

    func mapValue(_ entries: [MapLiteralEntry], in environment: EvaluationEnvironment,
                  locals: [String: Value], depth: Int) throws -> Value {
        var values: [Value.MapEntry] = []
        values.reserveCapacity(entries.count)
        for entry in entries {
            values.append(Value.MapEntry(
                key: entry.key,
                value: try evaluate(entry.value, in: environment, locals: locals, depth: depth)))
        }
        return .map(values)
    }

    /// Evaluates a call's arguments; ranges expand in place, so
    /// `sum(A:1..A:9, 10)` sees ≤10 numbers. (Internal, not private:
    /// the tail-call walker in Evaluator+Recursion.swift shares it.)
    func arguments(of expressions: [Expression], in environment: EvaluationEnvironment,
                   locals: [String: Value], depth: Int) throws -> [Value] {
        var arguments: [Value] = []
        arguments.reserveCapacity(expressions.count)
        for expr in expressions {
            if case .cellRange(let sheet, let fc, let fr, let tc, let tr) = expr {
                guard let resolveRange else {
                    throw EngineError.domainError(message: "no sheet available for ranges")
                }
                arguments.append(contentsOf: try resolveRange(sheet, fc, fr, tc, tr)
                    .map(Value.number))
            } else {
                arguments.append(try evaluate(expr, in: environment, locals: locals, depth: depth))
            }
        }
        return arguments
    }

    func reduce(_ operation: ReductionOperation, index: String,
                lowerExpr: Expression, upperExpr: Expression, bodyExpr: Expression,
                in environment: EvaluationEnvironment,
                locals: [String: Value], depth: Int) throws -> Value {
        let symbol = operation.symbol
        let lower = try requireInt(
            evaluate(lowerExpr, in: environment, locals: locals, depth: depth)
                .asNumber(for: "the \(symbol) lower bound"),
            "\(symbol) lower bound")
        let upper = try requireInt(
            evaluate(upperExpr, in: environment, locals: locals, depth: depth)
                .asNumber(for: "the \(symbol) upper bound"),
            "\(symbol) upper bound")
        // Empty range, by convention: ∑ → 0 (additive identity),
        // ∏ → 1 (multiplicative identity).
        let identity: BigDecimal = operation == .sum ? .zero : .one
        guard lower <= upper else { return .number(identity) }

        let (span, overflow) = upper.subtractingReportingOverflow(lower)
        guard !overflow, span < 100_000 else {
            throw EngineError.domainError(message: "\(symbol) spans more than 100,000 terms")
        }

        var total = identity
        var iterationLocals = locals
        for i in lower...upper {
            iterationLocals[index] = .number(BigDecimal(i)) // shadows globals, like a parameter
            let term = try evaluate(bodyExpr, in: environment,
                                    locals: iterationLocals, depth: depth)
                .asNumber(for: "the \(symbol) term")
            total = operation == .sum ? total + term : total * term
        }
        return .number(total)
    }
}
