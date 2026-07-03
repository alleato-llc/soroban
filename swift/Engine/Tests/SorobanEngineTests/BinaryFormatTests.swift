import Testing
import BigInt
@testable import Anzan
@testable import SorobanEngine

/// The shared bit-editor format library that moved into the engine
/// (`BinaryEditorBits` serializer, `BinaryEditorPresets`): the serializer is the
/// wire format every saved format persists as, so it earns a round-trip; the
/// presets ship to users, so they earn a well-formedness check.
@Suite("Binary editor format library")
struct BinaryFormatTests {
    /// Evaluate a layout's serialized `Bits::BitFormat(...)` and decode it back —
    /// the exact path a save/restore takes.
    private func roundTrip(_ layout: [BinaryView.FieldSpec]) -> [BinaryView.FieldSpec]? {
        let calc = Calculator()
        _ = calc.evaluate(BinaryEditorBits.schemaSource)
        guard case .success(let value) = calc.evaluateFormula(BinaryEditorBits.formatSource(layout))
        else { return nil }
        return BinaryView.layout(from: value)
    }

    @Test func numericFlagsAndEnumFieldsRoundTrip() {
        let layout: [BinaryView.FieldSpec] = [
            .init(name: "owner", width: 3, flags: ["r", "w", "x"], color: "blue"),
            .init(name: "mode", width: 2, values: ["idle", "run", "halt", "max"], color: "green"),
            .init(name: "port", width: 8, color: "orange", base: 16),
        ]
        #expect(roundTrip(layout) == layout)
    }

    @Test func reservedAndUnusedGapsRoundTrip() {
        let layout: [BinaryView.FieldSpec] = [
            .init(name: "hi", width: 4, color: "blue"),
            .init(name: "rsvd", width: 2, color: "green", reserved: true),
            .init(name: "spare", width: 2, color: "orange", unused: true),
        ]
        let decoded = roundTrip(layout)
        #expect(decoded == layout) // FieldSpec Equatable covers reserved/unused
        #expect(decoded?.map(\.reserved) == [false, true, false])
        #expect(decoded?.map(\.unused) == [false, false, true])
    }

    @Test func aBasePersistsThroughTheSerializer() {
        for base in [2, 8, 16] {
            let layout: [BinaryView.FieldSpec] = [.init(name: "v", width: 8, color: "blue", base: base)]
            #expect(roundTrip(layout)?.first?.base == base)
        }
        // 10 is the decimal default — it normalizes back to nil, not 10.
        let decimal: [BinaryView.FieldSpec] = [.init(name: "v", width: 8, color: "blue", base: 10)]
        #expect(roundTrip(decimal)?.first?.base == nil)
    }

    @Test func aColorlessLayoutIsCanonicalizedToThePaletteByPosition() {
        let bare: [BinaryView.FieldSpec] = [
            .init(name: "a", width: 4), .init(name: "b", width: 4), .init(name: "c", width: 4),
        ]
        let decoded = roundTrip(bare)
        #expect(decoded?.map(\.name) == ["a", "b", "c"])
        #expect(decoded?.map(\.width) == [4, 4, 4])
        #expect(decoded?.compactMap(\.color) == Array(BinaryEditorPalette.names.prefix(3)))
    }

    /// `formatValue` is the loose-map encoder for the richer presets (enum /
    /// reserved / flags / numeric in one layout, including duplicate field
    /// names like EFLAGS's repeated "reserved"). It must round-trip through
    /// `layout(from:)` directly (no serializer hop — presets ship as Values).
    @Test func formatValueRoundTripsMixedFields() {
        let layout: [BinaryView.FieldSpec] = [
            .init(name: "QR", width: 1, flags: ["QR"]),
            .init(name: "Opcode", width: 4, values: ["QUERY", "IQUERY", "STATUS"]),
            .init(name: "Z", width: 3, reserved: true),
            .init(name: "spare", width: 2, unused: true),
            .init(name: "addr", width: 8, base: 16),
        ]
        let decoded = BinaryView.layout(from: BinaryView.formatValue(layout))
        #expect(decoded?.map(\.name) == ["QR", "Opcode", "Z", "spare", "addr"])
        #expect(decoded?.map(\.width) == [1, 4, 3, 2, 8])
        #expect(decoded?.map(\.flags) == [["QR"], nil, nil, nil, nil])
        #expect(decoded?.map(\.values) == [nil, ["QUERY", "IQUERY", "STATUS"], nil, nil, nil])
        #expect(decoded?.map(\.reserved) == [false, false, true, false, false])
        #expect(decoded?.map(\.unused) == [false, false, false, true, false])
        #expect(decoded?.map(\.base) == [nil, nil, nil, nil, 16])
    }

