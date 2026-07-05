import SwiftUI
#if canImport(AppKit)
import AppKit
#elseif canImport(UIKit)
import UIKit
#endif

/// The app's own icon, for the About view. macOS reads the live application
/// icon; iOS loads the asset-catalog `AppIcon` (falling back to an abacus SF
/// Symbol if the lookup fails, since asset-catalog app icons aren't always
/// resolvable by name).
@MainActor func appIconImage() -> Image {
    #if canImport(AppKit)
    return Image(nsImage: NSApp.applicationIconImage)
    #elseif canImport(UIKit)
    if let image = UIImage(named: "AppIcon") {
        return Image(uiImage: image)
    }
    return Image(systemName: "square.grid.3x3.fill")
    #else
    return Image(systemName: "square.grid.3x3.fill")
    #endif
}
