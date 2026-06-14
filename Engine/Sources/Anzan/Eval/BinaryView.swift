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

// MARK: - Bit-field formats (named bit ranges)

extension BinaryView {
    /// A field in a format: a named bit range. Three flavors, mutually exclusive:
    /// a plain NUMERIC field (`flags == nil`, `values == nil`); a FLAGS field with
    /// per-bit names (high→low, count == width) giving each bit a meaning —
    /// `owner` as `["r","w","x"]`; or an ENUM field whose unsigned value indexes a
    /// label list — `mode` as `["idle","run","halt","max"]` (value 1 → "run").
    public struct FieldSpec: Sendable, Equatable {
        public let name: String
        public let width: Int
        public let flags: [String]?
        public let values: [String]?
        public init(name: String, width: Int, flags: [String]? = nil, values: [String]? = nil) {
            self.name = name; self.width = width; self.flags = flags; self.values = values
        }
    }

    /// One named bit range decoded from the value. A format packs fields into the
    /// LOW bits, listed high→low, so they read left-to-right in the grid as
    /// `[unlabeled high bits][f1][f2]…[fN]`.
    public struct Field: Sendable, Equatable {
        public let name: String
        public let width: Int
        public let lowBit: Int       // 0 = LSB
        public let value: BigInt     // the field's unsigned value
        public let flags: [String]?  // per-bit flag names (high→low), nil = not flags
        public let values: [String]? // enum value labels (value indexes them), nil = not enum

        /// The decoded meaning of a flag field: single-char flags read
        /// positionally with `-` for clear bits (`r-x`); multi-char flags list
        /// only the set ones (`ACK SYN`, or `—` when none). nil for non-flags.
        public var flagString: String? {
            guard let flags else { return nil }
            if flags.allSatisfy({ $0.count == 1 }) {
                return flags.enumerated().map { i, name in
                    isSet(bitFromTop: i) ? name : "-"
                }.joined()
            }
            let set = flags.enumerated().compactMap { i, name in
                isSet(bitFromTop: i) ? name : nil
            }
            return set.isEmpty ? "—" : set.joined(separator: " ")
        }

        /// The decoded label of an ENUM field: the value indexes the label list
        /// (`mode` value 2 of `["idle","run","halt","max"]` → "halt"). A value
        /// past the list shows the raw number. nil for non-enum.
        public var enumString: String? {
            guard let values else { return nil }
            guard let index = Int(exactly: value), index >= 0, index < values.count else {
                return String(value)
            }
            return values[index]
        }

        /// The field's human-readable decode — enum label, flag string, or the
        /// plain number — whichever applies.
        public var label: String {
            enumString ?? flagString ?? String(value)
        }

        /// Is the flag at position `i` (0 = the field's high bit) set?
        public func isSet(bitFromTop i: Int) -> Bool {
            (value >> BigInt(width - 1 - i)) & 1 == 1
        }
    }

