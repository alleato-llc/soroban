import SorobanEngine

// MARK: CSV export (File ▸ Export CSV…)

extension SheetModel {
    /// The active sheet as CSV text — computed VALUES, not formulas (the
    /// interop convention): numbers render plain (no thousands grouping),
    /// controls export their current value, definitions export their source.
    func activeSheetCSV() -> String {
        let sheet = store.activeSheet
        let rowCount: Int
        let columnCount: Int
        if let data = sheet.data {
            rowCount = data.rowCount // the full table, beyond the visible 10k
            columnCount = data.columnCount
        } else {
            let used = sheet.grid.cells.keys
            rowCount = (used.map(\.row).max() ?? -1) + 1
            columnCount = (used.map(\.column).max() ?? -1) + 1
        }
        var rows: [[String]] = []
        rows.reserveCapacity(rowCount)
        for row in 0..<rowCount {
            var fields: [String] = []
            fields.reserveCapacity(columnCount)
            for column in 0..<columnCount {
                fields.append(exportField(at: CellAddress(column: column, row: row), on: sheet))
            }
            rows.append(fields)
        }
        return CSV.encode(rows)
    }

    private func exportField(at address: CellAddress, on sheet: Sheet) -> String {
        if let data = sheet.data {
            return data.rawValue(row: address.row, column: address.column)
        }
        switch sheet.grid.displayValue(at: address) {
        case .empty: return ""
        case .text(let text): return text
        case .value(let value): return value.description
        case .error: return "#ERR"
        case .definition, .note: return sheet.grid.raw(at: address) // the source/note is the fact
        case .slider(let info), .stepper(let info): return info.value.description
        case .checkbox(let info): return info.isOn ? "1" : "0"
        case .dropdown(let info): return info.value.displayText
        }
    }
}
