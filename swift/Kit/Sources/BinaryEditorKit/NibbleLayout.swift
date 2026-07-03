import CoreGraphics

/// How many columns the binary nibble grid should use for `count` cells of
/// `itemWidth`, packed into `maxWidth` with `spacing` between them.
///
/// Pure so it can be unit-tested away from SwiftUI's `Layout`. The result is a
/// power of two (nibbles read in 4-bit groups) bounded by `count`. The guards
/// matter: SwiftUI proposes an INFINITE width while sizing a content-sized
/// window, and a naive `Int(maxWidth / itemWidth)` would trap on `Int(.infinity)`
/// — that crash shipped once. Infinite/degenerate inputs fall back to one row.
func nibbleColumnCount(maxWidth: CGFloat, itemWidth: CGFloat, spacing: CGFloat, count: Int) -> Int {
    guard count > 0, itemWidth > 0, maxWidth.isFinite else { return max(count, 1) }
    // How many items fit, bounded by `count` so an enormous proposed width never
    // overflows Int.
    let ratio = (maxWidth + spacing) / (itemWidth + spacing)
    let fit = ratio >= CGFloat(count) ? count : max(1, Int(ratio))
    var columns = 1
    while columns * 2 <= min(fit, count) { columns *= 2 }
    return columns
}
