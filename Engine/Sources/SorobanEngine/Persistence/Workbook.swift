import Anzan
import Foundation

/// The `.soroban` workbook file payload: a versioned JSON envelope holding
/// raw cell contents and user variables (cells + the variables their formulas
/// depend on make a workbook self-contained). Pure data — file I/O and
/// panels live in the app layer.
public struct Workbook: Codable, Equatable, Sendable {
    public static let formatIdentifier = "soroban-workbook"
    /// v2: `functions` became an ordered list of source lines (was a
    /// name→source map) to carry typed operator/function overloads.
    public static let currentVersion = 2

    public var format: String
    public var version: Int
    /// One worksheet's payload.
    public struct SheetPayload: Codable, Equatable, Sendable {
        public var name: String
        /// "A:1" → raw cell content, exactly as typed (markers included).
        /// Empty for data sheets (their values live in data.sqlite).
        public var cells: [String: String]
        /// "data" marks a sheet backed by a table in the package's
        /// data.sqlite; nil/absent means a normal grid sheet.
        public var kind: String?
        /// The data.sqlite table backing a data sheet.
        public var table: String?
        /// Non-default column widths, keyed by column name ("A") in points.
        public var columnWidths: [String: Double]
        /// Non-default row heights, keyed by 1-based row number ("5") in points.
        public var rowHeights: [String: Double]
        /// Per-cell presentation, keyed "A:1" — only non-default formats.
        /// Decodes to empty for files written before formatting existed.
        public var formats: [String: CellFormat]
        /// Named cells, keyed "A:1" → the name ('Projected Rate' syntax).
        public var names: [String: String]

        public var isData: Bool { kind == "data" }

