/// Signed arithmetic for `Integer`: small-word fast paths (no allocation) that fall
/// back to `Magnitude` limb arithmetic on overflow or when either side is `.big`.

extension Integer {
    static prefix func - (value: Integer) -> Integer {
        switch value {
        case .small(let v):
            if v != Int.min { return .small(-v) }
            return .pack(negative: false, [0x8000_0000_0000_0000]) // -(Int.min) = +2⁶³
        case .big(let negative, let magnitude):
            return .big(negative: !negative, magnitude: magnitude)
        }
    }

    static func + (lhs: Integer, rhs: Integer) -> Integer {
        // Fast path: both fit in Int and their sum doesn't overflow.
        if case .small(let a) = lhs, case .small(let b) = rhs {
            let (sum, overflow) = a.addingReportingOverflow(b)
            if !overflow { return .small(sum) }
        }
        return addSigned(lhs, rhs)
    }

    static func - (lhs: Integer, rhs: Integer) -> Integer {
        if case .small(let a) = lhs, case .small(let b) = rhs {
            let (diff, overflow) = a.subtractingReportingOverflow(b)
            if !overflow { return .small(diff) }
        }
        return addSigned(lhs, -rhs)
    }

    static func * (lhs: Integer, rhs: Integer) -> Integer {
        if case .small(let a) = lhs, case .small(let b) = rhs {
            let (product, overflow) = a.multipliedReportingOverflow(by: b)
            if !overflow { return .small(product) }
        }
        let magnitude = Magnitude.multiply(lhs.magnitudeLimbs, rhs.magnitudeLimbs)
        return .pack(negative: lhs.isNegative != rhs.isNegative, magnitude)
    }

    static func << (lhs: Integer, bits: Int) -> Integer {
        if bits <= 0 { return lhs }
        return .pack(negative: lhs.isNegative, Magnitude.shiftLeft(lhs.magnitudeLimbs, bits))
    }

    static func += (lhs: inout Integer, rhs: Integer) { lhs = lhs + rhs }
    static func -= (lhs: inout Integer, rhs: Integer) { lhs = lhs - rhs }
    static func *= (lhs: inout Integer, rhs: Integer) { lhs = lhs * rhs }

    /// Signed add via magnitudes: same sign → add, differing → subtract the smaller
    /// magnitude from the larger and take the larger's sign.
    private static func addSigned(_ lhs: Integer, _ rhs: Integer) -> Integer {
        if lhs.isZero { return rhs }
        if rhs.isZero { return lhs }
        let a = lhs.magnitudeLimbs
        let b = rhs.magnitudeLimbs
        if lhs.isNegative == rhs.isNegative {
            return .pack(negative: lhs.isNegative, Magnitude.add(a, b))
        }
        switch Magnitude.compare(a, b) {
        case 0:
            return .small(0)
        case 1:
            return .pack(negative: lhs.isNegative, Magnitude.subtract(a, b))
        default:
            return .pack(negative: rhs.isNegative, Magnitude.subtract(b, a))
        }
    }
}