    /// Duplicate field names (EFLAGS repeats "reserved"/"flags") must survive
    /// encode/decode in order — the parser keys off position, not name.
    @Test func formatValuePreservesDuplicateFieldNamesInOrder() {
        let layout: [BinaryView.FieldSpec] = [
            .init(name: "reserved", width: 2, reserved: true),
            .init(name: "flags", width: 2, flags: ["A", "B"]),
            .init(name: "reserved", width: 1, reserved: true),
            .init(name: "flags", width: 2, flags: ["C", "D"]),
        ]
        let decoded = BinaryView.layout(from: BinaryView.formatValue(layout))
        #expect(decoded?.map(\.name) == ["reserved", "flags", "reserved", "flags"])
        #expect(decoded?.map(\.width) == [2, 2, 1, 2])
        #expect(decoded?.map(\.reserved) == [true, false, true, false])
    }

    /// Decode real, known-good values through the new presets — the proof the
    /// layouts are not just the right *width* but the right *bits*, decoded by
    /// the same `fields()` path the editor uses.
    @Test func richPresetsDecodeRealWorldValues() {
        func decode(_ name: String, _ pattern: BigInt, width: Int) -> [BinaryView.Field]? {
            guard let format = BinaryEditorPresets.standard.first(where: { $0.name == name })?.format,
                  let layout = BinaryView.layout(from: format) else { return nil }
            return BinaryView(kind: .plain, width: width, pattern: pattern).fields(layout)
        }
        func field(_ fs: [BinaryView.Field]?, _ name: String) -> BinaryView.Field? {
            fs?.first { $0.name == name }
        }

        // IEEE 754 float 1.0 = 0x3F800000 → sign 0, exponent 127 (biased), mantissa 0.
        let f = decode("IEEE 754 float", BigInt(0x3F80_0000), width: 32)
        #expect(field(f, "sign")?.value == 0)
        #expect(field(f, "exponent")?.value == 127)
        #expect(field(f, "mantissa")?.value == 0)
        // IEEE 754 double -2.0 = 0xC000000000000000 → sign 1, exponent 1024, mantissa 0.
        let d = decode("IEEE 754 double", (BigInt(1) << 63) | (BigInt(1) << 62), width: 64)
        #expect(field(d, "sign")?.value == 1)
        #expect(field(d, "exponent")?.value == 1024)

        // Unix st_mode 0o100644 = regular file (type 0x8), rw-r--r--.
        let m = decode("Unix mode (st_mode)", BigInt(0o100644), width: 16)
        #expect(field(m, "type")?.value == 8)        // S_IFREG high nibble
        #expect(field(m, "owner")?.flagString == "rw-")
        #expect(field(m, "group")?.flagString == "r--")
        #expect(field(m, "other")?.flagString == "r--")

        // x86 EFLAGS 0x45 → CF (bit 0), PF (bit 2), ZF (bit 6) set; nothing else.
        let e = decode("x86 EFLAGS", BigInt(0x45), width: 32)
        #expect(field(e, "CF")?.flagString == "CF")
        #expect(field(e, "PF")?.flagString == "PF")
        #expect(e?.first { $0.flags?.contains("ZF") == true }?.flagString == "ZF")
        #expect(e?.first { $0.flags?.contains("CWR") == true } == nil) // sanity: no cross-format leakage

        // RGBA8888 0xAABBCCDD → r=0xAA, g=0xBB, b=0xCC, a=0xDD (hex readout).
        let c = decode("RGBA8888", BigInt(0xAABB_CCDD), width: 32)
        #expect(field(c, "r")?.valueText == "0xaa")
        #expect(field(c, "a")?.valueText == "0xdd")
    }

