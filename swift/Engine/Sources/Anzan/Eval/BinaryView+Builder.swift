import Foundation // String trimming

extension BinaryView {
    /// The model behind the app's visual bit-field builder — pure and host-free,
    /// so the SwiftUI view is just bindings over this. You claim a contiguous run
    /// of the open bits (`claim`), describe the pending field with the `draft*`
    /// inputs, then `addField`; the accumulated `fields` produce a `layout`
    /// (`[FieldSpec]`) that drives the editor and saves as a `Bits::BitFormat`.
    public struct FormatBuilder: Equatable, Sendable {
        public enum FieldKind: String, CaseIterable, Sendable, Identifiable {
            case numeric = "Numeric", flags = "Flags", enumeration = "Enum"
            case reserved = "Reserved", unused = "Unused"
            public var id: String { rawValue }
        }

        /// One field as the builder holds it (richer than `FieldSpec`: it keeps
        /// the editable `kind` + raw label list + a stable id for the UI list).
        public struct Field: Equatable, Sendable, Identifiable {
            public let id: Int
            public var name: String
            public var width: Int
            public var kind: FieldKind
            public var labels: [String] // flags: per-bit names; enum: value labels
            public var colorName: String
            public var base: Int // numeric display radix (10 decimal, 16 hex)

            /// The engine `FieldSpec` this field becomes — flags padded/truncated
            /// to the bit width, enum labels as-is, base dropped when decimal.
            public var spec: FieldSpec {
                switch kind {
                case .numeric:
                    return FieldSpec(name: name, width: width, color: colorName,
                                     base: base == 10 ? nil : base)
                case .flags:
                    var f = labels
                    if f.count < width { f += Array(repeating: "?", count: width - f.count) }
                    return FieldSpec(name: name, width: width, flags: Array(f.prefix(width)),
                                     color: colorName)
                case .enumeration:
                    return FieldSpec(name: name, width: width, values: labels, color: colorName)
                case .reserved:
                    return FieldSpec(name: name, width: width, color: colorName, reserved: true)
                case .unused:
                    return FieldSpec(name: name, width: width, color: colorName, unused: true)
                }
            }
        }

        private let palette: [String]
        private var nextID = 0

        public private(set) var fields: [Field] = []
        /// Bits claimed for the field about to be added (0 = none claimed).
        public private(set) var pendingWidth = 0

        // The pending field's editable inputs (the view binds these directly).
        public var draftName = ""
        public var draftKind: FieldKind = .numeric
        public var draftLabels = "" // comma-separated, for flags/enum
        public var draftColor: String
        public var draftBase = 10

        public init(palette: [String]) {
            self.palette = palette.isEmpty ? ["blue"] : palette
            self.draftColor = self.palette[0]
        }

        // MARK: Derived

        public var committedWidth: Int { fields.reduce(0) { $0 + $1.width } }
        public func freeBits(registerWidth: Int) -> Int { Swift.max(0, registerWidth - committedWidth) }
        public var layout: [FieldSpec] { fields.map(\.spec) }
        public var isEmpty: Bool { fields.isEmpty }
        /// Reserved and Unused are nameless "gap" fields (no name required).
        public var isGapKind: Bool { draftKind == .reserved || draftKind == .unused }
        public var canAddField: Bool {
            pendingWidth >= 1 && (isGapKind || !draftName.trimmingCharacters(in: .whitespaces).isEmpty)
        }

        // MARK: Mutation

        /// Claim a `bits`-wide pending group; clicking the same far edge clears it.
        public mutating func claim(_ bits: Int) {
            pendingWidth = (pendingWidth == bits) ? 0 : Swift.max(0, bits)
        }

        /// Commit the pending field from the draft inputs, then reset the draft
        /// (advancing the default color so successive fields differ). No-op when
        /// `canAddField` is false.
        public mutating func addField() {
            let trimmed = draftName.trimmingCharacters(in: .whitespaces)
            guard pendingWidth >= 1, isGapKind || !trimmed.isEmpty else { return }
            let name = isGapKind && trimmed.isEmpty ? draftKind.rawValue.lowercased() : trimmed
            let labels = (draftKind == .flags || draftKind == .enumeration) ? Self.parseLabels(draftLabels) : []
            fields.append(Field(id: nextID, name: name, width: pendingWidth, kind: draftKind,
                                labels: labels, colorName: draftColor, base: draftBase))
            nextID += 1
            resetDraft()
        }

        public mutating func remove(_ id: Field.ID) {
            fields.removeAll { $0.id == id }
            pendingWidth = 0
        }

        public mutating func recolor(_ id: Field.ID, to name: String) {
            if let i = fields.firstIndex(where: { $0.id == id }) { fields[i].colorName = name }
        }

        /// Rebuild from an existing layout, so an active format can be tweaked.
        public mutating func seed(from layout: [FieldSpec]) {
            fields = layout.enumerated().map { i, spec in
                let color = spec.color ?? palette[i % palette.count]
                if spec.reserved {
                    return Field(id: i, name: spec.name, width: spec.width, kind: .reserved,
                                 labels: [], colorName: color, base: 10)
                } else if spec.unused {
                    return Field(id: i, name: spec.name, width: spec.width, kind: .unused,
                                 labels: [], colorName: color, base: 10)
                } else if let flags = spec.flags {
                    return Field(id: i, name: spec.name, width: spec.width, kind: .flags,
                                 labels: flags, colorName: color, base: 10)
                } else if let values = spec.values {
                    return Field(id: i, name: spec.name, width: spec.width, kind: .enumeration,
                                 labels: values, colorName: color, base: 10)
                } else {
                    return Field(id: i, name: spec.name, width: spec.width, kind: .numeric,
                                 labels: [], colorName: color, base: spec.base ?? 10)
                }
            }
            nextID = fields.count
            resetDraft()
        }

        private mutating func resetDraft() {
            pendingWidth = 0
            draftName = ""
            draftKind = .numeric
            draftLabels = ""
            draftBase = 10
            draftColor = palette[fields.count % palette.count]
        }

        private static func parseLabels(_ text: String) -> [String] {
            text.split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
        }
    }
}
