import SwiftUI
import SorobanEngine

/// Autocomplete candidates, shown directly above the input bar while typing.
/// Tab accepts, ↑/↓ move, Esc dismisses (handled by InputBarView); clicking
/// a row accepts it too.
struct SuggestionsView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    private var theme: Theme { themeManager.current }
    private static let visibleLimit = 6

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(session.suggestions.prefix(Self.visibleLimit).enumerated()),
                    id: \.element) { index, completion in
                row(completion, isSelected: index == session.selectedSuggestion)
                    .onTapGesture { session.acceptSuggestion(index) }
            }
            if session.suggestions.count > Self.visibleLimit {
                Text("… \(session.suggestions.count - Self.visibleLimit) more")
                    .font(theme.font(scale: 0.8))
                    .foregroundStyle(theme.secondaryText.color)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 2)
            }
            // Inline docs for the highlighted candidate — ⌘/ opens the full entry.
            if let highlighted = highlightedDoc {
                Divider()
                HStack(spacing: 6) {
                    Text(highlighted.signature)
                        .font(theme.font(scale: 0.85))
                        .foregroundStyle(theme.resultText.color)
                    Text("— \(highlighted.summary)")
                        .font(.system(size: theme.fontSize * 0.8))
                        .foregroundStyle(theme.secondaryText.color)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text("⌘/ docs")
                        .font(theme.font(scale: 0.75))
                        .foregroundStyle(theme.secondaryText.color)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 3)
            }
        }
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.inputBackground.color.opacity(0.6))
    }

    private func row(_ completion: Completion, isSelected: Bool) -> some View {
        HStack(spacing: 8) {
            Text(badge(for: completion.kind))
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.secondaryText.color)
                .frame(width: 36, alignment: .trailing)
            Text(completion.name + (completion.kind == .function ? "(" : ""))
                .font(theme.font(scale: 0.93))
                .foregroundStyle(isSelected ? theme.accent.color : theme.resultText.color)
            if isSelected {
                Text("⇥")
                    .font(theme.font(scale: 0.8))
                    .foregroundStyle(theme.secondaryText.color)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .background(isSelected ? theme.accent.color.opacity(0.15) : .clear)
    }

    private var highlightedDoc: FunctionDoc? {
        guard session.suggestions.indices.contains(session.selectedSuggestion) else { return nil }
        return session.documentation(for: session.suggestions[session.selectedSuggestion].name)
    }

    private func badge(for kind: Completion.Kind) -> String {
        switch kind {
        case .function: "ƒ"
        case .variable: "var"
        case .constant: "const"
        }
    }
}
