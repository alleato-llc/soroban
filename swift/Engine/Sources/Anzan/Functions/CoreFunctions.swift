import BigInt
import Foundation

/// Shared numeric helpers used by both the operator table and the registry.
enum Functions {
    /// `^` and `pow()`: exact for integer exponents, Double-domain otherwise.
    static func pow(_ base: BigDecimal, _ exponent: BigDecimal) throws -> BigDecimal {
        if let n = exponent.intValue {
            return try base.power(n)
        }
        guard !base.isNegative else {
            throw EngineError.domainError(message: "negative base with non-integer exponent")
        }
        let result = Foundation.pow(base.doubleValue, exponent.doubleValue)
        guard let converted = BigDecimal(result) else {
            throw EngineError.domainError(message: "pow result out of range")
        }
        return converted
    }

    static func factorial(_ value: BigDecimal) throws -> BigDecimal {
        guard let n = value.intValue, n >= 0 else {
            throw EngineError.domainError(message: "fact() needs a non-negative integer")
        }
        guard n <= 10_000 else {
            throw EngineError.domainError(message: "fact(\(n)) is too large")
        }
        var result = Integer(1)
        for i in 2...Swift.max(n, 2) where n >= 2 {
            result *= Integer(i)
        }
        return BigDecimal(significand: result, exponent: 0)
    }

    /// n choose k, exactly — every intermediate division in the
    /// multiplicative formula is itself exact, so the BigInt never carries
    /// a remainder. k > n is 0 ways (the combinatorial convention).
    static func combinations(_ n: Int, _ k: Int) throws -> BigDecimal {
        guard n >= 0, k >= 0 else {
            throw EngineError.domainError(message: "choose() needs non-negative integers")
        }
        guard k <= n else { return .zero }
        let smaller = Swift.min(k, n - k)
        guard smaller <= 10_000 else {
            throw EngineError.domainError(message: "choose(\(n), \(k)) is too large")
        }
        var result = Integer(1)
        for i in 0..<smaller {
            result = result * Integer(n - i) / Integer(i + 1)
        }
        return BigDecimal(significand: result, exponent: 0)
    }

    /// Ordered selections of k from n — the falling factorial, exact.
    static func permutations(_ n: Int, _ k: Int) throws -> BigDecimal {
        guard n >= 0, k >= 0 else {
            throw EngineError.domainError(message: "perm() needs non-negative integers")
        }
        guard k <= n else { return .zero }
        guard k <= 10_000 else {
            throw EngineError.domainError(message: "perm(\(n), \(k)) is too large")
        }
        var result = Integer(1)
        for i in (n - k + 1)...Swift.max(n, 1) where k > 0 {
            result *= Integer(i)
        }
        return BigDecimal(significand: result, exponent: 0)
    }
}

extension BigDecimal {
    /// The exact Int value, when the number is an integer that fits.
    package var intValue: Int? {
        guard isInteger else { return nil }
        guard exponent <= 18, let value = Int(formatted(scientificThreshold: Int.max)) else {
            return nil
        }
        return value
    }
}

// MARK: - Registry entries

private func fn(_ name: String, _ arity: ClosedRange<Int>,
                category: FunctionCategory = .core,
                _ signature: String, _ summary: String, _ examples: [String],
                _ apply: @escaping @Sendable ([BigDecimal]) throws -> BigDecimal) -> BuiltinFunction {
    BuiltinFunction(name: name, category: category, signature: signature,
                    summary: summary, examples: examples, arity: arity,
                    apply: apply)
}

/// One-argument Double-domain function with a domain check.
private func doubleFn(_ name: String, category: FunctionCategory = .core,
                      _ signature: String, _ summary: String, _ examples: [String],
                      _ f: @escaping @Sendable (Double) -> Double) -> BuiltinFunction {
    fn(name, 1...1, category: category, signature, summary, examples) { args throws(EngineError) in
        try BigDecimal.viaDouble(name, args[0], f)
    }
}

