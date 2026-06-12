import SwiftUI

struct ContentView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    var body: some View {
        HStack(spacing: 0) {
            main
            if session.inspectorVisible {
                Divider()
                InspectorView()
            }
        }
        .background(themeManager.current.windowBackground.color)
        .frame(minWidth: 420, minHeight: 320)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    session.inspectorVisible.toggle()
                } label: {
                    Image(systemName: "sidebar.trailing")
                }
                .help("Toggle the Environment inspector (⌥⌘0)")
            }
        }
    }

    private var main: some View {
        VStack(spacing: 0) {
            switch session.activeView {
            case .log:
                HistoryLogView()
                if !session.suggestions.isEmpty {
                    Divider()
                    SuggestionsView()
                }
                Divider()
                InputBarView()

            case .sheet:
                // No input bar in grid mode — its results would land in the
                // hidden log. The worksheet strip owns the bottom edge (and
                // hosts the view toggle).
                SpreadsheetView()
                Divider()
                SheetTabBar()
            }
        }
        .frame(maxWidth: .infinity)
    }
}
