import SwiftUI
import SorobanEngine
#if canImport(AppKit)
import AppKit // NSWorkspace — the macOS-only "reveal themes folder" button
#endif

struct SettingsView: View {
    @Environment(ThemeManager.self) private var themeManager
    @Environment(CalculatorSession.self) private var session

    /// Monospaced families only: the log's error-caret column padding and the
    /// grid's number alignment rely on fixed-pitch rendering.
    private static let monospacedFamilies: [String] = monospacedFontFamilies()

    var body: some View {
        @Bindable var themeManager = themeManager
        @Bindable var session = session

        Form {
            Picker("Theme:", selection: $themeManager.currentName) {
                ForEach(themeManager.themes) { theme in
                    Text(theme.name).tag(theme.name)
                }
            }

            // Input/display dialect for the log (docs/MODES.md). Cells stay
            // canonical; only the log path reads/echoes glyphs per this mode.
            Picker("Mode:", selection: $session.mode) {
                ForEach(LanguageMode.allCases, id: \.self) { mode in
                    Text(mode.displayName).tag(mode)
                }
            }

            Divider()

            Picker("Font:", selection: $themeManager.fontFamilyOverride) {
                Text("Theme Default").tag(nil as String?)
                ForEach(Self.monospacedFamilies, id: \.self) { family in
                    Text(family).tag(Optional(family))
                }
            }

            LabeledContent("Size:") {
                HStack {
                    Slider(value: fontSize, in: ThemeManager.fontSizeRange, step: 1)
                    Text("\(Int(themeManager.current.fontSize)) pt")
                        .monospacedDigit()
                        .frame(width: 44, alignment: .trailing)
                    Button("Reset") {
                        themeManager.fontSizeOverride = nil
                        themeManager.fontFamilyOverride = nil
                    }
                    .disabled(themeManager.fontSizeOverride == nil
                              && themeManager.fontFamilyOverride == nil)
                }
            }

            LabeledContent("Preview:") {
                Text("pmt(0.05/12, 360, 200000) = -1073.64")
                    .font(themeManager.current.font())
                    .foregroundStyle(themeManager.current.resultText.color)
                    .padding(6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(themeManager.current.windowBackground.color,
                                in: RoundedRectangle(cornerRadius: 4))
            }

            Divider()

            if let directory = ThemeManager.userThemesDirectory {
                LabeledContent("Custom themes:") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Drop .json theme files here (restart to load):")
                        #if os(macOS)
                        // Reveal-in-Finder has no iPad analogue; the path is
                        // shown so the folder is still discoverable there.
                        Button("Open Themes Folder") {
                            NSWorkspace.shared.open(directory)
                        }
                        #else
                        Text(directory.path)
                            .font(.caption2)
                            .textSelection(.enabled)
                        #endif
                    }
                }
            }
        }
        .padding(20)
        .frame(width: 460)
    }

    private var fontSize: Binding<Double> {
        Binding(
            get: { themeManager.current.fontSize },
            set: { themeManager.fontSizeOverride = $0 }
        )
    }
}
