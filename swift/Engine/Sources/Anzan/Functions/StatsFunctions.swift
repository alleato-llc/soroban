let statsFunctions: [BuiltinFunction] = [
    BuiltinFunction(name: "sum", category: .stats,
                    signature: "sum(x, y, …)",
                    summary: "Adds the arguments. ∑(…) is the same; accepts cell ranges.",
                    examples: ["sum(1, 2, 3)", "sum(B:1..B:3)"],
                    arity: 1...Int.max) { args in
        args.reduce(.zero, +)
    },

    // ∏(…) lands here, the way ∑(…) lands on sum.
    BuiltinFunction(name: "product", category: .stats,
                    signature: "product(x, y, …)",
                    summary: "Multiplies the arguments. ∏(…) is the same; accepts cell ranges.",
                    examples: ["product(2, 3, 4)"],
                    arity: 1...Int.max) { args in
        args.reduce(.one, *)
    },

    // How many numbers — meaningful with ranges: count(A:1..A:99) skips
    // empty/text cells during expansion. Zero when a range expands empty.
    BuiltinFunction(name: "count", category: .stats,
                    signature: "count(…)",
                    summary: "How many numbers — over a range, empty and text cells are skipped.",
                    examples: ["count(B:1..B:9)"],
                    arity: 0...Int.max) { args in
        BigDecimal(args.count)
    },

    BuiltinFunction(name: "avg", category: .stats,
                    signature: "avg(x, y, …)",
                    summary: "Arithmetic mean. Over a range, empty/text cells are skipped.",
                    examples: ["avg(1, 2, 3, 4)", "avg(B:1..B:3)"],
                    arity: 1...Int.max) { args in
        try args.reduce(.zero, +) / BigDecimal(args.count)
    },

    BuiltinFunction(name: "median", category: .stats,
                    signature: "median(x, y, …)",
                    summary: "Middle value (mean of the two middle values for even counts).",
                    examples: ["median(5, 1, 3)", "median(4, 1, 3, 2)"],
                    arity: 1...Int.max) { args in
        let sorted = args.sorted()
        let mid = sorted.count / 2
        if sorted.count.isMultiple(of: 2) {
            return try (sorted[mid - 1] + sorted[mid]) / BigDecimal(2)
        }
        return sorted[mid]
    },

    // Sample standard deviation (n − 1 denominator, like spreadsheet STDEV).
    BuiltinFunction(name: "stdev", category: .stats,
                    signature: "stdev(x, y, …)",
                    summary: "Sample standard deviation (n − 1 denominator, like spreadsheet STDEV).",
                    examples: ["stdev(2, 4, 4, 4, 5, 5, 7, 9)"],
                    arity: 2...Int.max) { args in
        let mean = try args.reduce(.zero, +) / BigDecimal(args.count)
        let squares = args.reduce(BigDecimal.zero) { sum, x in
            sum + (x - mean) * (x - mean)
        }
        return try (squares / BigDecimal(args.count - 1)).squareRoot()
    },

    // stdev's square — same sample convention.
    BuiltinFunction(name: "variance", category: .stats,
                    signature: "variance(x, y, …)",
                    summary: "Sample variance (n − 1 denominator — stdev squared).",
                    examples: ["variance(2, 4, 4, 4, 5, 5, 7, 9)"],
                    arity: 2...Int.max) { args in
        let mean = try args.reduce(.zero, +) / BigDecimal(args.count)
        let squares = args.reduce(BigDecimal.zero) { sum, x in
            sum + (x - mean) * (x - mean)
        }
        return try squares / BigDecimal(args.count - 1)
    },

    BuiltinFunction(name: "mode", category: .stats,
                    signature: "mode(x, y, …)",
                    summary: "The most frequent value; ties go to the first one seen. Errors when nothing repeats.",
                    examples: ["mode(1, 2, 2, 3, 3, 3)"],
                    arity: 1...Int.max) { args in
        var best: (value: BigDecimal, count: Int)?
        for value in args {
            let count = args.count(where: { $0 == value })
            if count > (best?.count ?? 1) {
                best = (value, count)
            }
        }
        guard let best else {
            throw EngineError.domainError(message: "mode: no value repeats")
        }
        return best.value
    },

    // The trailing argument is p — the rest is the data, so a range reads
    // naturally: percentile(A:1..A:99, 0.9).
    BuiltinFunction(name: "percentile", category: .stats,
                    signature: "percentile(data…, p)",
                    summary: "The value below which a fraction p (0–1) of the data falls, with linear interpolation (spreadsheet PERCENTILE.INC). The LAST argument is p; everything before it is the data.",
                    examples: ["percentile(1, 2, 3, 4, 0.75)", "percentile(15, 20, 35, 40, 50, 0.4)"],
                    arity: 2...Int.max) { args in
        let p = args[args.count - 1]
        let data = args.dropLast().sorted()
        guard !p.isNegative, p <= .one else {
            throw EngineError.domainError(message: "percentile's p must be between 0 and 1")
        }
        // rank = p(n − 1); interpolate between the straddling values.
        let rank = p * BigDecimal(data.count - 1)
        let lower = rank.rounded(.down)
        guard let index = lower.intValue else {
            throw EngineError.domainError(message: "percentile is undefined here")
        }
        let fraction = rank - lower
        if fraction.isZero || index + 1 >= data.count {
            return data[index]
        }
        return data[index] + (data[index + 1] - data[index]) * fraction
    },

    BuiltinFunction(name: "geomean", category: .stats,
                    signature: "geomean(x, y, …)",
                    summary: "Geometric mean — the n-th root of the product; all values must be positive. The right average for growth rates.",
                    examples: ["geomean(4, 9)", "geomean(2, 4, 8)"],
                    arity: 1...Int.max) { args in
        for value in args where value.isZero || value.isNegative {
            throw EngineError.domainError(message: "geomean needs positive values")
        }
        return try nthRoot(args.reduce(.one, *), args.count)
    },

    // Paired-series functions split their arguments evenly — the xnpv/xirr
    // convention, so two equal-length ranges read naturally.
    BuiltinFunction(name: "correl", category: .stats,
                    signature: "correl(xs…, ys…)",
                    summary: "Pearson correlation of two equal-length series — the x values then the y values, split evenly (pass two equal ranges).",
                    examples: ["correl(1, 2, 3, 2, 4, 6)"],
                    arity: 4...Int.max) { args in
        let (xs, ys) = try splitPairs(args, "correl")
        let (mx, my) = try (mean(xs), mean(ys))
        var (sxy, sxx, syy) = (BigDecimal.zero, BigDecimal.zero, BigDecimal.zero)
        for (x, y) in zip(xs, ys) {
            sxy = sxy + (x - mx) * (y - my)
            sxx = sxx + (x - mx) * (x - mx)
            syy = syy + (y - my) * (y - my)
        }
        guard !sxx.isZero, !syy.isZero else {
            throw EngineError.domainError(message: "correl is undefined when a series is constant")
        }
        return try sxy / (sxx * syy).squareRoot()
    },

    BuiltinFunction(name: "slope", category: .stats,
                    signature: "slope(ys…, xs…)",
                    summary: "Slope of the least-squares line through (x, y) points — y values first, then x values (spreadsheet argument order), split evenly.",
                    examples: ["slope(2, 4, 6, 1, 2, 3)"],
                    arity: 4...Int.max) { args in
        let (ys, xs) = try splitPairs(args, "slope")
        return try regression(xs: xs, ys: ys).slope
    },

    BuiltinFunction(name: "intercept", category: .stats,
                    signature: "intercept(ys…, xs…)",
                    summary: "Intercept of the least-squares line — y values first, then x values, split evenly.",
                    examples: ["intercept(3, 5, 7, 1, 2, 3)"],
                    arity: 4...Int.max) { args in
        let (ys, xs) = try splitPairs(args, "intercept")
        return try regression(xs: xs, ys: ys).intercept
    },

    BuiltinFunction(name: "forecast", category: .stats,
                    signature: "forecast(x, ys…, xs…)",
                    summary: "Predicts y at x from the least-squares line through the data — x first, then the y values, then the x values (split evenly).",
                    examples: ["forecast(4, 2, 4, 6, 1, 2, 3)"],
                    arity: 5...Int.max) { args in
        let (ys, xs) = try splitPairs(Array(args.dropFirst()), "forecast")
        let line = try regression(xs: xs, ys: ys)
        return line.intercept + line.slope * args[0]
    },

    // Excel's classic, with both calling shapes: arrays (sumproduct(a, b))
    // or one flat even list (two equal ranges expand to exactly that).
    BuiltinFunction(name: "sumproduct", category: .stats,
                    signature: "sumproduct(xs…, ys…)",
                    summary: "Sum of elementwise products of two equal-length series — split evenly, so sumproduct(A:1..A:9, B:1..B:9) is the classic. Arrays work too: sumproduct(prices, quantities).",
                    examples: ["sumproduct(1, 2, 3, 4, 5, 6)", "sumproduct([2, 3], [10, 100])"],
                    arity: 2...Int.max) { args in
        let (xs, ys) = try splitPairs(args, "sumproduct")
        return zip(xs, ys).reduce(BigDecimal.zero) { sum, pair in
            sum + pair.0 * pair.1
        }
    },
]

