import SwiftUI

/// Scrolling log of calculations, newest at the bottom (SpeedCrunch style).
struct HistoryLogView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    private var theme: Theme { themeManager.current }

    var body: some View {
        // The log model (LogStore) isn't @Observable; subscribe to its changes
        // through the session's bridge (the grid's `generation` pattern).
        let _ = session.logGeneration
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 6) {
                    if session.entries.isEmpty {
                        emptyState
                    }
                    ForEach(session.entries) { entry in
                        EntryView(entry: entry, theme: theme)
                            .id(entry.id)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: session.logGeneration) {
                guard let last = session.entries.last else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Type an expression below — or double-click one:")
            // A random trio from the welcome pool, chosen once at launch.
            // Double-click drops the example into the input bar.
            ForEach(session.welcomeExamples, id: \.self) { example in
                Text("  \(example)")
                    .contentShape(Rectangle())
                    .onHover { $0 ? NSCursor.pointingHand.push() : NSCursor.pop() }
                    .onTapGesture(count: 2) { session.useExample(example) }
            }
        }
        .font(theme.font())
        .foregroundStyle(theme.secondaryText.color)
        .padding(.top, 8)
    }
}

private struct EntryView: View {
    let entry: HistoryEntry
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            if case .comment(let text) = entry.outcome {
                // A standalone note — dim italic, no separate expression echo
                // (the expression IS the comment). Recall/copy via menu.
                Text("# \(text)")
                    .font(theme.font(scale: 0.93).italic())
                    .foregroundStyle(theme.secondaryText.color)
                    .textSelection(.enabled)
                    .contextMenu {
                        Button("Edit Note") { session.recall(expression: entry.expression) }
                        Button("Copy Note") { copyToPasteboard(entry.expression) }
                    }
            } else {
                expressionAndResult
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private var expressionAndResult: some View {
        // Expression line — selectable; recall via context menu.
        Text(entry.expression)
            .font(theme.font(scale: 0.93))
            .foregroundStyle(theme.expressionText.color)
            .textSelection(.enabled)
            .contextMenu {
                Button("Edit Expression") {
                    session.recall(expression: entry.expression)
                }
                Button("Copy Expression") {
                    copyToPasteboard(entry.expression)
                }
            }

            switch entry.outcome {
            case .value(let text):
                // The annotation (hex echo) and a trailing # note both render
                // dimmer and are display-only: Insert/Copy use the bare value.
                (Text("= \(text)").fontWeight(.medium)
                    .foregroundColor(theme.resultText.color)
                    + Text(entry.annotation.map { "  \($0)" } ?? "")
                    .foregroundColor(theme.secondaryText.color)
                    + Text(entry.note.map { "  # \($0)" } ?? "")
                    .foregroundColor(theme.secondaryText.color))
                    .font(theme.font())
                    .textSelection(.enabled)
                    .contextMenu {
                        Button("Insert Result") {
                            session.insert(value: text)
                        }
                        Button("Copy Result") {
                            copyToPasteboard(text)
                        }
                    }

            case .comment:
                EmptyView() // handled above (never reached)

            case .info(let text):
                // man()/help() output: a doc block, not a result.
                Text(text)
                    .font(theme.font(scale: 0.93))
                    .foregroundStyle(theme.resultText.color)
                    .textSelection(.enabled)
                    .padding(8)
                    .background(theme.inputBackground.color.opacity(0.5),
                                in: RoundedRectangle(cornerRadius: 6))

            case .error(let message, let position):
                VStack(alignment: .leading, spacing: 0) {
                    if let position, position < entry.expression.count {
                        // Caret under the offending column (monospaced font).
                        Text(String(repeating: " ", count: position) + "^")
                            .font(theme.font(scale: 0.93))
                            .foregroundStyle(theme.errorText.color)
                    }
                    Text(message)
                        .font(theme.font(scale: 0.93))
                        .foregroundStyle(theme.errorText.color)
                        .textSelection(.enabled)
                }
            }
    }

    private func copyToPasteboard(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
