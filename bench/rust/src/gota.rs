//! Gota harness (Rust). Copy this file into your project as-is and do not edit it.
//!
//! It owns the measurement: argument parsing, the buffer, the peak-of-batches timing
//! loop, and the JSON output. Your code plugs in through `run(impl, register)`: the
//! harness hands your closure a `&Bencher` and the buffer, and you call
//! `b.bench(name, op)` for each operation. See ../../PROTOCOL.md.

use std::time::{Duration, Instant};

pub struct Bencher {
    impl_name: String,
    buf_bytes: usize,
    warmup: Duration,
    measure: Duration,
}

impl Bencher {
    // Report peak throughput across many batches. The clock is read only at batch
    // boundaries, and the maximum MB/s over the batches is the reproducible rate:
    // jitter, frequency scaling, and contention only ever make a batch slower, so the
    // fastest batch reflects the code running unimpeded. Batch grows until >= 100ms.
    pub fn bench(&self, name: &str, mut op: impl FnMut()) {
        let start = Instant::now();
        while start.elapsed() < self.warmup {
            op();
        }
        let mut batch: u64 = 1;
        loop {
            let s = Instant::now();
            for _ in 0..batch {
                op();
            }
            if s.elapsed() >= Duration::from_millis(100) {
                break;
            }
            batch = batch.saturating_mul(2);
        }
        let mut best = 0.0f64;
        let mut total: u64 = 0;
        let mut samples: Vec<f64> = Vec::new(); // per-batch MB/s; median vs peak shows stability
        let start = Instant::now();
        while start.elapsed() < self.measure {
            let s = Instant::now();
            for _ in 0..batch {
                op();
            }
            let mbps = (self.buf_bytes as f64) * (batch as f64) / 1e6 / s.elapsed().as_secs_f64();
            if mbps > best {
                best = mbps;
            }
            samples.push(mbps);
            total += batch;
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = samples.len();
        let median = if n == 0 {
            0.0
        } else if n % 2 == 1 {
            samples[n / 2]
        } else {
            (samples[n / 2 - 1] + samples[n / 2]) / 2.0
        };
        println!(
            "{{\"impl\":\"{}\",\"bench\":\"{}\",\"mbps\":{:.2},\"mbps_median\":{:.2},\"iters\":{}}}",
            self.impl_name, name, best, median, total
        );
    }
}

pub fn run(impl_name: &str, register: impl FnOnce(&Bencher, &mut Vec<u8>)) {
    let args: Vec<String> = std::env::args().collect();
    let buf_bytes: usize = args.get(1).map_or(1_048_576, |s| s.parse().unwrap());
    let warmup = Duration::from_secs_f64(args.get(2).map_or(0.5, |s| s.parse().unwrap()));
    let measure = Duration::from_secs_f64(args.get(3).map_or(2.0, |s| s.parse().unwrap()));
    let b = Bencher {
        impl_name: impl_name.to_string(),
        buf_bytes,
        warmup,
        measure,
    };
    let mut data = vec![0u8; buf_bytes];
    register(&b, &mut data);
}
