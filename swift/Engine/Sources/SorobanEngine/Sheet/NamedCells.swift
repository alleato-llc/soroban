import Anzan
import Foundation

/// Reference-rewriting for named cells: renaming a name updates every
/// formula that uses it; deleting offers to inline the cell address instead.
/// Token-precise (the lexer's character ranges), so spacing and `# comments`
/// survive — the same technique control expressions use.
public enum NamedCells {
    /// Rewrites every reference to `oldName` in one raw cell text.
    ///
    /// Scoping rules mirror resolution: an UNQUALIFIED `'Old'` only refers to
    /// this name when the formula lives on the name's own sheet
    /// (`onOwningSheet`); a QUALIFIED `Sheet!'Old'` refers to it from
    /// anywhere when the qualifier is the owning sheet. References to
    /// same-spelled names on OTHER sheets are left alone.
    ///
    /// `replacement` is spliced over the quoted token only — pass `'New
    /// Name'` (quoted) for renames or `B:7` for address inlining; qualifiers
    /// stay put either way. Returns nil when nothing matched.
    public static func rewriting(_ raw: String, oldName: String, owningSheet: String?,
                                 onOwningSheet: Bool, replacement: String) -> String? {
        guard let tokens = try? Lexer.tokenize(raw) else { return nil }

        // Collect matching quotedName token ranges, back to front.
        var ranges: [Range<Int>] = []
        for (index, token) in tokens.enumerated() {
            guard case .quotedName(let name) = token.kind,
                  name.compare(oldName, options: .caseInsensitive) == .orderedSame else { continue }
            // A quoted name FOLLOWED by ! is a sheet qualifier, not a name.
            if index + 1 < tokens.count, case .bang = tokens[index + 1].kind { continue }

            // Qualified? Look back for `sheet` `!`.
            var qualifier: String?
            if index >= 2, case .bang = tokens[index - 1].kind {
                switch tokens[index - 2].kind {
                case .identifier(let sheet), .quotedName(let sheet):
                    qualifier = sheet
                default:
                    break
                }
            }

            let matches: Bool
            if let qualifier {
                matches = owningSheet.map {
                    qualifier.compare($0, options: .caseInsensitive) == .orderedSame
                } ?? false
            } else {
                matches = onOwningSheet
            }
            if matches {
                ranges.append(token.range)
            }
        }
        guard !ranges.isEmpty else { return nil }

        var characters = Array(raw)
        for range in ranges.reversed() {
            characters.replaceSubrange(range, with: replacement)
        }
        return String(characters)
    }
}