let coreFunctions: [BuiltinFunction] = [
    fn("abs", 1...1, "abs(x)",
       "Absolute value of x.",
       ["abs(-5)", "abs(3.2)"]) { args in
        args[0].isNegative ? -args[0] : args[0]
    },
    fn("min", 1...Int.max, "min(x, y, …)",
       "Smallest of the arguments. Accepts cell ranges.",
       ["min(3, 1, 2)", "min(B:1..B:3)"]) { args in args.min()! },
    fn("max", 1...Int.max, "max(x, y, …)",
       "Largest of the arguments. Accepts cell ranges.",
       ["max(3, 1, 2)", "max(B:1..B:3)"]) { args in args.max()! },
    fn("round", 1...2, "round(x, places = 0)",
       "Rounds x to a number of decimal places (banker's rounding: halves go to the even neighbor). Negative places round left of the decimal point.",
       ["round(2.567, 2)", "round(2.5)", "round(1234, -2)"]) { args in
        let places = try args.count == 2 ? requireInt(args[1], "round places") : 0
        return args[0].rounded(toPlaces: places)
    },
    fn("floor", 1...1, "floor(x)",
       "Largest integer ≤ x.",
       ["floor(2.9)", "floor(-1.5)"]) { args in args[0].rounded(.down) },
    fn("ceil", 1...1, "ceil(x)",
       "Smallest integer ≥ x.",
       ["ceil(2.1)", "ceil(-1.5)"]) { args in args[0].rounded(.up) },
    fn("trunc", 1...1, "trunc(x)",
       "Drops the fractional part (rounds toward zero).",
       ["trunc(2.9)", "trunc(-1.5)"]) { args in args[0].rounded(.towardZero) },
    fn("sqrt", 1...1, "sqrt(x)",
       "Square root, exact to 50 significant digits. √x is the same thing.",
       ["sqrt(16)", "√2"]) { args in try args[0].squareRoot() },
    fn("cbrt", 1...1, "cbrt(x)",
       "Cube root. Works for negative numbers.",
       ["cbrt(27)", "cbrt(-8)"]) { args in try nthRoot(args[0], 3) },
    fn("root", 2...2, "root(x, n)",
       "nth root of x. Odd roots of negatives are real.",
       ["root(32, 5)", "root(-27, 3)"]) { args in
        try nthRoot(args[0], requireInt(args[1], "root degree"))
    },
    fn("pow", 2...2, "pow(x, y)",
       "x raised to y. Exact for integer exponents. Equivalent to the ^ operator — except in Programmer mode, where ^ is XOR, so pow is how you write a power there.",
       ["pow(2, 10)", "pow(4, 0.5)"]) { args in try Functions.pow(args[0], args[1]) },
    fn("mod", 2...2, "mod(x, y)",
       "Remainder of x ÷ y, with the sign of x (exact). In the default dialect modulo is this function and the postfix % means percent (3% → 0.03); in Programmer mode the `%` operator is modulo (a % b). See man modes.",
       ["mod(10, 3)", "mod(-7, 3)"]) { args in try args[0] % args[1] },
    fn("fact", 1...1, "fact(n)",
       "Factorial of a non-negative integer, computed exactly.",
       ["fact(5)", "fact(20)"]) { args in try Functions.factorial(args[0]) },
    fn("choose", 2...2, "choose(n, k)",
       "Binomial coefficient — n choose k, computed exactly (choose(100, 50) keeps all 30 digits). k > n is 0.",
       ["choose(5, 2)", "choose(52, 5)"]) { args in
        try Functions.combinations(requireInt(args[0], "choose n"),
                                   requireInt(args[1], "choose k"))
    },
    fn("perm", 2...2, "perm(n, k)",
       "Permutations — ordered selections of k from n, computed exactly. k > n is 0.",
       ["perm(5, 2)", "perm(10, 10)"]) { args in
        try Functions.permutations(requireInt(args[0], "perm n"),
                                   requireInt(args[1], "perm k"))
    },
    fn("gcd", 2...Int.max, "gcd(a, b, …)",
       "Greatest common divisor of integers.",
       ["gcd(12, 18)", "gcd(12, 18, 24)"]) { args in
        let ints = try args.map { try requireInt($0, "gcd") }
        return BigDecimal(ints.reduce(0) { gcd(abs($0), abs($1)) })
    },
    fn("lcm", 2...Int.max, "lcm(a, b, …)",
       "Least common multiple of integers.",
       ["lcm(4, 6)", "lcm(2, 3, 5)"]) { args in
        let ints = try args.map { try requireInt($0, "lcm") }
        return BigDecimal(try ints.reduce(1) { a, b in
            guard a != 0 || b != 0 else { return 0 }
            let g = gcd(abs(a), abs(b))
            guard g != 0 else { return 0 }
            let (result, overflow) = (a / g).multipliedReportingOverflow(by: b)
            guard !overflow else { throw EngineError.domainError(message: "lcm overflow") }
            return abs(result)
        })
    },
    fn("percent", 1...1, "percent(x)",
       "x divided by 100 — handy for rates: tax = percent(8.25).",
       ["percent(8.25)", "200 * percent(15)"]) { args in
        try args[0] / BigDecimal(100)
    },
    fn("not", 1...1, category: .logic, "not(x)",
       "Logical negation: 1 when x is 0, otherwise 0.",
       ["not(0)", "not(5)"]) { args in args[0].isZero ? .one : .zero },
    fn("and", 2...Int.max, category: .logic, "and(a, b, …)",
       "1 when every argument is nonzero. Use for combined conditions — comparisons can't chain.",
       ["and(1 < 2, 2 < 3)", "and(1, 0)"]) { args in
        args.allSatisfy { !$0.isZero } ? .one : .zero
    },
    fn("or", 2...Int.max, category: .logic, "or(a, b, …)",
       "1 when any argument is nonzero.",
       ["or(0, 0, 4)", "or(1 > 2, 2 > 1)"]) { args in
        args.contains { !$0.isZero } ? .one : .zero
    },
    doubleFn("exp", "exp(x)",
             "e raised to x (≈15 significant digits).",
             ["exp(0)", "exp(1)"], Foundation.exp),
    fn("ln", 1...1, "ln(x)",
       "Natural logarithm (base e, ≈15 significant digits).",
       ["ln(e)", "ln(10)"]) { args in try logarithm("ln", args[0], Foundation.log) },
    fn("log10", 1...1, "log10(x)",
       "Base-10 logarithm.",
       ["log10(1000)", "log10(2)"]) { args in try logarithm("log10", args[0], Foundation.log10) },
    fn("log", 2...2, "log(base, x)",
       "Logarithm of x in an arbitrary base.",
       ["log(2, 8)", "log(5, 125)"]) { args in
        // The ratio is taken in the Double domain so clean cases like
        // log(2, 8) come out exact instead of 2.999…97.
        for arg in args where arg.isNegative || arg.isZero {
            throw EngineError.domainError(message: "log needs positive arguments")
        }
        let result = Foundation.log(args[1].doubleValue) / Foundation.log(args[0].doubleValue)
        guard let converted = BigDecimal(result) else {
            throw EngineError.domainError(message: "log is undefined for these arguments")
        }
        return converted
    },

    // Goal seek as a formula: find x with f(x) = target. Newton with a
    // numeric derivative from the guess, expanding-bracket bisection as the
    // fallback — the same regime as rate()/irr(), and like them it works in
    // the Double domain (~15 significant digits).
    BuiltinFunction(
        name: "solve",
        category: .core,
        signature: "solve(f, target = 0, guess = 1)",
        summary: "Finds x where f(x) = target, numerically (Newton + bisection, ~15 significant digits). Pass a lambda or function name: solve(x -> x^2, 2) is √2; solve(r -> npv(r, …), 0, 0.1) is goal seek.",
        examples: ["solve(x -> x^2, 2)", "solve(cos, 0, 1)"],
        arity: 1...3,
        applyHigherOrder: { arguments, apply in
            let target = arguments.count > 1
                ? try arguments[1].asNumber(for: "solve's target").doubleValue : 0
            let guess = arguments.count > 2
                ? try arguments[2].asNumber(for: "solve's guess").doubleValue : 1

            func g(_ x: Double) throws -> Double {
                guard let input = BigDecimal(x) else {
                    throw EngineError.domainError(message: "solve() left the number line")
                }
                return try apply(arguments[0], [.number(input)])
                    .asNumber(for: "f's result in solve()").doubleValue - target
            }

            let tolerance = 1e-12

            // Newton from the guess.
            var x = guess
            for _ in 0..<60 {
                let value = try g(x)
                if abs(value) < tolerance, let result = BigDecimal(x) {
                    return .number(result)
                }
                let h = Swift.max(abs(x), 1e-4) * 1e-7
                let slope = try (g(x + h) - g(x - h)) / (2 * h)
                guard slope.isFinite, slope != 0 else { break }
                let next = x - value / slope
                guard next.isFinite else { break }
                x = next
            }

            // Bisection over an expanding bracket around the guess.
            var radius = Swift.max(abs(guess), 1.0)
            while radius <= 1e9 {
                let (lo, hi) = (guess - radius, guess + radius)
                let (fLo, fHi) = (try g(lo), try g(hi))
                if fLo.isFinite, fHi.isFinite, fLo.sign != fHi.sign {
                    var (a, b, fA) = (lo, hi, fLo)
                    for _ in 0..<200 {
                        let mid = (a + b) / 2
                        let fMid = try g(mid)
                        if abs(fMid) < tolerance || (b - a) / 2 < 1e-15 {
                            guard let result = BigDecimal(mid) else { break }
                            return .number(result)
                        }
                        if fMid.sign == fA.sign { (a, fA) = (mid, fMid) } else { b = mid }
                    }
                    break
                }
                radius *= 4
            }
            throw EngineError.domainError(
                message: "solve() did not converge — try a different guess")
        }),
    // The spreadsheet's #REF! adapted: deleting a referenced row/column
    // splices this call over the dead reference, so the formula errors
    // loudly instead of silently reading shifted neighbors. Registry slot
    // justified as arrival vocabulary — the name appears in rewritten
    // formulas, so man(refError) must answer for it.
    fn("refError", 0...0, "refError()",
       "Always errors: marks a reference whose row or column was deleted. Replace it with the cell you meant.",
       ["if(true, 42, refError())"]) { _ in
        throw EngineError.domainError(message: "refers to a deleted cell")
    },
]

