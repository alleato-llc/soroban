import SwiftUI
import SorobanEngine

extension LanguageMode {
    /// Human label for the Settings picker and the input-bar affordance.
    var displayName: String {
        switch self {
        case .normal: return "Normal"
        case .programmer: return "Programmer"
        case .scientific: return "Scientific"
        }
    }

    /// Compact SF Symbol for the input-bar status affordance. Scientific has
    /// no SF Symbol glyph — its badge is the literal π (see `badgeText`).
    var symbol: String? {
        switch self {
        case .normal: return "number"
        case .programmer: return "chevron.left.forwardslash.chevron.right"
        case .scientific: return nil
        }
    }

    /// Literal-text badge for modes without an SF Symbol (`π` for Scientific).
    var badgeText: String? {
        self == .scientific ? "π" : nil
    }
}

/// The single-line expression input pinned to the bottom of the window.
struct InputBarView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager
    @Environment(\.openWindow) private var openWindow
    @FocusState private var focused: Bool

    private var theme: Theme { themeManager.current }

    var body: some View {
        @Bindable var session = session

        HStack(spacing: 8) {
            Text(">")
                .font(theme.font())
                .foregroundStyle(theme.accent.color)

            TextField("Expression", text: $session.input)
                .textFieldStyle(.plain)
                .font(theme.font())
                .foregroundStyle(theme.resultText.color)
                .focused($focused)
                .onSubmit { session.submit() }
                .onChange(of: session.input) { old, new in
                    // A leading operator on an empty line prepends `ans`
                    // (SpeedCrunch-style); that rewrite handles its own refresh.
                    if session.applyAnsPrefixIfNeeded(old: old, new: new) { return }
                    session.refreshSuggestions()
                }
                // ↑/↓ navigate the suggestion list when it's open,
                // input history otherwise.
                .onKeyPress(.upArrow) {
                    if session.suggestions.isEmpty {
                        session.recallPrevious()
                    } else {
                        session.moveSuggestion(-1)
                    }
                    return .handled
                }
                .onKeyPress(.downArrow) {
                    if session.suggestions.isEmpty {
                        session.recallNext()
                    } else {
                        session.moveSuggestion(1)
                    }
                    return .handled
                }
                .onKeyPress(.tab) {
                    guard !session.suggestions.isEmpty else { return .ignored }
                    session.acceptSuggestion()
                    return .handled
                }
                // Esc closes suggestions first; a second press clears the line.
                .onKeyPress(.escape) {
                    if session.suggestions.isEmpty {
                        session.input = ""
                    } else {
                        session.dismissSuggestions()
                    }
                    return .handled
                }

            // Binary bit-editor toggle — only in Programmer mode (where the
            // editor lives); accented when shown. Mirrors ⌥⌘B / the ✕ on the panel.
            if session.mode == .programmer {
                Button { session.binaryEditorShown.toggle() } label: {
                    Text("01")
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(session.binaryEditorShown
                                         ? theme.accent.color : theme.secondaryText.color)
                }
                .buttonStyle(.plain)
                .help("Show/hide the binary editor (⌥⌘B)")
            }

            // Input dialect (docs/MODES.md): a compact status affordance — the
            // icon shows the active mode, the menu switches it. Accented when not
            // Normal so a non-default dialect is visible at a glance.
            Menu {
                ForEach(LanguageMode.allCases, id: \.self) { candidate in
                    Button {
                        session.mode = candidate
                    } label: {
                        if session.mode == candidate {
                            Label(candidate.displayName, systemImage: "checkmark")
                        } else {
                            Text(candidate.displayName)
                        }
                    }
                }
            } label: {
                // `#` Normal · `π` Scientific · `</>` Programmer — π is literal
                // text (no SF Symbol exists for it), styled to match the icons.
                if let badge = session.mode.badgeText {
                    Text(badge)
                        .font(.system(size: 13, weight: .medium, design: .serif))
                        .foregroundStyle(session.mode == .normal
                                         ? theme.secondaryText.color : theme.accent.color)
                } else {
                    Image(systemName: session.mode.symbol ?? "number")
                        .foregroundStyle(session.mode == .normal
                                         ? theme.secondaryText.color : theme.accent.color)
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Input dialect: \(session.mode.displayName)")

            // Function reference (also ⌘/; with autocomplete open it jumps
            // to the highlighted function).
            Button {
                openWindow(id: "reference")
            } label: {
                Image(systemName: "book")
                    .foregroundStyle(theme.secondaryText.color)
            }
            .buttonStyle(.plain)
            .help("Function Reference (⌘/)")

            ViewToggleButton(floating: false)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(theme.inputBackground.color)
        .onAppear { focused = true }
        // Keep focus on the input when the user clicks around the log.
        .onTapGesture { focused = true }
    }
}

/// Switches between the calculation log and the grid (also ⌘\).
/// Inline at the right edge of the input bar in log mode; floating over the
/// bottom-right corner of the grid (the same screen spot) in grid mode.
struct ViewToggleButton: View {
    let floating: Bool

    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    private var theme: Theme { themeManager.current }

    var body: some View {
        Button {
            session.toggleView()
        } label: {
            Image(systemName: session.activeView == .log
                  ? "tablecells" : "list.bullet.rectangle")
                .foregroundStyle(theme.accent.color)
                .padding(floating ? 8 : 0)
                .background {
                    if floating {
                        Circle()
                            .fill(theme.inputBackground.color)
                            .shadow(radius: 2, y: 1)
                    }
                }
        }
        .buttonStyle(.plain)
        .help(session.activeView == .log ? "Show grid (⌘\\)" : "Show log (⌘\\)")
    }
}
