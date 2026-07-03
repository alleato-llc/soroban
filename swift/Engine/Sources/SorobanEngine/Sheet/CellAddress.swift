import Anzan
/// One grid coordinate. `column` 0-based (0 = A); `row` 0-based internally,
/// rendered 1-based ("A:1") to match the formula syntax.
///
/// ALL name↔index and 0-vs-1-based conversions live here — don't re-implement
/// column-letter or "A:1"-key parsing anywhere else.
public struct CellAddress: Hashable, Codable, Sendable, CustomStringConvertible {
    public let column: Int
    public let row: Int

    public init(column: Int, row: Int) {
        self.column = column
        self.row = row
    }

    /// From the user-facing forms: column name + 1-based row, bounds-checked.
    public init?(columnName: String, rowNumber: Int) {
        guard let column = Self.columnIndex(forName: columnName),
              (1...Spreadsheet.rowCount).contains(rowNumber) else { return nil }
        self.init(column: column, row: rowNumber - 1)
    }

    /// From a serialization key ("A:1"), as used in workbook files.
    public init?(key: String) {
        let parts = key.split(separator: ":")
        guard parts.count == 2, let rowNumber = Int(parts[1]) else { return nil }
        self.init(columnName: String(parts[0]), rowNumber: rowNumber)
    }

    /// "A" for 0, case-insensitive on the way in.
    public static func columnIndex(forName name: String) -> Int? {
        guard name.count == 1,
              let scalar = name.uppercased().unicodeScalars.first,
              scalar.value >= 65, scalar.value < 65 + UInt32(Spreadsheet.columnCount) else {
            return nil
        }
        return Int(scalar.value) - 65
    }

    public static func columnName(forIndex index: Int) -> String {
        String(UnicodeScalar(UInt8(65 + index)))
    }

    public var columnName: String { Self.columnName(forIndex: column) }

    /// 1-based, as displayed and serialized.
    public var rowNumber: Int { row + 1 }

    public var description: String { "\(columnName):\(rowNumber)" }
}
