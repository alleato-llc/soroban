// Gota harness (Swift). Copy this file into your project as-is and do not edit it.
//
// It owns the measurement: argument parsing, the buffer, the peak-of-batches timing
// loop, and the JSON output. Your code plugs in through Gota.run(impl, register): the
// harness hands your closure a Gota bencher and the buffer, and you call
// b.bench(name, op) for each operation. See ../../PROTOCOL.md.

import Foundation

public final class Gota {

    private let impl: String
    private let bufBytes: Int
    private let warmup: Double
    private let measure: Double

    private init(impl: String, bufBytes: Int, warmup: Double, measure: Double) {
        self.impl = impl
        self.bufBytes = bufBytes
        self.warmup = warmup
        self.measure = measure
    }

    // Report peak throughput across many batches (max MB/s is the reproducible rate;
    // jitter only ever slows a batch). The clock is read only at batch boundaries.
    public func bench(_ name: String, _ op: () -> Void) {
        var start = DispatchTime.now().uptimeNanoseconds
        while Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9 < warmup {
            op()
        }
        var batch = 1
        while true {
            start = DispatchTime.now().uptimeNanoseconds
            for _ in 0..<batch { op() }
            if Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9 >= 0.1 { break }
            batch *= 2
        }
        var best = 0.0
        var total = 0
        var samples: [Double] = []  // per-batch MB/s; median vs peak shows stability
        let t0 = DispatchTime.now().uptimeNanoseconds
        while Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e9 < measure {
            start = DispatchTime.now().uptimeNanoseconds
            for _ in 0..<batch { op() }
            let secs = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9
            let mbps = Double(bufBytes) * Double(batch) / 1e6 / secs
            if mbps > best { best = mbps }
            samples.append(mbps)
            total += batch
        }
        samples.sort()
        let n = samples.count
        let median = n == 0 ? 0.0
            : (n % 2 == 1 ? samples[n / 2] : (samples[n / 2 - 1] + samples[n / 2]) / 2)
        let f = { (x: Double) in String(format: "%.2f", x) }
        print("{\"impl\":\"\(impl)\",\"bench\":\"\(name)\",\"mbps\":\(f(best)),\"mbps_median\":\(f(median)),\"iters\":\(total)}")
    }

    public static func run(_ impl: String, _ register: (Gota, inout [UInt8]) -> Void) {
        let args = CommandLine.arguments
        let bufBytes = args.count > 1 ? (Int(args[1]) ?? 1_048_576) : 1_048_576
        let warmup = args.count > 2 ? (Double(args[2]) ?? 0.5) : 0.5
        let measure = args.count > 3 ? (Double(args[3]) ?? 2.0) : 2.0
        let b = Gota(impl: impl, bufBytes: bufBytes, warmup: warmup, measure: measure)
        var data = [UInt8](repeating: 0, count: bufBytes)
        register(b, &data)
    }
}
