import Anzan

/// Token-precise rewriting of cell references inside raw cell text — the
/// engine behind structural edits (insert/delete rows & columns), fill/paste
/// reference adjustment, and sheet-rename rewriting. Same technique as
/// `NamedCells.rewriting`: lex, collect token ranges, splice back-to-front so
/// spacing and `# comments` survive. Every function returns nil when nothing
/// matched (so callers skip untouched cells).
///
/// Two token shapes are deliberately IGNORED everywhere: compact map keys
/// (`{b:1}` lexes as a cell-reference token but the parser decomposes it into
/// key + value — detected as "directly after `{` or `,` inside braces") and
/// multi-letter columns (`age:36` — named-argument sugar; real columns are
/// single letters A–Z).
public enum ReferenceRewriter {
    public enum Axis: Sendable { case row, column }

    // MARK: Structural shifts (insert/delete rows & columns)

    /// Rewrites references for an insert (`delta > 0`) or delete
    /// (`delta < 0`, the slots `index ..< index - delta` are removed) on
    /// `editedSheet`. Positions are 1-based for rows, 0-based for columns —
    /// the same spaces `CellAddress` uses.
    ///
    /// Scope mirrors resolution: a QUALIFIED ref matches when its qualifier
    /// names the edited sheet (from any sheet); an UNQUALIFIED ref matches
    /// only when the formula lives on the edited sheet (`onEditedSheet`).
    /// Pins shift like everything else — `$` is copy-time semantics, not an
    /// anchor against structural edits (Excel agrees).
    ///
    /// Deleted references become `refError()` (qualifier included in the
    /// splice); range corners clamp inward instead, and a fully-deleted
    /// range becomes `refError()` whole.
    public static func shifting(_ raw: String, axis: Axis, from index: Int,
                                by delta: Int, editedSheet: String,
                                onEditedSheet: Bool) -> String? {
        guard delta != 0, let sites = scan(raw) else { return nil }

        func matches(_ site: Site) -> Bool {
            if let qualifier = site.qualifier {
                return qualifier.compare(editedSheet, options: .caseInsensitive) == .orderedSame
            }
            return onEditedSheet
        }

        let deadCount = delta < 0 ? -delta : 0
        // nil = deleted; otherwise the shifted position.
        func shifted(_ position: Int) -> Int? {
            if position >= index + deadCount { return position + delta }
            if position >= index { return delta > 0 ? position + delta : nil }
            return position
        }

        var splices: [(Range<Int>, String)] = []
        for unit in units(of: sites) {
            switch unit {
            case .single(let site):
                guard matches(site) else { continue }
                guard let position = shifted(site.position(on: axis)) else {
                    splices.append((site.spliceStart..<site.range.upperBound, "refError()"))
                    continue
                }
                if position != site.position(on: axis) {
                    splices.append((site.range, site.text(with: position, on: axis)))
                }

            case .pair(let first, let second):
                guard matches(first) else { continue } // the qualifier rides corner one
                let (lo, hi) = first.position(on: axis) <= second.position(on: axis)
                    ? (first, second) : (second, first)
                // Clamp dead corners inward: the low corner lands on the
                // first survivor after the hole, the high on the last before.
                var loPos = lo.position(on: axis)
                var hiPos = hi.position(on: axis)
                if deadCount > 0, (index..<index + deadCount).contains(loPos) {
                    loPos = index + deadCount
                }
                if deadCount > 0, (index..<index + deadCount).contains(hiPos) {
                    hiPos = index - 1
                }
                guard loPos <= hiPos,
                      let newLo = shifted(loPos), let newHi = shifted(hiPos) else {
                    splices.append((first.spliceStart..<second.range.upperBound, "refError()"))
                    continue
                }
                if newLo != lo.position(on: axis) {
                    splices.append((lo.range, lo.text(with: newLo, on: axis)))
                }
                if newHi != hi.position(on: axis) {
                    splices.append((hi.range, hi.text(with: newHi, on: axis)))
                }
            }
        }
        return apply(splices, to: raw)
    }

