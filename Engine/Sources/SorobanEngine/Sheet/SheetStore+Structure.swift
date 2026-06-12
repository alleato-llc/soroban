import Anzan

// MARK: Structural edits (insert/delete rows & columns)
//
// One operation = two distinct effects, recorded separately so undo is exact:
//   1. RAW REWRITES across all grid sheets (shift / refError / range clamps),
//      recorded with PRE-move addresses and old+new text.
//   2. A CONTENT MOVE on the edited sheet (cells, names, formats, sizes
//      re-key; a delete also captures the removed slice).
// Undo runs the inverse move, restores the removed slice, then re-applies
// the OLD raws at their pre-op addresses. Redo just re-executes the op —
// the state at redo time is identical to op time, so the recompute is
// deterministic.

extension SheetStore {
    /// One cell's raw-text change, at its PRE-move address.
    public struct CellRewrite: Sendable {
        public let address: CellAddress
        public let old: String
        public let new: String
    }

    /// Everything needed to undo (or describe) one structural edit.
    /// Slot indices are 0-based on BOTH axes (CellAddress space).
    public struct StructuralChange: Sendable {
        public let axis: ReferenceRewriter.Axis
        public let index: Int
        public let count: Int
        public let isInsert: Bool
        public let sheetName: String
        public let rewrites: [String: [CellRewrite]]
        public let removedCells: [CellAddress: String]
        public let removedNames: [CellAddress: String]
        public let removedFormats: [CellAddress: CellFormat]
        public let removedSizes: [Int: Double] // heights (row axis) / widths (column)
    }

    /// Inserts `count` empty rows/columns at `slot`, shifting content and
    /// every reference (its own sheet's unqualified refs; qualified refs from
    /// anywhere). Refuses when occupied content would fall off the grid.
    public func insertSlots(axis: ReferenceRewriter.Axis, at slot: Int, count: Int,
                            in sheet: Sheet) throws -> StructuralChange {
        try validate(axis: axis, slot: slot, count: count, in: sheet, isInsert: true)
        let bound = axis == .row ? Spreadsheet.rowCount : Spreadsheet.columnCount
        let occupied = sheet.grid.cells.keys.map { position(of: $0, on: axis) }
            + sheet.grid.cellNames.keys.map { position(of: $0, on: axis) }
        if occupied.contains(where: { $0 >= bound - count && $0 >= slot }) {
            throw EngineError.domainError(
                message: "inserting would push cells off the grid (the last \(count) \(axis == .row ? "row(s)" : "column(s)") must be empty)")
        }
        return execute(axis: axis, slot: slot, count: count, isInsert: true, in: sheet)
    }

    /// Deletes `count` rows/columns at `slot`. References INTO the deleted
    /// band become `refError()`; range corners clamp inward.
    public func deleteSlots(axis: ReferenceRewriter.Axis, at slot: Int, count: Int,
                            in sheet: Sheet) throws -> StructuralChange {
        try validate(axis: axis, slot: slot, count: count, in: sheet, isInsert: false)
        return execute(axis: axis, slot: slot, count: count, isInsert: false, in: sheet)
    }

    /// Exact inverse of a recorded change (undo). The caller guarantees the
    /// stack discipline: the sheet is in the op's post state.
    public func revert(_ change: StructuralChange) {
        guard let sheet = sheets.first(where: {
            $0.name.compare(change.sheetName, options: .caseInsensitive) == .orderedSame
        }) else { return }
        let delta = change.isInsert ? -change.count : change.count
        let dropBand = change.isInsert ? change.index..<(change.index + change.count) : nil

        // 1. Other sheets: restore old raws.
        for (sheetName, rewrites) in change.rewrites
        where sheetName.compare(change.sheetName, options: .caseInsensitive) != .orderedSame {
            guard let other = sheets.first(where: {
                $0.name.compare(sheetName, options: .caseInsensitive) == .orderedSame
            }) else { continue }
            for rewrite in rewrites {
                other.grid.setCell(rewrite.old, at: rewrite.address)
            }
        }

        // 2. Edited sheet: inverse content move (+ removed-slice restore),
        //    then old raws at their pre-op addresses.
        var raws: [CellAddress: String] = [:]
        for (address, cell) in sheet.grid.cells {
            guard let moved = moved(address, on: change.axis, from: change.index + (change.isInsert ? change.count : 0),
                                    by: delta, dropping: dropBand) else { continue }
            raws[moved] = cell.raw
        }
        for (address, raw) in change.removedCells { raws[address] = raw }
        for rewrite in change.rewrites[change.sheetName] ?? [] { raws[rewrite.address] = rewrite.old }

        var names: [CellAddress: String] = [:]
        for (address, name) in sheet.grid.cellNames {
            guard let moved = moved(address, on: change.axis, from: change.index + (change.isInsert ? change.count : 0),
                                    by: delta, dropping: dropBand) else { continue }
            names[moved] = name
        }
        for (address, name) in change.removedNames { names[address] = name }

        var formats: [CellAddress: CellFormat] = [:]
        for (address, format) in sheet.formats {
            guard let moved = moved(address, on: change.axis, from: change.index + (change.isInsert ? change.count : 0),
                                    by: delta, dropping: dropBand) else { continue }
            formats[moved] = format
        }
        for (address, format) in change.removedFormats { formats[address] = format }

        var sizes = movedSizes(sizes(of: sheet, on: change.axis),
                               from: change.index + (change.isInsert ? change.count : 0),
                               by: delta, dropping: dropBand)
        for (slot, size) in change.removedSizes { sizes[slot] = size }

        commit(raws: raws, names: names, formats: formats, sizes: sizes,
               on: change.axis, to: sheet)
    }

    // MARK: Mechanics

