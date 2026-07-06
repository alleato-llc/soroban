// Cross-engine benchmark runner (Swift engine). YOUR code — the op is one full
// `Calculator.evaluate` of an Anzan line, the fair symmetric operation both engines
// expose (Swift's parser is `package`-scoped, so parse+eval via `evaluate` is the only
// externally reachable seam — and the honest one to compare). The Gota harness
// (Gota.swift, copy-as-is) owns timing; we register one op per workload. The metric is
// OP_THROUGHPUT (evaluations/sec, peak of batches): a single evaluate() is faster than
// the clock, so the harness batches it, auto-calibrating across the ~1000× spread
// between `arith` and `fib` without a shared buffer to fill. See ../../README.md.
//
//   swift build -c release && .build/release/runner [buffer_bytes] [warmup_s] [measure_s]

import SorobanEngine

/// Keep a computed value alive across an opaque call so `-O` can't delete the work we
/// are timing (Swift's answer to Rust's `std::hint::black_box`).
@inline(never)
func blackHole<T>(_ value: T) {
    withExtendedLifetime(value) {}
}

@main
struct Runner {
    static let impl = "swift-engine"

    /// Evaluate `line` once and trap if it errors — a loud guard (run untimed, before
    /// each bench) so a mistyped workload fails here instead of silently benchmarking
    /// the error path.
    static func guardEval(_ calc: Calculator, _ line: String) {
        if case .failure(let error) = calc.evaluate(line) {
            fatalError("benchmark workload \(line) did not evaluate: \(error)")
        }
    }

    static func main() {
        Gota.run(impl) { b, _ in
            // arith — BigDecimal division at working precision (the exact-arithmetic core).
            do {
                let calc = Calculator()
                let line = "(123456789.98 / 7.13 + 98765.4) * 2.5 - 1000000 / 3"
                guardEval(calc, line)
                b.bench("arith") { blackHole(calc.evaluate(line)) }
            }
            // fib — interpreter dispatch + user-function recursion (definition untimed).
            do {
                let calc = Calculator()
                guardEval(calc, "fib(n) = if(n <= 2, 1, fib(n - 1) + fib(n - 2))")
                let line = "fib(20)"
                guardEval(calc, line)
                b.bench("fib") { blackHole(calc.evaluate(line)) }
            }
            // reduce — the indexed ∑ form: a tight reduce loop over a bigint accumulator.
            do {
                let calc = Calculator()
                let line = "sigma_i=1^1000(i^2)"
                guardEval(calc, line)
                b.bench("reduce") { blackHole(calc.evaluate(line)) }
            }
            // transcendental — the f64 `viaDouble` seam (trig routed through Double).
            do {
                let calc = Calculator()
                let line = "sin(1) + cos(2) + tan(0.5) + atan2(1, 1)"
                guardEval(calc, line)
                b.bench("transcendental") { blackHole(calc.evaluate(line)) }
            }
            // finance — exact `pmt` (a `power` + `div` composite).
            do {
                let calc = Calculator()
                let line = "pmt(0.05 / 12, 360, 200000)"
                guardEval(calc, line)
                b.bench("finance") { blackHole(calc.evaluate(line)) }
            }
        }
    }
}
