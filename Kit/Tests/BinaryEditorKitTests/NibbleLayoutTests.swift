import CoreGraphics
import Testing
@testable import BinaryEditorKit

/// Regression coverage for the nibble-grid column math — the function that
/// trapped on `Int(.infinity)` and crashed the standalone editor. Pure, so it
/// tests without SwiftUI.
@Suite("Nibble grid columns")
struct NibbleLayoutTests {
    // The crash: SwiftUI proposes an infinite width while sizing a
    // content-sized window. Must NOT trap — falls back to one row of `count`.
    @Test func infiniteWidthDoesNotTrapAndFitsOneRow() {
        #expect(nibbleColumnCount(maxWidth: .infinity, itemWidth: 20, spacing: 18, count: 32) == 32)
        #expect(nibbleColumnCount(maxWidth: .greatestFiniteMagnitude, itemWidth: 20, spacing: 18, count: 8) == 8)
    }

    @Test func degenerateInputsFallBackToOneRow() {
        #expect(nibbleColumnCount(maxWidth: 500, itemWidth: 0, spacing: 18, count: 16) == 16)  // zero item width
        #expect(nibbleColumnCount(maxWidth: 500, itemWidth: 20, spacing: 18, count: 0) == 1)    // no items
    }

    @Test func tinyWidthGivesASingleColumn() {
        #expect(nibbleColumnCount(maxWidth: 5, itemWidth: 20, spacing: 18, count: 8) == 1)
        #expect(nibbleColumnCount(maxWidth: 0, itemWidth: 20, spacing: 18, count: 8) == 1)
    }

    @Test func oversizedWidthCapsAtCount() {
        // A huge but finite width never returns more columns than there are cells.
        #expect(nibbleColumnCount(maxWidth: 1_000_000, itemWidth: 20, spacing: 18, count: 32) == 32)
        #expect(nibbleColumnCount(maxWidth: 1_000_000, itemWidth: 20, spacing: 18, count: 6) == 4) // power-of-two ≤ 6
    }

    @Test func columnsAreAlwaysAPowerOfTwo() {
        // ratio ≈ 5 → fit 5 → largest power of two ≤ min(5, 8) = 4.
        #expect(nibbleColumnCount(maxWidth: 172, itemWidth: 20, spacing: 18, count: 8) == 4)
        for w in stride(from: 10.0, through: 2000.0, by: 7.0) {
            let n = nibbleColumnCount(maxWidth: CGFloat(w), itemWidth: 20, spacing: 18, count: 32)
            #expect(n > 0 && (n & (n - 1)) == 0, "got \(n) for width \(w)") // power of two
        }
    }
}
