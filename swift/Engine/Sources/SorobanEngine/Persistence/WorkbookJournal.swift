import Anzan
import Foundation

/// The scratch-persistence journal: cell edits append one JSON line each
/// (O(1) per edit) instead of rewriting the whole workbook; a periodic
/// snapshot (the normal Workbook encode) compacts it. `.soroban` interchange
/// files never contain a journal — this exists only for live autosave.
///
/// Replay is order-preserving and idempotent: entries are absolute cell
/// values, so replaying a stale journal over a newer snapshot converges on
/// the same final state (the crash-consistency property compaction relies on).
public enum WorkbookJournal {
    public struct Entry: Codable, Equatable, Sendable {
        /// Sheet name (matched case-insensitively on replay).
        public let sheet: String
        /// "A:1"-form cell key.
        public let cell: String
        /// Raw contents; empty string clears the cell.
        public let raw: String

        public init(sheet: String, cell: String, raw: String) {
            self.sheet = sheet
            self.cell = cell
            self.raw = raw
        }
    }

    /// One compact JSON line per entry (JSONL), newline-terminated.
    public static func encodeLine(_ entry: Entry) -> Data? {
        guard var data = try? JSONEncoder().encode(entry) else { return nil }
        data.append(0x0A)
        return data
    }

    /// Tolerant line-by-line decode — a torn final line (crash mid-append)
    /// or hand-mangled line is skipped, not fatal.
    public static func decode(_ data: Data) -> [Entry] {
        data.split(separator: 0x0A).compactMap { line in
            try? JSONDecoder().decode(Entry.self, from: line)
        }
    }

    /// Replays entries onto a workbook (in order; last writer wins).
    /// Entries for unknown sheets or invalid keys are skipped.
    public static func apply(_ entries: [Entry], to workbook: inout Workbook) {
        for entry in entries {
            guard let index = workbook.sheets.firstIndex(where: {
                $0.name.compare(entry.sheet, options: .caseInsensitive) == .orderedSame
            }), CellAddress(key: entry.cell) != nil else { continue }

            if entry.raw.isEmpty {
                workbook.sheets[index].cells.removeValue(forKey: entry.cell)
            } else {
                workbook.sheets[index].cells[entry.cell] = entry.raw
            }
        }
    }
}