    // MARK: Relative adjustment (fill / paste)

    /// Shifts every reference's unpinned axes by the copy offset — the heart
    /// of fill down/right and in-app paste. Qualified refs adjust too
    /// (Excel-style); named-cell references never do (names are the
    /// absolute-by-meaning reference). A ref pushed off the grid becomes
    /// `refError()`; a range with a dead corner dies whole.
    public static func adjustingRelative(_ raw: String, byRows: Int,
                                         byColumns: Int) -> String? {
        guard byRows != 0 || byColumns != 0, let sites = scan(raw) else { return nil }

        func adjusted(_ site: Site) -> (row: Int, column: Int)? {
            let row = site.pinRow ? site.row : site.row + byRows
            let column = site.pinColumn ? site.columnIndex : site.columnIndex + byColumns
            guard (1...Spreadsheet.rowCount).contains(row),
                  (0..<Spreadsheet.columnCount).contains(column) else { return nil }
            return (row, column)
        }

        var splices: [(Range<Int>, String)] = []
        for unit in units(of: sites) {
            switch unit {
            case .single(let site):
                guard let new = adjusted(site) else {
                    splices.append((site.spliceStart..<site.range.upperBound, "refError()"))
                    continue
                }
                if new.row != site.row || new.column != site.columnIndex {
                    splices.append((site.range, site.text(row: new.row, columnIndex: new.column)))
                }

            case .pair(let first, let second):
                guard let newFirst = adjusted(first), let newSecond = adjusted(second) else {
                    splices.append((first.spliceStart..<second.range.upperBound, "refError()"))
                    continue
                }
                for (site, new) in [(first, newFirst), (second, newSecond)]
                where new.row != site.row || new.column != site.columnIndex {
                    splices.append((site.range, site.text(row: new.row, columnIndex: new.column)))
                }
            }
        }
        return apply(splices, to: raw)
    }

    // MARK: Sheet rename

    /// Rewrites `Old!…` / `'Old Name'!…` qualifiers to the new spelling —
    /// bare when the new name is identifier-shaped, quoted otherwise.
    /// A quoted name NOT followed by `!` is a named cell and stays put.
    public static func renamingSheet(_ raw: String, from oldName: String,
                                     to newName: String) -> String? {
        guard let tokens = try? Lexer.tokenize(raw) else { return nil }

        var splices: [(Range<Int>, String)] = []
        for (index, token) in tokens.enumerated() {
            let name: String
            switch token.kind {
            case .identifier(let n), .quotedName(let n): name = n
            default: continue
            }
            guard index + 1 < tokens.count, case .bang = tokens[index + 1].kind,
                  name.compare(oldName, options: .caseInsensitive) == .orderedSame else {
                continue
            }
            splices.append((token.range, spelled(newName)))
        }
        return apply(splices, to: raw)
    }

    /// Sheet names render bare when the identifier syntax can carry them.
    private static func spelled(_ name: String) -> String {
        let identifierShaped = !name.isEmpty
            && (name.first!.isLetter || name.first! == "_")
            && name.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
        return identifierShaped ? name : "'\(name)'"
    }

    // MARK: Token scanning

    /// One real cell-reference token and everything a rewrite needs.
    private struct Site {
        let column: String       // as typed
        let columnIndex: Int
        let row: Int
        let pinColumn: Bool
        let pinRow: Bool
        let range: Range<Int>
        let qualifier: String?   // Budget!A:1 — set on the ref AFTER the bang
        let qualifierStart: Int? // char start of the qualifier token
        let tokenIndex: Int
        let followedByDotDot: Bool // A:1.. — opens a range

        /// refError() splices swallow the qualifier too — `Budget!refError()`
        /// wouldn't parse.
        var spliceStart: Int { qualifierStart ?? range.lowerBound }

        func position(on axis: Axis) -> Int {
            axis == .row ? row : columnIndex
        }

