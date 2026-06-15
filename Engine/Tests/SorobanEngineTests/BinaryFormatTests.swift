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
        #expect(decoded?.map(\.color) == Array(BinaryEditorPalette.names.prefix(3)))
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
