//! Cross-engine benchmark runner (Rust engine). YOUR code — the op is one full
//! `Calculator::evaluate` of an Anzan line, the fair symmetric operation both engines
//! expose. The Gota harness (src/gota.rs, copy-as-is) owns timing; we register one op
//! per workload. The metric is OP_THROUGHPUT (evaluations/sec, peak of batches): a
//! single evaluate() is faster than the clock, so the harness batches it, which
//! auto-calibrates across the ~1000× spread between `arith` and `fib` without a shared
//! buffer to fill. See ../README.md and ../../gota PROTOCOL.
//!
//!     cargo build --release && ./target/release/runner [buffer_bytes] [warmup_s] [measure_s]

mod gota;

use anzan::Calculator;

const IMPL: &str = "rust-engine";

/// Evaluate `line` once and panic if it errors — a loud guard (run untimed, before
/// each bench) so a mistyped workload fails here instead of silently benchmarking the
/// error path. Returns nothing; the timed op re-runs the same line.
fn guard(calc: &mut Calculator, line: &str) {
    if let Err(e) = calc.evaluate(line) {
        panic!("benchmark workload {line:?} did not evaluate: {e:?}");
    }
}

fn main() {
    gota::run(IMPL, |b, _data| {
        // arith — BigDecimal division at working precision (the exact-arithmetic core).
        {
            let mut calc = Calculator::new();
            let line = "(123456789.98 / 7.13 + 98765.4) * 2.5 - 1000000 / 3";
            guard(&mut calc, line);
            b.bench("arith", || {
                let _ = std::hint::black_box(calc.evaluate(std::hint::black_box(line)));
            });
        }
        // fib — interpreter dispatch + user-function recursion (definition is untimed).
        {
            let mut calc = Calculator::new();
            guard(&mut calc, "fib(n) = if(n <= 2, 1, fib(n - 1) + fib(n - 2))");
            let line = "fib(20)";
            guard(&mut calc, line);
            b.bench("fib", || {
                let _ = std::hint::black_box(calc.evaluate(std::hint::black_box(line)));
            });
        }
        // reduce — the indexed ∑ form: a tight reduce loop over a bigint accumulator.
        {
            let mut calc = Calculator::new();
            let line = "sigma_i=1^1000(i^2)";
            guard(&mut calc, line);
            b.bench("reduce", || {
                let _ = std::hint::black_box(calc.evaluate(std::hint::black_box(line)));
            });
        }
        // transcendental — the f64 `via_double` seam (trig routed through libm).
        {
            let mut calc = Calculator::new();
            let line = "sin(1) + cos(2) + tan(0.5) + atan2(1, 1)";
            guard(&mut calc, line);
            b.bench("transcendental", || {
                let _ = std::hint::black_box(calc.evaluate(std::hint::black_box(line)));
            });
        }
        // finance — exact `pmt` (a `power` + `div` composite).
        {
            let mut calc = Calculator::new();
            let line = "pmt(0.05 / 12, 360, 200000)";
            guard(&mut calc, line);
            b.bench("finance", || {
                let _ = std::hint::black_box(calc.evaluate(std::hint::black_box(line)));
            });
        }
    });
}