        /// The token re-rendered with one axis changed (pins and, where the
        /// column is unchanged, its typed case are preserved).
        func text(with position: Int, on axis: Axis) -> String {
            axis == .row ? text(row: position, columnIndex: columnIndex, keepColumnCase: true)
                         : text(row: row, columnIndex: position)
        }

        func text(row newRow: Int, columnIndex newColumn: Int,
                  keepColumnCase: Bool = false) -> String {
            let columnText = (keepColumnCase || newColumn == columnIndex)
                ? column : CellAddress.columnName(forIndex: newColumn)
            return (pinColumn ? "$" : "") + columnText + ":"
                + (pinRow ? "$" : "") + String(newRow)
        }
    }

    /// A standalone reference or a `lo..hi` range pair (corners share the
    /// first corner's qualifier — that's how the parser scopes ranges).
    private enum Unit {
        case single(Site)
        case pair(Site, Site)
    }

    /// All real reference sites in the raw text, or nil when it doesn't lex
    /// (plain labels) or holds none.
    private static func scan(_ raw: String) -> [Site]? {
        guard let tokens = try? Lexer.tokenize(raw) else { return nil }

        var sites: [Site] = []
        var brackets: [Token.Kind] = [] // innermost-bracket tracking for {b:1}
        for (index, token) in tokens.enumerated() {
            switch token.kind {
            case .leftParen, .leftBracket, .leftBrace:
                brackets.append(token.kind)
            case .rightParen, .rightBracket, .rightBrace:
                if !brackets.isEmpty { brackets.removeLast() }

            case .cellReference(let column, let row, let pinColumn, let pinRow):
                // Multi-letter columns are named-argument sugar, never cells.
                guard column.count == 1,
                      let columnIndex = CellAddress.columnIndex(forName: column) else {
                    continue
                }
                // Compact map key: directly after `{` or `,` while the
                // innermost bracket is a brace (mirrors Parser.mapLiteral).
                if case .leftBrace? = brackets.last, index > 0 {
                    switch tokens[index - 1].kind {
                    case .leftBrace, .comma: continue
                    default: break
                    }
                }
                var qualifier: String?
                var qualifierStart: Int?
                if index >= 2, case .bang = tokens[index - 1].kind {
                    switch tokens[index - 2].kind {
                    case .identifier(let sheet), .quotedName(let sheet):
                        qualifier = sheet
                        qualifierStart = tokens[index - 2].range.lowerBound
                    default:
                        break
                    }
                }
                var followedByDotDot = false
                if index + 1 < tokens.count, case .dotDot = tokens[index + 1].kind {
                    followedByDotDot = true
                }
                sites.append(Site(column: column, columnIndex: columnIndex, row: row,
                                  pinColumn: pinColumn, pinRow: pinRow,
                                  range: token.range, qualifier: qualifier,
                                  qualifierStart: qualifierStart, tokenIndex: index,
                                  followedByDotDot: followedByDotDot))
            default:
                break
            }
        }

        // Pairing happens against the token stream: site, `..`, site.
        return sites.isEmpty ? nil : sites
    }

    /// Groups consecutive sites joined by `..` into range pairs.
    private static func units(of sites: [Site]) -> [Unit] {
        var units: [Unit] = []
        var index = 0
        while index < sites.count {
            let site = sites[index]
            if site.followedByDotDot, index + 1 < sites.count,
               sites[index + 1].tokenIndex == site.tokenIndex + 2,
               sites[index + 1].qualifier == nil { // corner two is always bare
                units.append(.pair(site, sites[index + 1]))
                index += 2
                continue
            }
            units.append(.single(site))
            index += 1
        }
        return units
    }

    /// Splices replacements back-to-front; nil when there are none.
    private static func apply(_ splices: [(Range<Int>, String)], to raw: String) -> String? {
        guard !splices.isEmpty else { return nil }
        var characters = Array(raw)
        for (range, text) in splices.sorted(by: { $0.0.lowerBound > $1.0.lowerBound }) {
            characters.replaceSubrange(range, with: text)
        }
        return String(characters)
    }
}