let trigFunctions: [BuiltinFunction] = [
    doubleFn("sin", category: .trig, "sin(x)", "Sine of x (radians).",
             ["sin(0)", "sin(pi / 2)"], Foundation.sin),
    doubleFn("cos", category: .trig, "cos(x)", "Cosine of x (radians).",
             ["cos(0)", "cos(pi)"], Foundation.cos),
    doubleFn("tan", category: .trig, "tan(x)", "Tangent of x (radians).",
             ["tan(0)", "tan(pi / 4)"], Foundation.tan),
    doubleFn("asin", category: .trig, "asin(x)", "Inverse sine, in radians. x must be in [-1, 1].",
             ["asin(1)", "asin(0.5)"], Foundation.asin),
    doubleFn("acos", category: .trig, "acos(x)", "Inverse cosine, in radians. x must be in [-1, 1].",
             ["acos(1)", "acos(0)"], Foundation.acos),
    doubleFn("atan", category: .trig, "atan(x)", "Inverse tangent, in radians.",
             ["atan(1)", "atan(0)"], Foundation.atan),
    fn("atan2", 2...2, category: .trig, "atan2(y, x)",
       "Angle of the point (x, y), in radians — the quadrant-aware inverse tangent.",
       ["atan2(1, 1)", "atan2(1, 0)"]) { args in
        guard let result = BigDecimal(Foundation.atan2(args[0].doubleValue,
                                                       args[1].doubleValue)) else {
            throw EngineError.domainError(message: "atan2 is undefined for these arguments")
        }
        return result
    },
    doubleFn("sinh", category: .trig, "sinh(x)", "Hyperbolic sine.",
             ["sinh(0)", "sinh(1)"], Foundation.sinh),
    doubleFn("cosh", category: .trig, "cosh(x)", "Hyperbolic cosine.",
             ["cosh(0)", "cosh(1)"], Foundation.cosh),
    doubleFn("tanh", category: .trig, "tanh(x)", "Hyperbolic tangent.",
             ["tanh(0)", "tanh(1)"], Foundation.tanh),
    doubleFn("asinh", category: .trig, "asinh(x)", "Inverse hyperbolic sine.",
             ["asinh(0)", "asinh(1)"], Foundation.asinh),
    doubleFn("acosh", category: .trig, "acosh(x)", "Inverse hyperbolic cosine. x must be ≥ 1.",
             ["acosh(1)", "acosh(2)"], Foundation.acosh),
    doubleFn("atanh", category: .trig, "atanh(x)", "Inverse hyperbolic tangent. |x| must be < 1.",
             ["atanh(0)", "atanh(0.5)"], Foundation.atanh),
    // Pure BigDecimal — π is the 60-digit constant, so deg(pi) is exactly
    // 180 and rad survives round-trips at full working precision.
    fn("deg", 1...1, category: .trig, "deg(x)",
       "Radians → degrees, at full precision (deg(pi) is exactly 180).",
       ["deg(pi)", "deg(pi / 4)"]) { args in
        try args[0] * BigDecimal(180) / Constants.pi
    },
    fn("rad", 1...1, category: .trig, "rad(x)",
       "Degrees → radians, at full precision.",
       ["rad(180)", "sin(rad(90))"]) { args in
        try args[0] * Constants.pi / BigDecimal(180)
    },
]

