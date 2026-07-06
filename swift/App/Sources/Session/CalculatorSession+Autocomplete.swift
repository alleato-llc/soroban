import SorobanEngine
import Foundation

// Autocomplete (suggestion list state + acceptance), the SpeedCrunch ans-prefix
// continuation, and ↑/↓ input-history recall. The stored cursors/suggestion
// state live on the class body in `CalculatorSession.swift`.

extension CalculatorSession {
    // MARK: Autocomplete

    /// SpeedCrunch-style continuation: when the field was empty and the user
    /// just typed a leading binary operator, prepend `ans` so `+5` becomes
    /// `ans+5`. Returns true if it rewrote (the caller then skips the normal
    /// suggestion refresh — the rewrite re-enters onChange and handles it).
    /// Only fires on genuine typing from an empty field, never on a programmatic
    /// set (history recall / accept set `suppressNextSuggestionRefresh` first).
    func applyAnsPrefixIfNeeded(old: String, new: String) -> Bool {
        guard !suppressNextSuggestionRefresh,
              old.allSatisfy({ $0 == " " }),
              let rewritten = Calculator.ansPrefixed(new, mode: mode), rewritten != new
        else { return false }
        suppressNextSuggestionRefresh = true // the re-entrant onChange won't pop suggestions
        input = rewritten
        return true
    }

    /// Recomputes suggestions for the identifier being typed at the caret
    /// (end of input). Called from the input field's onChange.
    func refreshSuggestions() {
        if suppressNextSuggestionRefresh {
            suppressNextSuggestionRefresh = false
            dismissSuggestions()
            return
        }
        let word = Calculator.trailingIdentifier(of: input)
        suggestions = word.isEmpty ? [] : calculator.completions(forPrefix: word)
        selectedSuggestion = 0
    }

    func dismissSuggestions() {
        suggestions = []
        selectedSuggestion = 0
    }

    /// ↑/↓ within the open suggestion list (wraps around).
    func moveSuggestion(_ delta: Int) {
        guard !suggestions.isEmpty else { return }
        selectedSuggestion = (selectedSuggestion + delta + suggestions.count) % suggestions.count
    }

    /// Replaces the typed prefix with the chosen candidate; functions get
    /// their opening parenthesis for free.
    func acceptSuggestion(_ index: Int? = nil) {
        let chosen = index ?? selectedSuggestion
        guard suggestions.indices.contains(chosen) else { return }
        let completion = suggestions[chosen]

        suppressNextSuggestionRefresh = true
        input.removeLast(Calculator.trailingIdentifier(of: input).count)
        input += completion.name
        if completion.kind == .function {
            input += "("
        }
    }

    /// ↑ — step back through past inputs.
    func recallPrevious() {
        guard !inputHistory.isEmpty else { return }
        if historyCursor == nil {
            draft = input
            historyCursor = inputHistory.count
        }
        guard let cursor = historyCursor, cursor > 0 else { return }
        historyCursor = cursor - 1
        suppressNextSuggestionRefresh = true
        input = inputHistory[cursor - 1]
    }

    /// ↓ — step forward, ending at the stashed draft.
    func recallNext() {
        guard let cursor = historyCursor else { return }
        suppressNextSuggestionRefresh = true
        if cursor >= inputHistory.count - 1 {
            historyCursor = nil
            input = draft
        } else {
            historyCursor = cursor + 1
            input = inputHistory[cursor + 1]
        }
    }

    /// Clicking a log line: expressions replace the input, results append.
    func recall(expression: String) {
        suppressNextSuggestionRefresh = true
        input = expression
        historyCursor = nil
    }

    func insert(value: String) {
        suppressNextSuggestionRefresh = true
        input += input.isEmpty || input.hasSuffix(" ") ? value : " \(value)"
        historyCursor = nil
    }
}
