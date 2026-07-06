#if canImport(AppKit)
import AppKit
#elseif canImport(UIKit)
import UIKit
#endif

/// The installed monospaced font families, sorted. The log's error-caret
/// column padding and the grid's number alignment rely on fixed-pitch
/// rendering, so only monospaced families are offered. macOS enumerates via
/// `NSFontManager`; iPadOS via `UIFont` + the monospace descriptor trait.
func monospacedFontFamilies() -> [String] {
    #if canImport(AppKit)
    return NSFontManager.shared.availableFontFamilies
        .filter { NSFont(name: $0, size: 12)?.isFixedPitch == true }
        .sorted()
    #elseif canImport(UIKit)
    return UIFont.familyNames
        .filter { family in
            guard let name = UIFont.fontNames(forFamilyName: family).first,
                  let font = UIFont(name: name, size: 12) else { return false }
            return font.fontDescriptor.symbolicTraits.contains(.traitMonoSpace)
        }
        .sorted()
    #else
    return []
    #endif
}