// MARK: - Helpers

/// Splits an even-length argument list into its two series (the xnpv/xirr
/// convention for paired data: pass two equal-length ranges).
private func splitPairs(_ args: [BigDecimal],
                        _ name: String) throws -> ([BigDecimal], [BigDecimal]) {
    guard args.count.isMultiple(of: 2) else {
        throw EngineError.domainError(
            message: "\(name) wants two equal-length series — got \(args.count) values")
    }
    let half = args.count / 2
    return (Array(args[..<half]), Array(args[half...]))
}

private func mean(_ values: [BigDecimal]) throws -> BigDecimal {
    try values.reduce(.zero, +) / BigDecimal(values.count)
}

/// Least-squares line through the points; exact sums, working-precision
/// division.
private func regression(xs: [BigDecimal],
                        ys: [BigDecimal]) throws -> (slope: BigDecimal, intercept: BigDecimal) {
    let (mx, my) = try (mean(xs), mean(ys))
    var (sxy, sxx) = (BigDecimal.zero, BigDecimal.zero)
    for (x, y) in zip(xs, ys) {
        sxy = sxy + (x - mx) * (y - my)
        sxx = sxx + (x - mx) * (x - mx)
    }
    guard !sxx.isZero else {
        throw EngineError.domainError(message: "the x values are constant — the line is vertical")
    }
    let slope = try sxy / sxx
    return (slope, my - slope * mx)
}