    @Test func everyPresetDecodesAndIsWellFormed() {
        for (name, format) in BinaryEditorPresets.standard {
            let layout = BinaryView.layout(from: format)
            #expect(layout != nil, "preset \(name) should decode to a layout")
            #expect(layout?.allSatisfy { $0.width >= 1 } == true, "preset \(name) has a zero-width field")
            #expect(layout?.isEmpty == false, "preset \(name) is empty")
        }
    }

    @Test func presetTotalWidthsMatchTheirProtocols() {
        func bits(_ name: String) -> Int? {
            guard let format = BinaryEditorPresets.standard.first(where: { $0.name == name })?.format,
                  let layout = BinaryView.layout(from: format) else { return nil }
            return BinaryView.layoutWidth(layout)
        }
        #expect(bits("Unix permissions") == 9)   // rwx × 3
        #expect(bits("TCP flags") == 8)
        #expect(bits("RGB565") == 16)
        #expect(bits("IPv4 address") == 32)
        #expect(bits("MAC address") == 48)
        #expect(bits("IPv6 address") == 128)
        // Floating point.
        #expect(bits("IEEE 754 float") == 32)
        #expect(bits("IEEE 754 double") == 64)
        #expect(bits("IEEE 754 half") == 16)
        #expect(bits("bfloat16") == 16)
        // Color.
        #expect(bits("RGBA8888") == 32)
        #expect(bits("ARGB1555") == 16)
        #expect(bits("RGBA4444") == 16)
        // Networking.
        #expect(bits("DNS header flags") == 16)
        #expect(bits("VLAN 802.1Q tag") == 16)
        #expect(bits("IPv4 DSCP/ECN") == 8)
        // Systems.
        #expect(bits("x86 EFLAGS") == 32)
        #expect(bits("Unix mode (st_mode)") == 16)
        #expect(bits("FAT date") == 16)
        #expect(bits("FAT time") == 16)
    }

    /// The richer presets carry enum / reserved fields — spot-check that the
    /// decode preserves those kinds (not just total width).
    @Test func richPresetsCarryEnumAndReservedFields() {
        func layout(_ name: String) -> [BinaryView.FieldSpec]? {
            guard let format = BinaryEditorPresets.standard.first(where: { $0.name == name })?.format
            else { return nil }
            return BinaryView.layout(from: format)
        }
        let dns = layout("DNS header flags")
        #expect(dns?.contains { $0.name == "Opcode" && $0.values != nil } == true)
        #expect(dns?.contains { $0.name == "Z" && $0.reserved } == true)

        let eflags = layout("x86 EFLAGS")
        #expect(eflags?.filter(\.reserved).count == 5)
        #expect(eflags?.contains { $0.flags?.contains("CF") == true } == true)

        let mode = layout("Unix mode (st_mode)")
        #expect(mode?.contains { $0.name == "type" && $0.base == 16 } == true)
        #expect(mode?.contains { $0.name == "owner" && $0.flags == ["r", "w", "x"] } == true)
    }

    /// The user-visible chain: save a format as a typed variable, close, reopen.
    @Test func aSavedFormatSurvivesSaveAndReopen() throws {
        let layout: [BinaryView.FieldSpec] = [
            .init(name: "ver", width: 4, color: "blue", base: 16),
            .init(name: "flags", width: 4, flags: ["A", "B", "C", "D"], color: "green"),
        ]
        let calc = Calculator()
        _ = calc.evaluate(BinaryEditorBits.schemaSource)
        _ = calc.evaluate("perm = \(BinaryEditorBits.formatSource(layout))")

        let workbook = Workbook(
            sheets: [], variables: calc.environment.userVariables,
            functions: calc.environment.allUserFunctions,
            dataTypes: calc.environment.userDataTypes,
            namespaces: calc.environment.namespaceSources,
            imports: calc.environment.importedNamespaces)
        let decoded = try Workbook.decode(try workbook.encode())

        let fresh = Calculator()
        fresh.restoreSession(from: decoded)
        let restored = try #require(fresh.environment.userVariables["perm"])
        #expect(BinaryView.layout(from: restored) == layout)
    }
}
