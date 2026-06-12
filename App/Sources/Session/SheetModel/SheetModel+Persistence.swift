import SorobanEngine
import Foundation

// MARK: Scratch persistence (untitled work, Application Support)
//
// Snapshot + journal (WAL pattern): cell edits append one JSON line to the
// journal (O(1) per edit, written immediately off-main — a crash loses at
// most the in-flight line). Structural changes (layout, sheets, open/new)
// and journal growth trigger a compaction: full snapshot + journal
// truncation. Replay is idempotent, so a crash between those two steps is
// safe. All file work runs on one ordered chain of detached tasks (appends
// and compactions never reorder); the closures are @Sendable, hence
// nonisolated — see the GCD/SIGTRAP note in CLAUDE.md for why not
// DispatchQueue.

extension SheetModel {
    private static let compactionThreshold = 256

    /// Default after-mutation hook: autosave while untitled, then notify
    /// the WorkbookManager (dirty marking).
    func persistAfterChange() {
        if autosaveToScratch {
            saveScratch()
        }
        onContentChange?()
    }

    /// The scratch file is a full Workbook since layout/functions/variables
    /// joined the format; pre-existing scratch files were a bare
    /// ["A:1": raw] dictionary — still readable below.
    func loadScratch() {
        guard let url = Self.storeURL,
              let data = try? Data(contentsOf: url) else { return }

        if var workbook = try? Workbook.decode(data) {
            if let journalURL = Self.journalURL,
               let journalData = try? Data(contentsOf: journalURL) {
                WorkbookJournal.apply(WorkbookJournal.decode(journalData), to: &workbook)
            }
            apply(workbook)
            return
        }
        // Legacy cells-only scratch.
        if let stored = try? JSONDecoder().decode([String: String].self, from: data) {
            apply(Workbook(cells: stored, variables: [:]))
        }
    }

    private func enqueuePersist(_ op: @escaping @Sendable () -> Void) {
        persistChain = Task.detached(priority: .utility) { [previous = persistChain] in
            await previous?.value
            op()
        }
    }

    /// O(1) journal appends for cell edits; compacts when the journal grows.
    func persistCellEdits(_ changes: [(CellAddress, String)], sheetName: String) {
        // Data-sheet edits are already durable (SQLite commits synchronously)
        // and can't replay through the grid journal. Data cells aren't
        // dependency-graph nodes either, so refresh formulas that read them.
        if let sheet = store.sheets.first(where: {
            $0.name.compare(sheetName, options: .caseInsensitive) == .orderedSame
        }), sheet.isData {
            store.recalculate()
            generation += 1
            onContentChange?()
            return
        }
        if autosaveToScratch {
            var data = Data()
            for (address, raw) in changes {
                let entry = WorkbookJournal.Entry(sheet: sheetName, cell: "\(address)", raw: raw)
                if let line = WorkbookJournal.encodeLine(entry) {
                    data.append(line)
                }
            }
            let lines = data // immutable copy for the @Sendable closure
            enqueuePersist { Self.appendJournal(lines) }
            journalEntriesSinceSnapshot += changes.count
            if journalEntriesSinceSnapshot >= Self.compactionThreshold {
                saveScratch()
            }
        }
        onContentChange?()
    }

    /// Full snapshot + journal truncation (compaction), on the ordered chain.
    func saveScratch() {
        journalEntriesSinceSnapshot = 0
        let snapshot = currentWorkbook()
        enqueuePersist { Self.writeSnapshotAndTruncateJournal(snapshot) }
    }

    /// Flush for quit/open paths: wait (bounded) for the chain to drain.
    func flushScratchNow() {
        guard autosaveToScratch else { return }
        journalEntriesSinceSnapshot = 0
        let snapshot = currentWorkbook()
        let done = DispatchSemaphore(value: 0)
        enqueuePersist {
            Self.writeSnapshotAndTruncateJournal(snapshot)
            done.signal()
        }
        _ = done.wait(timeout: .now() + 2)
    }

    private nonisolated static var journalURL: URL? {
        storeURL?.deletingLastPathComponent().appendingPathComponent("scratch-journal.jsonl")
    }

    private nonisolated static func appendJournal(_ data: Data) {
        guard !data.isEmpty, let url = journalURL else { return }
        if let handle = try? FileHandle(forWritingTo: url) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            try? data.write(to: url)
        }
    }

    private nonisolated static func writeSnapshotAndTruncateJournal(_ workbook: Workbook) {
        guard let url = storeURL, let data = try? workbook.encode() else { return }
        try? data.write(to: url, options: .atomic)
        // Snapshot first, truncate second: a crash in between replays old
        // entries onto a snapshot that already contains them — idempotent.
        if let journalURL {
            try? Data().write(to: journalURL, options: .atomic)
        }
    }
}
