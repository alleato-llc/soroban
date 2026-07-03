import SorobanEngine
import Foundation

// MARK: Data sheets (SQLite-backed imports)

extension SheetModel {
    /// Resets the working database for open/new: closes the current store,
    /// replaces the file with the package's copy (or nothing).
    func prepareWorkingDatabase(copyFrom packageDatabase: URL?) {
        dataStore = nil // closes the handle
        guard let working = Self.workingDataURL else { return }
        let fm = FileManager.default
        for suffix in ["", "-wal", "-shm"] {
            try? fm.removeItem(at: URL(fileURLWithPath: working.path + suffix))
        }
        if let packageDatabase {
            try? fm.copyItem(at: packageDatabase, to: working)
            dataStore = try? DataStore(url: working)
        }
    }

    /// Imports a CSV as a new data sheet — a copy of the file in the
    /// workbook's SQLite store (editable; the source file is never touched).
    /// Returns a user-facing error message, or nil on success.
    func importCSV(from url: URL) -> String? {
        guard let text = (try? String(contentsOf: url, encoding: .utf8))
            ?? (try? String(contentsOf: url, encoding: .isoLatin1)) else {
            return "couldn't read \(url.lastPathComponent) as text"
        }
        var rows = CSV.parse(text)
        guard !rows.isEmpty else { return "\(url.lastPathComponent) has no rows" }
        var truncatedColumns = false
        rows = rows.map { row in
            if row.count > Spreadsheet.columnCount { truncatedColumns = true }
            return Array(row.prefix(Spreadsheet.columnCount))
        }

        // Sheet/table name from the file name, sanitized to the sheet rules.
        var base = url.deletingPathExtension().lastPathComponent
            .replacingOccurrences(of: "!", with: " ")
            .replacingOccurrences(of: "'", with: " ")
            .trimmingCharacters(in: .whitespaces)
        if base.isEmpty { base = "Data" }
        base = String(base.prefix(SheetStore.maxNameLength - 4))
        var name = base
        var n = 2
        while store.sheets.contains(where: {
            $0.name.compare(name, options: .caseInsensitive) == .orderedSame
        }) {
            name = "\(base) \(n)"
            n += 1
        }

        do {
            let dataStore = try ensureDataStore()
            try dataStore.createTable(name: name, rows: rows)
            guard let data = DataSheet(table: name, store: dataStore) else {
                return "import failed"
            }
            try store.addDataSheet(named: name, data: data)
            store.activeIndex = store.sheets.count - 1
            endEditing()
            deselect()
            generation += 1
            persistAfterChange() // manifest changed (snapshot path)
            return truncatedColumns
                ? "imported \(rows.count) rows (columns beyond Z were dropped)" : nil
        } catch {
            return "\(error)"
        }
    }

    private func ensureDataStore() throws -> DataStore {
        if let dataStore { return dataStore }
        guard let url = Self.workingDataURL else {
            throw EngineError.domainError(message: "no working directory")
        }
        let created = try DataStore(url: url)
        dataStore = created
        return created
    }
}
