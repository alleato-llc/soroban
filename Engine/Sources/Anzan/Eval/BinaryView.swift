import BigInt

/// A read/edit view of an integer `Value`'s bits — the model behind the app's
/// binary bit-editor overlay (macOS-Calculator-style). Pure and host-free: the
/// width policy and two's-complement encoding live here so the UI stays thin
/// and the logic is tested.
///
/// A `fixedInt` edits at its own declared width and signedness (full
/// two's-complement). A plain non-negative integer edits as an UNSIGNED register
/// at a chosen width (signed bit-editing is the job of the typed `Int…` values).
/// Widths are capped at 128 bits; wider values, negatives, and non-integers are
/// not editable and carry a reason the host can explain.
public struct BinaryView: Sendable, Equatable {
    /// Display widths a plain integer may use (the bit grid). Capped at `maxWidth`.
    public static let editableWidths = [8, 16, 32, 64, 128, 256]
    public static let maxWidth = 256

    /// Why a value can't be bit-edited (the host shows the matching hint).
    public enum Unavailable: Error, Sendable, Equatable {
        case notAnInteger   // a decimal/string/array/… — no bits to edit
        case negative       // a plain negative number — wrap it in a signed Int type
        case tooWide        // needs more than 128 bits (a huge integer, or Int256/UInt256)
    }

    public enum Kind: Sendable, Equatable {
        case plain                 // a bare Number, edited as an unsigned register
        case fixed(signed: Bool)   // an Int…/UInt… value, edited in two's-complement
    }

    public let kind: Kind
    public let width: Int
    /// The unsigned bit pattern, always in `[0, 2^width)`.
    public let pattern: BigInt

    /// True for a signed fixed-width value (the high bit is the sign).
    public var signed: Bool { if case .fixed(let s) = kind { return s }; return false }

    /// The narrowest editable width that can hold the current value — the host
    /// grays out smaller picker options (they can't represent it). A fixed-width
    /// value is locked to its own width (its picker is hidden anyway).
    public var minimumWidth: Int {
        switch kind {
        case .fixed:
            return width
        case .plain:
            let needed = pattern.isZero ? 1 : String(pattern, radix: 2).count
            return BinaryView.editableWidths.first { $0 >= needed } ?? BinaryView.maxWidth
        }
    }

    /// The bits MSB→LSB, length `width` — index 0 of the array is the high bit.
    public var bits: [Bool] {
        (0..<width).reversed().map { (pattern & (BigInt(1) << $0)) != 0 }
    }

    /// The current value, reconstructed in its original kind (a fixed-width int
    /// keeps its type and signedness; a plain register is a Number).
    public var value: Value {
        switch kind {
        case .plain:
            return .number(BigDecimal(significand: pattern, exponent: 0))
        case .fixed(let signed):
            let decoded = signed && pattern >= (BigInt(1) << (width - 1))
                ? pattern - (BigInt(1) << width)
                : pattern
            // In range by construction — `width` is an allowed width and `decoded`
            // sits within it, so the validating initializer cannot throw.
            return .fixedInt(try! FixedInt(value: decoded, bits: width, signed: signed))
        }
    }

    /// A new view with bit `index` (0 = LSB) flipped; same kind and width.
    public func flippingBit(_ index: Int) -> BinaryView {
        precondition(index >= 0 && index < width, "bit index out of range")
        return BinaryView(kind: kind, width: width, pattern: pattern ^ (BigInt(1) << index))
    }

    /// Build a view for `value`, displaying a plain integer at least
    /// `preferredWidth` wide (auto-bumped to fit, ignored for a fixed-width int).
    public static func make(for value: Value, preferredWidth: Int = 32)
        -> Result<BinaryView, Unavailable> {
        switch value {
        case .fixedInt(let f):
            guard f.bits <= maxWidth else { return .failure(.tooWide) }  // Int256/UInt256
            let pat = f.value < 0 ? f.value + (BigInt(1) << f.bits) : f.value
            return .success(BinaryView(kind: .fixed(signed: f.signed), width: f.bits, pattern: pat))

        case .number(let n):
            guard n.isInteger else { return .failure(.notAnInteger) }
            let magnitude = n.significand * BigInt(10).power(n.exponent)  // exponent ≥ 0
            guard magnitude >= 0 else { return .failure(.negative) }
            let needed = magnitude.isZero ? 1 : String(magnitude, radix: 2).count
            guard needed <= maxWidth else { return .failure(.tooWide) }
            let floor = min(max(preferredWidth, 1), maxWidth)
            let width = editableWidths.first { $0 >= needed && $0 >= floor }
                ?? editableWidths.first { $0 >= needed }
                ?? maxWidth
            return .success(BinaryView(kind: .plain, width: width, pattern: magnitude))

        default:
            return .failure(.notAnInteger)
        }
    }
}
