import SwiftUI

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
                .onChange(of: session.input) {
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
