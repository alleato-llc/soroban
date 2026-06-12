import SorobanEngine
import Foundation

// MARK: Workbook content (what a .soroban file stores)

extension SheetModel {
    /// Snapshot of everything a `.soroban` file stores.
    func currentWorkbook() -> Workbook {
        Workbook(
            sheets: store.sheets.map { sheet in
                Workbook.SheetPayload(
                    name: sheet.name,
                    cells: sheet.isData ? [:] : Dictionary(uniqueKeysWithValues:
                        sheet.grid.raws.map { ("\($0.key)", $0.value) }),
                    kind: sheet.isData ? "data" : nil,
                    table: sheet.data?.table,
                    columnWidths: Dictionary(uniqueKeysWithValues: sheet.columnWidths.map {
                        (CellAddress.columnName(forIndex: $0.key), $0.value)
                    }),
                    rowHeights: Dictionary(uniqueKeysWithValues: sheet.rowHeights.map {
                        (String($0.key + 1), $0.value)
                    }),
                    formats: Dictionary(uniqueKeysWithValues: sheet.formats.map {
                        ("\($0.key)", $0.value)
                    }),
                    names: Dictionary(uniqueKeysWithValues: sheet.grid.cellNames.map {
                        ("\($0.key)", $0.value)
                    }))
            },
            activeSheet: store.activeSheet.name,
            variables: calculator.environment.userVariables,
            functions: calculator.environment.allUserFunctions,
            dataTypes: calculator.environment.userDataTypes)
    }

    /// Replaces the whole session state from a workbook (open / new / scratch
    /// restore): variables, functions, sheets with layout. Clears selection
    /// and, while untitled, persists to the scratch file.
    func apply(_ workbook: Workbook) {
        // Data types, then functions, then variables (record variables are
        // constructor calls and need their types back first) — the engine
        // owns that ordering.
        calculator.restoreSession(from: workbook)

        var newSheets: [Sheet] = []
        for payload in workbook.sheets {
            // Tolerate hand-edited duplicates/invalid names by skipping.
            guard (try? SheetStore.validated(
                name: payload.name,
                existing: newSheets, exceptIndex: nil)) != nil else { continue }

            let sheet: Sheet
            if payload.isData {
                // Tolerant: a data sheet whose table is missing is skipped.
                guard let dataStore,
                      let data = DataSheet(table: payload.table ?? payload.name,
                                           store: dataStore) else { continue }
                sheet = store.makeDataSheet(name: payload.name, data: data)
            } else {
                sheet = store.makeSheet(name: payload.name)
                var contents: [CellAddress: String] = [:]
                for (key, raw) in payload.cells {
                    guard let address = CellAddress(key: key) else { continue }
                    contents[address] = raw
                }
                sheet.grid.load(contents)
            }

            for (key, width) in payload.columnWidths {
                guard let column = CellAddress.columnIndex(forName: key) else { continue }
                sheet.columnWidths[column] = Double(CGFloat(width).clamped(to: Self.columnWidthRange))
            }
            for (key, height) in payload.rowHeights {
                guard let row = Int(key), (1...Spreadsheet.rowCount).contains(row) else { continue }
                sheet.rowHeights[row - 1] = Double(CGFloat(height).clamped(to: Self.rowHeightRange))
            }
            for (key, format) in payload.formats {
                guard let address = CellAddress(key: key), !format.isDefault else { continue }
                sheet.formats[address] = format
            }
            var cellNames: [CellAddress: String] = [:]
            for (key, name) in payload.names {
                guard let address = CellAddress(key: key) else { continue }
                cellNames[address] = name
            }
            sheet.grid.loadCellNames(cellNames)
            newSheets.append(sheet)
        }
        if newSheets.isEmpty {
            newSheets = [store.makeSheet(name: "Sheet 1")]
        }
        store.replaceSheets(newSheets, activeName: workbook.activeSheet)

        selected = nil
        editing = nil
        clearUndoHistory() // a different document — old edits don't apply
        generation += 1
        if autosaveToScratch {
            saveScratch()
        }
    }
}
