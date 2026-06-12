import SwiftUI
import Observation

/// Loads built-in themes from the app bundle and user themes from
/// `~/Library/Application Support/Soroban/Themes/*.json`; remembers the choice.
@Observable
final class ThemeManager {
    private(set) var themes: [Theme] = []

    /// The picked theme, without overrides. Views should use `current`.
    private var selection: Theme {
        didSet { UserDefaults.standard.set(selection.name, forKey: Self.defaultsKey) }
    }

    /// App-level font preference (Settings): personal/accessibility choice,
    /// deliberately independent of the color theme — switching themes keeps
    /// your font. nil → whatever the theme (or system monospaced) provides.
    var fontFamilyOverride: String? {
        didSet { UserDefaults.standard.set(fontFamilyOverride, forKey: Self.fontKey) }
    }

    var fontSizeOverride: Double? {
        didSet {
            if let fontSizeOverride {
                UserDefaults.standard.set(fontSizeOverride, forKey: Self.sizeKey)
            } else {
                UserDefaults.standard.removeObject(forKey: Self.sizeKey)
            }
        }
    }

    /// What every view renders with: the picked theme + font overrides.
    var current: Theme {
        var theme = selection
        if let fontFamilyOverride { theme.fontName = fontFamilyOverride }
        if let fontSizeOverride { theme.fontSize = fontSizeOverride }
        return theme
    }

    /// Picker binding (by name — `current` carries overrides, so binding the
    /// Theme value itself would never match the listed options).
    var currentName: String {
        get { selection.name }
        set {
            if let theme = themes.first(where: { $0.name == newValue }) {
                selection = theme
            }
        }
    }

    private static let defaultsKey = "selectedTheme"
    private static let fontKey = "fontFamilyOverride"
    private static let sizeKey = "fontSizeOverride"

    init() {
        let loaded = Self.loadThemes()
        themes = loaded
        let saved = UserDefaults.standard.string(forKey: Self.defaultsKey)
        selection = loaded.first { $0.name == saved } ?? loaded[0]
        fontFamilyOverride = UserDefaults.standard.string(forKey: Self.fontKey)
        fontSizeOverride = UserDefaults.standard.object(forKey: Self.sizeKey) as? Double
    }

    /// User theme directory, created on first access so users can find it.
    static var userThemesDirectory: URL? {
        guard let support = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first else { return nil }
        let directory = support.appendingPathComponent("Soroban/Themes", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private static func loadThemes() -> [Theme] {
        var themes: [Theme] = []
        let decoder = JSONDecoder()

        // XcodeGen copies App/Resources contents flat into the bundle root,
        // so check both there and a Themes/ subdirectory. Non-theme JSON is
        // harmless — anything that doesn't decode as a Theme is skipped.
        var builtinURLs = Bundle.main.urls(forResourcesWithExtension: "json",
                                           subdirectory: "Themes") ?? []
        if builtinURLs.isEmpty {
            builtinURLs = Bundle.main.urls(forResourcesWithExtension: "json",
                                           subdirectory: nil) ?? []
        }
        let userURLs = userThemesDirectory.flatMap {
            try? FileManager.default.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil)
                .filter { $0.pathExtension == "json" }
        } ?? []

        for url in builtinURLs + userURLs {
            guard let data = try? Data(contentsOf: url),
                  let theme = try? decoder.decode(Theme.self, from: data),
                  !themes.contains(where: { $0.name == theme.name }) else { continue }
            themes.append(theme)
        }
        themes.sort { $0.name < $1.name }

        if themes.isEmpty {
            themes = [.fallback] // bundle missing/corrupt — never crash over styling
        }
        return themes
    }
}

extension Theme {
    /// Compiled-in safety net, also used by previews.
    static let fallback = Theme(
        name: "Soroban Dark",
        windowBackground: HexColor(hex: "#1E1E28")!,
        inputBackground: HexColor(hex: "#2A2A38")!,
        expressionText: HexColor(hex: "#9DA5B4")!,
        resultText: HexColor(hex: "#E6E6F0")!,
        errorText: HexColor(hex: "#FF6B6B")!,
        secondaryText: HexColor(hex: "#6C7086")!,
        accent: HexColor(hex: "#7AA2F7")!,
        fontName: nil,
        fontSize: 14
    )
}