// MARK: - Helpers

func requireInt(_ value: BigDecimal, _ what: String) throws -> Int {
    guard let n = value.intValue else {
        throw EngineError.domainError(message: "\(what) must be an integer")
    }
    return n
}

/// Internal, not private: geomean (StatsFunctions) shares it.
func nthRoot(_ value: BigDecimal, _ degree: Int) throws -> BigDecimal {
    guard degree > 0 else {
        throw EngineError.domainError(message: "root degree must be positive")
    }
    if degree == 1 { return value }
    if degree == 2 { return try value.squareRoot() }
    if value.isNegative {
        // Odd roots of negatives are real.
        guard degree % 2 == 1 else {
            throw EngineError.domainError(message: "even root of a negative number")
        }
        return try -nthRoot(-value, degree)
    }
    return try BigDecimal.viaDouble("root", value) { Foundation.pow($0, 1.0 / Double(degree)) }
}

private func logarithm(_ name: String, _ value: BigDecimal,
                       _ f: (Double) -> Double) throws -> BigDecimal {
    guard !value.isNegative, !value.isZero else {
        throw EngineError.domainError(message: "\(name) needs a positive argument")
    }
    return try BigDecimal.viaDouble(name, value, f)
}

private func gcd(_ a: Int, _ b: Int) -> Int {
    var (a, b) = (a, b)
    while b != 0 { (a, b) = (b, a % b) }
    return a
}

// MARK: - Directional rounding

extension BigDecimal {
    enum Direction { case down, up, towardZero }

    /// floor/ceil/trunc to an integer.
    func rounded(_ direction: Direction) -> BigDecimal {
        if isInteger { return self }
        let scale = Integer(10).power(-exponent)
        let (q, r) = significand.quotientAndRemainder(dividingBy: scale)
        var result = q
        switch direction {
        case .towardZero:
            break
        case .down:
            if r.sign == .minus { result -= 1 }
        case .up:
            if r.sign == .plus { result += 1 }
        }
        return BigDecimal(significand: result, exponent: 0)
    }
}