        public init(name: String, cells: [String: String],
                    kind: String? = nil, table: String? = nil,
                    columnWidths: [String: Double] = [:],
                    rowHeights: [String: Double] = [:],
                    formats: [String: CellFormat] = [:],
                    names: [String: String] = [:]) {
            self.name = name
            self.cells = cells
            self.kind = kind
            self.table = table
            self.columnWidths = columnWidths
            self.rowHeights = rowHeights
            self.formats = formats
            self.names = names
        }

        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            name = try container.decode(String.self, forKey: .name)
            cells = try container.decodeIfPresent([String: String].self, forKey: .cells) ?? [:]
            kind = try container.decodeIfPresent(String.self, forKey: .kind)
            table = try container.decodeIfPresent(String.self, forKey: .table)
            columnWidths = try container.decodeIfPresent([String: Double].self, forKey: .columnWidths) ?? [:]
            rowHeights = try container.decodeIfPresent([String: Double].self, forKey: .rowHeights) ?? [:]
            formats = try container.decodeIfPresent([String: CellFormat].self, forKey: .formats) ?? [:]
            names = try container.decodeIfPresent([String: String].self, forKey: .names) ?? [:]
        }
    }

    /// Ordered worksheets. Always at least one after decoding.
    public var sheets: [SheetPayload]
    /// Which sheet was active when saved.
    public var activeSheet: String?
    /// Variable name → value via `Value.description` (round-trips exactly —
    /// numbers as before, structures as their canonical literals).
    public var variables: [String: String]
    /// Function definition lines ("f(x) = x * 2"), in order — re-evaluated on
    /// open. A list, not a name→source map, because one name can have several
    /// typed overloads (`+(a: Point, b: Point)`, `+(a: Point, s: Number)`).
    /// Decodes a legacy name→source object too (pre-overload files).
    public var functions: [String]
    /// Data type name → original declaration line ("data Person { … }").
    /// Re-evaluated on open BEFORE variables (record variables persist as
    /// constructor calls). Decodes to empty for older files. Excludes namespace
    /// members (qualified `Bits::BitField` names) — those restore via `namespaces`.
    public var dataTypes: [String: String]
    /// Namespace declaration lines ("namespace Bits { … }"), in order — replayed
    /// on open to re-register their (qualified) members. Decodes empty for older
    /// files. (docs/MODULES.md 2c)
    public var namespaces: [String]
    /// Imported namespace names, restored (after `namespaces`) by replaying
    /// `import Name`. Decodes empty for older files.
    public var imports: [String]

    public init(sheets: [SheetPayload], activeSheet: String? = nil,
                variables: [String: Value],
                functions: [UserFunction] = [],
                dataTypes: [String: DataType] = [:],
                namespaces: [String] = [],
                imports: [String] = []) {
        self.format = Self.formatIdentifier
        self.version = Self.currentVersion
        self.sheets = sheets
        self.activeSheet = activeSheet
        self.variables = variables.mapValues(\.description)
        // Namespace members carry qualified names + empty source; they restore
        // via `namespaces`, so keep them out of the flat function/type maps.
        self.functions = functions.filter { !$0.name.contains("::") }.map(\.source)
        self.dataTypes = Dictionary(uniqueKeysWithValues:
            dataTypes.values.filter { !$0.name.contains("::") }.map { ($0.name, $0.source) })
        self.namespaces = namespaces
        self.imports = imports
    }

    /// Single-sheet convenience (tests, simple tooling).
    public init(cells: [String: String], variables: [String: Value],
                functions: [UserFunction] = [],
                columnWidths: [String: Double] = [:],
                rowHeights: [String: Double] = [:]) {
        self.init(sheets: [SheetPayload(name: "Sheet 1", cells: cells,
                                        columnWidths: columnWidths,
                                        rowHeights: rowHeights)],
                  variables: variables, functions: functions)
    }

    /// Back-compat view of the first sheet (kept for older call sites).
    public var cells: [String: String] { sheets.first?.cells ?? [:] }
    public var columnWidths: [String: Double] { sheets.first?.columnWidths ?? [:] }
    public var rowHeights: [String: Double] { sheets.first?.rowHeights ?? [:] }

    enum CodingKeys: String, CodingKey {
        case format, version, sheets, activeSheet, variables, functions, dataTypes, namespaces, imports
        // Legacy flat single-sheet fields (read-only).
        case cells, columnWidths, rowHeights
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        format = try container.decode(String.self, forKey: .format)
        version = try container.decode(Int.self, forKey: .version)
        variables = try container.decode([String: String].self, forKey: .variables)
        // v2+: an ordered list of source lines. Legacy (v1): a name→source map.
        if let lines = try? container.decode([String].self, forKey: .functions) {
            functions = lines
        } else if let legacy = try container.decodeIfPresent([String: String].self, forKey: .functions) {
            functions = legacy.sorted { $0.key < $1.key }.map(\.value)
        } else {
            functions = []
        }
        dataTypes = try container.decodeIfPresent([String: String].self, forKey: .dataTypes) ?? [:]
        namespaces = try container.decodeIfPresent([String].self, forKey: .namespaces) ?? []
        imports = try container.decodeIfPresent([String].self, forKey: .imports) ?? []
        activeSheet = try container.decodeIfPresent(String.self, forKey: .activeSheet)

        if let decoded = try container.decodeIfPresent([SheetPayload].self, forKey: .sheets),
           !decoded.isEmpty {
            sheets = decoded
        } else {
            // Legacy flat format: the whole file was one implicit sheet.
            sheets = [SheetPayload(
                name: "Sheet 1",
                cells: try container.decodeIfPresent([String: String].self, forKey: .cells) ?? [:],
                columnWidths: try container.decodeIfPresent([String: Double].self, forKey: .columnWidths) ?? [:],
                rowHeights: try container.decodeIfPresent([String: Double].self, forKey: .rowHeights) ?? [:])]
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(format, forKey: .format)
        try container.encode(version, forKey: .version)
        try container.encode(sheets, forKey: .sheets)
        try container.encodeIfPresent(activeSheet, forKey: .activeSheet)
        try container.encode(variables, forKey: .variables)
        try container.encode(functions, forKey: .functions)
        try container.encode(dataTypes, forKey: .dataTypes)
        if !namespaces.isEmpty { try container.encode(namespaces, forKey: .namespaces) }
        if !imports.isEmpty { try container.encode(imports, forKey: .imports) }
    }

    /// Parsed variables; entries that fail to parse are dropped (they could
    /// only come from a hand-edited file). Numbers take the fast path;
    /// structured values re-parse from their canonical literals.
    public var parsedVariables: [String: Value] {
        variables.compactMapValues(Value.init(parsing:))
    }

    // MARK: Codec

    public func encode() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(self)
    }

    public static func decode(_ data: Data) throws -> Workbook {
        let workbook: Workbook
        do {
            workbook = try JSONDecoder().decode(Workbook.self, from: data)
        } catch {
            throw WorkbookError.notAWorkbook
        }
        guard workbook.format == formatIdentifier else {
            throw WorkbookError.notAWorkbook
        }
        guard workbook.version <= currentVersion else {
            throw WorkbookError.unsupportedVersion(workbook.version)
        }
        return workbook
    }
}

public enum WorkbookError: Error, Equatable, CustomStringConvertible {
    case notAWorkbook
    case unsupportedVersion(Int)

    public var description: String {
        switch self {
        case .notAWorkbook:
            return "This file is not a Soroban workbook."
        case .unsupportedVersion(let version):
            return "This workbook uses format version \(version), which needs a newer version of Soroban."
        }
    }
}
