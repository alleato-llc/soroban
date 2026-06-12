import SwiftUI
import SorobanEngine

/// The Function Reference window: searchable categories on the left, entries
/// with live-computed examples on the right. Opens via ⌘/, the Help menu, or
/// the book button in the input bar; autocomplete's ⌘/ jumps straight to the
/// highlighted function.
struct ReferenceView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    @State private var query = ""
    @State private var selectedCategory: String?

    private var theme: Theme { themeManager.current }

    private var categories: [DocCategory] {
        let all = session.referenceDocumentation()
        guard !query.isEmpty else { return all }
        let needle = query.lowercased()
        return all.compactMap { category in
            let hits = category.entries.filter {
                $0.name.lowercased().contains(needle)
                    || $0.signature.lowercased().contains(needle)
                    || $0.summary.lowercased().contains(needle)
            }
            return hits.isEmpty ? nil : DocCategory(title: category.title, entries: hits)
        }
    }

    var body: some View {
        HSplitView {
            sidebar
                .frame(minWidth: 160, maxWidth: 220)
            detail
                .frame(minWidth: 380, maxWidth: .infinity)
        }
        .background(theme.windowBackground.color)
        .frame(minWidth: 600, minHeight: 420)
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            TextField("Search functions", text: $query)
                .textFieldStyle(.roundedBorder)
                .padding(8)
            List(categories, selection: $selectedCategory) { category in
                Text(category.title)
                    .font(theme.font(scale: 0.93))
                    .tag(category.title)
            }
            .scrollContentBackground(.hidden)
        }
        .background(theme.inputBackground.color.opacity(0.4))
    }

    private var detail: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(visibleCategories) { category in
                        Text(category.title)
                            .font(theme.font(scale: 1.1))
                            .fontWeight(.bold)
                            .foregroundStyle(theme.accent.color)
                            .id(category.title)
                        ForEach(category.entries) { entry in
                            DocEntryView(entry: entry, theme: theme)
                                .id(entry.id)
                        }
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: selectedCategory) {
                if let selectedCategory {
                    withAnimation { proxy.scrollTo(selectedCategory, anchor: .top) }
                }
            }
            // Autocomplete's ⌘/ requested a specific function.
            .onChange(of: session.requestedDocEntry) {
                scrollToRequest(proxy)
            }
            .onAppear {
                scrollToRequest(proxy)
            }
        }
    }

    /// When searching, show every matching category; otherwise all of them
    /// (the sidebar scrolls, the search narrows).
    private var visibleCategories: [DocCategory] {
        categories
    }

    private func scrollToRequest(_ proxy: ScrollViewProxy) {
        // Explicit request wins; otherwise ⌘/ with autocomplete open means
        // "docs for the highlighted candidate".
        let highlighted = session.suggestions.indices.contains(session.selectedSuggestion)
            ? session.suggestions[session.selectedSuggestion].name : nil
        guard let requested = session.requestedDocEntry ?? highlighted else { return }
        session.requestedDocEntry = nil
        query = ""
        DispatchQueue.main.async {
            withAnimation { proxy.scrollTo(requested, anchor: .top) }
        }
    }
}

private struct DocEntryView: View {
    let entry: FunctionDoc
    let theme: Theme

    @Environment(CalculatorSession.self) private var session
    @Environment(\.dismissWindow) private var dismissWindow

    /// Scratch calculator for example results (sheet-dependent examples show
    /// no result here — they still copy/insert fine).
    private static let previewCalculator = Calculator()

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(entry.signature)
                .font(theme.font())
                .fontWeight(.semibold)
                .foregroundStyle(theme.resultText.color)
                .textSelection(.enabled)
            Text(entry.summary)
                .font(.system(size: theme.fontSize * 0.93))
                .foregroundStyle(theme.expressionText.color)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)

            ForEach(entry.examples, id: \.self) { example in
                exampleRow(example)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.inputBackground.color.opacity(0.5),
                    in: RoundedRectangle(cornerRadius: 6))
    }

    private func exampleRow(_ example: String) -> some View {
        Button {
            // Click an example → it lands in the log input, ready to run.
            session.activeView = .log
            session.recall(expression: example)
            dismissWindow(id: "reference")
        } label: {
            HStack(spacing: 6) {
                Text(example)
                    .font(theme.font(scale: 0.93))
                    .foregroundStyle(theme.accent.color)
                if case .success(let outcome) = Self.previewCalculator.evaluate(example) {
                    Text("→ \(outcome.description)")
                        .font(theme.font(scale: 0.93))
                        .foregroundStyle(theme.secondaryText.color)
                }
            }
        }
        .buttonStyle(.plain)
        .help("Click to try it in the calculator")
    }
}