    private func validate(axis: ReferenceRewriter.Axis, slot: Int, count: Int,
                          in sheet: Sheet, isInsert: Bool) throws {
        guard !sheet.isData else {
            throw EngineError.domainError(
                message: "data sheets have a fixed shape — edit the source data instead")
        }
        let bound = axis == .row ? Spreadsheet.rowCount : Spreadsheet.columnCount
        guard count >= 1, slot >= 0, slot < bound,
              isInsert || slot + count <= bound else {
            throw EngineError.domainError(message: "out of grid bounds")
        }
    }

    private func execute(axis: ReferenceRewriter.Axis, slot: Int, count: Int,
                         isInsert: Bool, in sheet: Sheet) -> StructuralChange {
        let delta = isInsert ? count : -count
        // The rewriter speaks 1-based rows (token space), 0-based columns.
        let rewriteFrom = axis == .row ? slot + 1 : slot

        // 1. Raw rewrites everywhere, recorded at pre-move addresses.
        var allRewrites: [String: [CellRewrite]] = [:]
        for other in sheets where !other.isData {
            var rewrites: [CellRewrite] = []
            for (address, cell) in other.grid.cells {
                guard let rewritten = ReferenceRewriter.shifting(
                    cell.raw, axis: axis, from: rewriteFrom, by: delta,
                    editedSheet: sheet.name, onEditedSheet: other === sheet) else { continue }
                rewrites.append(CellRewrite(address: address, old: cell.raw, new: rewritten))
            }
            if !rewrites.isEmpty { allRewrites[other.name] = rewrites }
            // Other sheets change text in place; the edited sheet's rewrites
            // fold into the rebuilt map below.
            if other !== sheet {
                for rewrite in rewrites { other.grid.setCell(rewrite.new, at: rewrite.address) }
            }
        }
        let ownRewrites = Dictionary(uniqueKeysWithValues:
            (allRewrites[sheet.name] ?? []).map { ($0.address, $0.new) })

        // 2. Content move on the edited sheet.
        let dropBand = isInsert ? nil : slot..<(slot + count)
        var removedCells: [CellAddress: String] = [:]
        var raws: [CellAddress: String] = [:]
        for (address, cell) in sheet.grid.cells {
            let raw = ownRewrites[address] ?? cell.raw
            guard let moved = moved(address, on: axis, from: slot, by: delta, dropping: dropBand) else {
                removedCells[address] = cell.raw // pre-rewrite text: it's leaving the grid
                continue
            }
            raws[moved] = raw
        }

        var removedNames: [CellAddress: String] = [:]
        var names: [CellAddress: String] = [:]
        for (address, name) in sheet.grid.cellNames {
            guard let moved = moved(address, on: axis, from: slot, by: delta, dropping: dropBand) else {
                removedNames[address] = name
                continue
            }
            names[moved] = name
        }

        var removedFormats: [CellAddress: CellFormat] = [:]
        var formats: [CellAddress: CellFormat] = [:]
        for (address, format) in sheet.formats {
            guard let moved = moved(address, on: axis, from: slot, by: delta, dropping: dropBand) else {
                removedFormats[address] = format
                continue
            }
            formats[moved] = format
        }

        var removedSizes: [Int: Double] = [:]
        if let dropBand {
            for (key, size) in sizes(of: sheet, on: axis) where dropBand.contains(key) {
                removedSizes[key] = size
            }
        }
        let movedSizes = movedSizes(sizes(of: sheet, on: axis), from: slot, by: delta, dropping: dropBand)

        commit(raws: raws, names: names, formats: formats, sizes: movedSizes,
               on: axis, to: sheet)

        return StructuralChange(axis: axis, index: slot, count: count, isInsert: isInsert,
                                sheetName: sheet.name, rewrites: allRewrites,
                                removedCells: removedCells, removedNames: removedNames,
                                removedFormats: removedFormats, removedSizes: removedSizes)
    }

    /// Where an address lands after the move; nil = inside the dropped band.
    private func moved(_ address: CellAddress, on axis: ReferenceRewriter.Axis,
                       from slot: Int, by delta: Int,
                       dropping band: Range<Int>?) -> CellAddress? {
        let pos = position(of: address, on: axis)
        if let band, band.contains(pos) { return nil }
        guard pos >= (band?.upperBound ?? slot) else { return address }
        return axis == .row
            ? CellAddress(column: address.column, row: address.row + delta)
            : CellAddress(column: address.column + delta, row: address.row)
    }

    private func position(of address: CellAddress, on axis: ReferenceRewriter.Axis) -> Int {
        axis == .row ? address.row : address.column
    }

    private func sizes(of sheet: Sheet, on axis: ReferenceRewriter.Axis) -> [Int: Double] {
        axis == .row ? sheet.rowHeights : sheet.columnWidths
    }

    private func movedSizes(_ sizes: [Int: Double], from slot: Int, by delta: Int,
                            dropping band: Range<Int>?) -> [Int: Double] {
        var out: [Int: Double] = [:]
        for (key, size) in sizes {
            if let band, band.contains(key) { continue }
            out[key >= (band?.upperBound ?? slot) ? key + delta : key] = size
        }
        return out
    }

    private func commit(raws: [CellAddress: String], names: [CellAddress: String],
                        formats: [CellAddress: CellFormat], sizes: [Int: Double],
                        on axis: ReferenceRewriter.Axis, to sheet: Sheet) {
        sheet.grid.sliderOverrides = [:] // no drag survives a structural edit
        sheet.grid.loadCellNames(names)
        sheet.formats = formats
        if axis == .row { sheet.rowHeights = sizes } else { sheet.columnWidths = sizes }
        sheet.grid.load(raws) // reparse + rebuildDefinitions + recalculate
        recalculate()
    }
}
