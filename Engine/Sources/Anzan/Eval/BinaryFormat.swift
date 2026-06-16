/// Shared, host-neutral format data for the binary bit-editor — the band
/// palette, the typed `Bits` module schema + serializer, and the built-in
/// presets. These live beside `BinaryView` (the editor model already lives in
/// Anzan) so the calculator app, the standalone Tama app, and the test suite
/// all draw from ONE source — and so the serializer/presets are unit-testable
/// (a SwiftUI package has no test target; the engine does).

/// The bit-field band palette, by NAME — a field's persisted `color` is one of
/// these; a host maps each name to a real (theme-adapting) color.
public enum BinaryEditorPalette {
    public static let names = ["blue", "green", "orange", "purple", "pink", "teal"]
}

/// The `Bits` module schema and the serializer that renders a layout as a
/// re-parseable `Bits::BitFormat(...)` constructor (the typed form a format
/// saves as). Pure string work; a host evaluates the result through a
/// `Calculator` to persist or restore.
public enum BinaryEditorBits {
    /// Defined once per session, before the first saved format.
    public static let schemaSource =
        "namespace Bits { data BitField { name: String, bits: Number, kind: String, "
        + "flags: [String], values: [String], color: String, base: Number }; "
        + "data BitFormat { fields: [BitField] } }"

    /// A field with no explicit color gets the palette name for its position.
    public static func formatSource(_ layout: [BinaryView.FieldSpec]) -> String {
        func list(_ strings: [String]) -> String {
            strings.map { "\"\($0)\"" }.joined(separator: ", ")
        }
        let fields = layout.enumerated().map { index, spec -> String in
            let kind: String, flags: [String], values: [String]
            if spec.reserved {
                kind = "reserved"; flags = []; values = []
            } else if spec.unused {
                kind = "unused"; flags = []; values = []
            } else if let f = spec.flags, !f.isEmpty {
                kind = "flags"; flags = f; values = []
            } else if let v = spec.values, !v.isEmpty {
                kind = "enum"; flags = []; values = v
            } else {
                kind = "numeric"; flags = []; values = []
            }
            let color = spec.color ?? BinaryEditorPalette.names[index % BinaryEditorPalette.names.count]
            let base = spec.base ?? 10
            return "Bits::BitField(name: \"\(spec.name)\", bits: \(spec.width), "
                + "kind: \"\(kind)\", flags: [\(list(flags))], values: [\(list(values))], "
                + "color: \"\(color)\", base: \(base))"
        }.joined(separator: ", ")
        return "Bits::BitFormat(fields: [\(fields)])"
    }
}

/// Built-in formats shipped with the editor (not language constructs). Flag
/// fields decode each bit to a meaning (`r-x`); the networking formats read in
/// hex (`numericFormatMap`).
public enum BinaryEditorPresets {
    public static let standard: [(name: String, format: Value)] = [
        ("Unix permissions", BinaryView.flagFormatMap([
            ("owner", ["r", "w", "x"]), ("group", ["r", "w", "x"]), ("other", ["r", "w", "x"])])),
        ("TCP flags", BinaryView.flagFormatMap([
            ("flags", ["CWR", "ECE", "URG", "ACK", "PSH", "RST", "SYN", "FIN"])])),
        ("RGB565", BinaryView.formatMap([("r", 5), ("g", 6), ("b", 5)])),
        ("IPv4 address", BinaryView.formatMap([
            ("octet1", 8), ("octet2", 8), ("octet3", 8), ("octet4", 8)])),
        ("MAC address", BinaryView.numericFormatMap([
            ("oui1", 8, 16), ("oui2", 8, 16), ("oui3", 8, 16),
            ("nic1", 8, 16), ("nic2", 8, 16), ("nic3", 8, 16)])),
        ("IPv6 address", BinaryView.numericFormatMap([
            ("h1", 16, 16), ("h2", 16, 16), ("h3", 16, 16), ("h4", 16, 16),
            ("h5", 16, 16), ("h6", 16, 16), ("h7", 16, 16), ("h8", 16, 16)])),
    ]
}
