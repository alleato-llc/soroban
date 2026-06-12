import SorobanEngine
import Foundation

// MARK: Environment inspector data (the sheet half)
//
// The inspector shows every live name with its value and PROVENANCE — log
// (a 𝑖/λ/𝑫 typed into the calculator) or a specific cell. The log half lives
// on CalculatorSession (logVariables/…); this gathers the sheet-scoped
// definitions and named cells across every worksheet.

extension SheetModel {
    /// Where a name comes from — drives the badge and the click-to-jump.
    enum Provenance: Equatable {
        case log
        case cell(sheet: String, address: CellAddress)
    }

    struct EnvEntry: Identifiable {
        let id: String          // unique within its section
        let name: String        // display label (signature for functions)
        let detail: String      // value, field list, or "λ"
        let provenance: Provenance
    }

    /// Sheet-scoped 𝑖 definitions across all sheets, with their current
    /// values (evaluated lazily — errors render as "—").
    func sheetVariables() -> [EnvEntry] {
        sheetDefinitions(matching: .variable).map { sheet, def in
            let value = (try? sheet.grid.definedValue(named: def.name))?
                .flatMap { $0 }?.description ?? "—"
            return EnvEntry(id: "\(sheet.name)!\(def.address)",
                            name: def.name, detail: value,
                            provenance: .cell(sheet: sheet.name, address: def.address))
        }
    }

    func sheetFunctions() -> [EnvEntry] {
        sheetDefinitions(matching: .function).map { sheet, def in
            EnvEntry(id: "\(sheet.name)!\(def.address)",
                     name: def.signature, detail: "λ",
                     provenance: .cell(sheet: sheet.name, address: def.address))
        }
    }

    func sheetDataTypes() -> [EnvEntry] {
        sheetDefinitions(matching: .dataType).map { sheet, def in
            EnvEntry(id: "\(sheet.name)!\(def.address)",
                     name: def.name, detail: "𝑫",
                     provenance: .cell(sheet: sheet.name, address: def.address))
        }
    }

    /// Every named cell across all sheets, with its current numeric value.
    func namedCells() -> [EnvEntry] {
        _ = generation
        var entries: [EnvEntry] = []
        for sheet in store.sheets where !sheet.isData {
            for (address, name) in sheet.grid.cellNames.sorted(by: { $0.value < $1.value }) {
                let value = (try? sheet.grid.numericValue(
                    column: address.columnName, row: address.rowNumber))?.description ?? "—"
                entries.append(EnvEntry(id: "\(sheet.name)!\(address)",
                                        name: "'\(name)'", detail: value,
                                        provenance: .cell(sheet: sheet.name, address: address)))
            }
        }
        return entries
    }

    private func sheetDefinitions(matching kind: Spreadsheet.SheetDefinition.Kind)
        -> [(sheet: Sheet, def: Spreadsheet.SheetDefinition)] {
        _ = generation
        var result: [(Sheet, Spreadsheet.SheetDefinition)] = []
        for sheet in store.sheets where !sheet.isData {
            for def in sheet.grid.definitions.values.sorted(by: { $0.name < $1.name })
            where def.kind == kind {
                result.append((sheet, def))
            }
        }
        return result
    }
}
