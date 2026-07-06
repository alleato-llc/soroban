# Benchmark results

Soroban's two calculator engines — the Rust `anzan` crate and the Swift `SorobanEngine`
library — evaluating the SAME five Anzan workloads under one protocol. The op is one
`Calculator.evaluate(line)`; the number is peak **evaluations/sec** (op throughput), so
higher is better. results.json also records each run's median rate (mbps_median) as a
stability signal.

Both engines implement the same language against one shared spec, so this is an honest,
apples-to-apples comparison of the two implementations on identical inputs. It is a
**same-box, same-run** measurement: the Rust-vs-Swift RATIO is the deliverable and holds
on any machine, but the absolute evals/sec are hardware/toolchain-specific (and noisy on
shared CI) — read them as a trend, not a fixed number.

Workloads: `arith` exercises BigDecimal division at working precision; `fib` the
interpreter's dispatch + user-function recursion (`fib(20)`); `reduce` the indexed ∑
loop (`sigma_i=1^1000(i^2)`); `transcendental` the f64 libm/Double seam
(`sin+cos+tan+atan2`); `finance` the exact `pmt` (power + div).

Machine: Apple M4 Max | Darwin arm64 | 2026-07-06 | commit 571bbf6.

| Implementation | Arithmetic | Fibonacci | Reduction (∑) | Transcendental | Finance (pmt) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust engine | 464827.3 | 80.1 | 4215.6 | 268100.5 | 1803.4 |
| Swift engine | 90420.1 | 136.8 | 6354.2 | 57198.2 | 738.4 |
