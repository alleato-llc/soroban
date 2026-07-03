import SwiftUI
import AppKit

/// Custom About window (replaces the stock About panel via the `.appInfo`
/// command group): tells the story of the name and states what the app is
/// for. Theme-aware like every other view — colors come from ThemeManager.
struct AboutView: View {
    @Environment(ThemeManager.self) private var themeManager

    private var version: String {
        let short = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "—"
        let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "—"
        return "Version \(short) (\(build))"
    }

    var body: some View {
        let theme = themeManager.current

        VStack(spacing: 14) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)

            VStack(spacing: 4) {
                Text("Soroban・算盤")
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(theme.resultText.color)
                Text(version)
                    .font(.caption)
                    .foregroundStyle(theme.secondaryText.color)
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("A soroban (算盤) is the Japanese abacus — a centuries-old "
                     + "instrument of exact arithmetic. Every bead is a definite "
                     + "digit; there is no rounding and no drift. The name is a "
                     + "promise kept by the engine: arithmetic here is exact, "
                     + "not approximately right.")
                Text("Soroban is a powerful, exact, modern calculator — and a "
                     + "spreadsheet for when you mean business: formulas, live "
                     + "controls, named cells, and data sheets, all on the same "
                     + "50-digit decimal engine.")
            }
            .font(.callout)
            .foregroundStyle(theme.expressionText.color)
            .fixedSize(horizontal: false, vertical: true)

            Text("Free & open source")
                .font(.caption)
                .foregroundStyle(theme.secondaryText.color)
        }
        .padding(28)
        .frame(width: 440)
        .background(theme.windowBackground.color)
    }
}
