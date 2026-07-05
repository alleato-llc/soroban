#if canImport(AppKit)
import AppKit
#endif

/// Whether the Shift key is currently held, driving shift-click range
/// extension on the grid. macOS reads the live modifier flags; on touch
/// iPadOS there is no ambient modifier, so it reports `false` (a
/// hardware-keyboard iPad enhancement can revisit this later).
@MainActor func isShiftKeyDown() -> Bool {
    #if canImport(AppKit)
    return NSEvent.modifierFlags.contains(.shift)
    #else
    return false
    #endif
}