    /// Parse a layout from either a MAP — each entry's value a positive integer
    /// bit WIDTH (`owner: 3`) or an array of per-bit FLAG names
    /// (`owner: ["r","w","x"]`, width = count) — OR a typed `Bits::BitFormat`
    /// RECORD with a `fields` list of `BitField` records (`name`, `bits`,
    /// `flags`), read structurally by field name. Insertion order is preserved
    /// (first = highest field). Returns nil if it's neither shape.
    public static func layout(from value: Value) -> [FieldSpec]? {
        switch value {
        case .map(let entries):
            guard !entries.isEmpty else { return nil }
            var layout: [FieldSpec] = []
            for entry in entries {
                switch entry.value {
                case .number(let n):
                    guard n.isInteger, let width = n.intValue, width >= 1 else { return nil }
                    layout.append(FieldSpec(name: entry.key, width: width))
                case .array(let items):
                    guard !items.isEmpty else { return nil }
                    var names: [String] = []
                    for item in items {
                        guard case .string(let name) = item else { return nil }
                        names.append(name)
                    }
                    layout.append(FieldSpec(name: entry.key, width: names.count, flags: names))
                default:
                    return nil
                }
            }
            return layout

        case .record(let record):
            // A BitFormat-shaped record: a `fields` list of BitField records. Each
            // field is a flags / enum / numeric field — chosen by `kind` when the
            // record carries it, else derived from which list is non-empty.
            guard case .array(let fieldValues)? = member(record, "fields"), !fieldValues.isEmpty
            else { return nil }
            var layout: [FieldSpec] = []
            for fieldValue in fieldValues {
                guard case .record(let field) = fieldValue,
                      case .string(let name)? = member(field, "name") else { return nil }
                let kind: String? = if case .string(let k)? = member(field, "kind") { k } else { nil }
                let flags = stringList(member(field, "flags"))
                let values = stringList(member(field, "values"))
                if (kind == "flags" || kind == nil), let flags, !flags.isEmpty {
                    layout.append(FieldSpec(name: name, width: flags.count, flags: flags))
                } else if (kind == "enum" || kind == nil), let values, !values.isEmpty,
                          case .number(let bits)? = member(field, "bits"),
                          bits.isInteger, let width = bits.intValue, width >= 1 {
                    layout.append(FieldSpec(name: name, width: width, values: values))
                } else if case .number(let bits)? = member(field, "bits"),
                          bits.isInteger, let width = bits.intValue, width >= 1 {
                    layout.append(FieldSpec(name: name, width: width))
                } else {
                    return nil
                }
            }
            return layout

        default:
            return nil
        }
    }

    private static func member(_ record: Value.RecordValue, _ key: String) -> Value? {
        record.entries.first { $0.key == key }?.value
    }

    private static func stringList(_ value: Value?) -> [String]? {
        guard case .array(let items)? = value else { return nil }
        var names: [String] = []
        for item in items { guard case .string(let name) = item else { return nil }; names.append(name) }
        return names
    }

    /// Build a format map from numeric (name, width) pairs — the inverse of
    /// `layout(from:)` (the `MapEntry` initializer is module-internal).
    public static func formatMap(_ pairs: [(name: String, width: Int)]) -> Value {
        .map(pairs.map { Value.MapEntry(key: $0.name, value: .number(BigDecimal($0.width))) })
    }

    /// Build a format map from flag fields — each value is an array of per-bit
    /// flag names (`owner: ["r","w","x"]`).
    public static func flagFormatMap(_ pairs: [(name: String, flags: [String])]) -> Value {
        .map(pairs.map { pair in
            Value.MapEntry(key: pair.name, value: .array(pair.flags.map { Value.string($0) }))
        })
    }

    /// The total width a layout occupies.
    public static func layoutWidth(_ layout: [FieldSpec]) -> Int {
        layout.reduce(0) { $0 + $1.width }
    }

    /// Decode the current value into `layout`'s fields (high→low, matching the
    /// grid). Bits above the layout's total are simply unlabeled.
    public func fields(_ layout: [FieldSpec]) -> [Field] {
        var top = Self.layoutWidth(layout)
        return layout.map { f in
            let low = top - f.width
            top = low
            let mask = (BigInt(1) << f.width) - 1
            let value = low >= 0 ? (pattern >> low) & mask : BigInt(0)
            return Field(name: f.name, width: f.width, lowBit: max(low, 0), value: value,
                         flags: f.flags, values: f.values)
        }
    }

    /// A new view with field `name` set to `value` (clamped to the field's
    /// width), every other bit unchanged. Unknown name → unchanged.
    public func setting(field name: String, to value: BigInt, layout: [FieldSpec]) -> BinaryView {
        var top = Self.layoutWidth(layout)
        let registerMask = (BigInt(1) << width) - 1
        for f in layout {
            let low = top - f.width
            top = low
            guard f.name == name, low >= 0 else { continue }
            let fieldMask = ((BigInt(1) << f.width) - 1) << low
            let cleared = pattern & (registerMask ^ fieldMask)
            let placed = ((max(value, 0) << low) & fieldMask)
            return BinaryView(kind: kind, width: width, pattern: cleared | placed)
        }
        return self
    }
}
