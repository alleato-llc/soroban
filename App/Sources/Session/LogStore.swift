import SorobanEngine
import Foundation

/// The calculation log — a UI-free model owning the running tape and its
/// persistence, decoupled from the SwiftUI session (mirrors how `SheetStore`
/// owns the grid for the engine). Because it's a plain, non-`@MainActor` class,
/// it conforms to `LogSource` **directly**, so the `History` reflection API
/// reads it with no adapter and no `MainActor.assumeIsolated` bridging.
///
/// Single-threaded discipline: the tape is appended (on submit) and read (by the
/// engine during evaluation) only on the main actor, so `@unchecked Sendable`
/// rests on the same basis as the reflection handles. The SwiftUI session
/// observes changes through its own `logGeneration` bridge (the grid's pattern),
/// since this model isn't `@Observable`.
final class LogStore: LogSource, @unchecked Sendable {
    /// The running tape, oldest → newest (capped, reloaded on launch).
    private(set) var entries: [HistoryEntry] = []

    /// Whether to persist to the on-disk log file. False in tests, so they never
    /// touch the real `…/Application Support/Soroban/log.json` — the same
    /// discipline `SheetModel.autosaveToScratch` enforces for the scratch file.
    var persists: Bool

    private let limit = 500

    init(persists: Bool = true) {
        self.persists = persists
        if persists { load() }
    }

    func append(_ entry: HistoryEntry) {
        entries.append(entry)
        save()
    }

    func clear() {
        entries.removeAll()
        save()
    }

    // MARK: History reflection (the LogSource seam)

    var logCount: Int { entries.count }

    func logRecord(at index: Int) -> LogRecord? {
        entries.indices.contains(index) ? LogRecord(entries[index]) : nil
    }

    // MARK: Persistence (snapshot the whole small tape — cheap, simple)

    private static var url: URL? {
        guard let dir = try? FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask,
            appropriateFor: nil, create: true) else { return nil }
        let folder = dir.appendingPathComponent("Soroban", isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder.appendingPathComponent("log.json")
    }

    private func load() {
        guard let url = Self.url,
              let data = try? Data(contentsOf: url),
              let saved = try? JSONDecoder().decode([HistoryEntry].self, from: data) else { return }
        entries = saved
    }

    private func save() {
        if entries.count > limit { entries.removeFirst(entries.count - limit) }
        guard persists, let url = Self.url,
              let data = try? JSONEncoder().encode(entries) else { return }
        try? data.write(to: url, options: .atomic)
    }
}

/// Maps a logged entry to the host-neutral `LogRecord` the engine's History
/// reflection reads. The result text re-parses to a typed value (number/string,
/// losslessly); nil for structured/non-value lines → callers use `.text`.
extension LogRecord {
    init(_ entry: HistoryEntry) {
        let text: String
        let value: Value?
        let isError: Bool
        let isComment: Bool
        let isInfo: Bool
        switch entry.outcome {
        case .value(let rendered):
            text = rendered; value = Value(parsing: rendered)
            isError = false; isComment = false; isInfo = false
        case .info(let rendered):
            text = rendered; value = nil
            isError = false; isComment = false; isInfo = true
        case .error(let message, _):
            text = message; value = nil
            isError = true; isComment = false; isInfo = false
        case .comment(let comment):
            text = comment; value = nil
            isError = false; isComment = true; isInfo = false
        case .mode(let label):
            // Display-only marker (like info): visible in History, never a value.
            text = label; value = nil
            isError = false; isComment = false; isInfo = true
        }
        self.init(input: entry.expression, text: text, value: value,
                  isError: isError, isComment: isComment, isInfo: isInfo, note: entry.note ?? "")
    }
}
