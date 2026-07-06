import Foundation
#if canImport(AppKit)
import AppKit
#elseif canImport(UIKit)
import UIKit
#endif

/// Cross-platform pasteboard access — macOS `NSPasteboard`, iPadOS
/// `UIPasteboard`. The grid copies plain TSV (so Excel/Numbers interop works)
/// plus a custom `com.alleato.soroban.cells` payload carrying the copy's origin,
/// so an in-app paste can offset relative references. External apps see only
/// the TSV. This is the one seam that knows which OS pasteboard to speak;
/// callers (`SheetModel+Clipboard`, the log/inspector copy buttons) stay
/// platform-agnostic.
enum Clipboard {
    /// The custom type's identifier, spelled the same on both platforms.
    static let cellsType = "com.alleato.soroban.cells"

    /// A COPY: plain TSV for interop + the custom cells payload (when present).
    static func writeCells(tsv: String, cells: Data?) {
        #if canImport(AppKit)
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(tsv, forType: .string)
        if let cells {
            pasteboard.setData(cells, forType: .init(cellsType))
        }
        #elseif canImport(UIKit)
        var item: [String: Any] = ["public.utf8-plain-text": tsv]
        if let cells { item[cellsType] = cells }
        UIPasteboard.general.setItems([item])
        #endif
    }

    /// A plain-string write (cut, and the log/inspector copy buttons).
    static func write(string: String) {
        #if canImport(AppKit)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
        #elseif canImport(UIKit)
        UIPasteboard.general.string = string
        #endif
    }

    static func readString() -> String? {
        #if canImport(AppKit)
        return NSPasteboard.general.string(forType: .string)
        #elseif canImport(UIKit)
        return UIPasteboard.general.string
        #else
        return nil
        #endif
    }

    /// The custom cells payload, if this pasteboard carries one.
    static func readCellsData() -> Data? {
        #if canImport(AppKit)
        return NSPasteboard.general.data(forType: .init(cellsType))
        #elseif canImport(UIKit)
        return UIPasteboard.general.data(forPasteboardType: cellsType)
        #else
        return nil
        #endif
    }
}
