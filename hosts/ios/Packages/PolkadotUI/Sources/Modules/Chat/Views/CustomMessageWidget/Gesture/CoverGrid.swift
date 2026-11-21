import SwiftUI

public extension CustomMessageWidgetNode {
    /// A cover cut into `columns` x `rows` cells (`RendererNode.CoverGrid`), with the cells its tree already clears.
    struct CoverGridProps {
        public let key: String
        public let columns: Int
        public let rows: Int
        public let cleared: CoverCells

        public init(key: String, columns: Int, rows: Int, cleared: CoverCells = CoverCells()) {
            self.key = key
            self.columns = min(max(columns, 1), CoverCells.maxSide)
            self.rows = min(max(rows, 1), CoverCells.maxSide)
            self.cleared = cleared
        }

        /// From a tree's node: its sides as sent, and its cleared cells as a bitset (`CoverCells(bits:)`).
        public init(key: String, columns: some BinaryInteger, rows: some BinaryInteger, bits: Data) {
            self.init(key: key, columns: Int(columns), rows: Int(rows), cleared: CoverCells(bits: bits))
        }
    }
}

/// Cleared cells of a cover grid, one bit per cell, `row * columns + column`.
public struct CoverCells: Equatable {
    /// Most cells a side may have, as the renderer allows.
    public static let maxSide = 128
    private static let limit = maxSide * maxSide

    private var words: [UInt64] = []
    public private(set) var count = 0
    public var isEmpty: Bool { words.allSatisfy { $0 == 0 } }

    public init() {}

    /// A tree's bitset: cell `index` is bit `index % 8` of byte `index / 8`.
    public init(bits: Data) {
        for (offset, byte) in bits.enumerated() where byte != 0 {
            for bit in 0 ..< 8 where byte & (1 << bit) != 0 {
                insert(offset * 8 + bit)
            }
        }
    }

    public func contains(_ cell: Int) -> Bool {
        guard cell >= 0, cell / 64 < words.count else { return false }
        return words[cell / 64] & (1 << UInt64(cell % 64)) != 0
    }

    /// Adds `cell`; false when it was already cleared or lies outside any grid.
    @discardableResult
    public mutating func insert(_ cell: Int) -> Bool {
        guard cell >= 0, cell < Self.limit, !contains(cell) else { return false }
        let word = cell / 64
        if word >= words.count { words.append(contentsOf: repeatElement(0, count: word - words.count + 1)) }
        words[word] |= 1 << UInt64(cell % 64)
        count += 1
        return true
    }

    public func union(_ other: CoverCells) -> CoverCells {
        guard !other.isEmpty else { return self }
        var merged = self
        other.forEach(below: Self.limit) { merged.insert($0) }
        return merged
    }

    /// Calls `body` with every cleared cell below `limit`, in order.
    func forEach(below limit: Int, _ body: (Int) -> Void) {
        for (index, word) in words.enumerated() where word != 0 {
            var rest = word
            while rest != 0 {
                let bit = rest.trailingZeroBitCount
                let cell = index * 64 + bit
                if cell >= limit { return }
                body(cell)
                rest &= rest - 1
            }
        }
    }
}

/// The cells a worker cleared on one keyed cover since its last tree (`RendererNode.ClearCells`), observed by that
/// cover's mask alone, so an update redraws only the mask.
@Observable
public final class CoverGridChannel {
    /// nil until the worker clears a cell after the current tree.
    public private(set) var cleared: CoverCells?

    @MainActor
    func clear(_ cells: [Int]) {
        var next = cleared ?? CoverCells()
        var changed = false
        for cell in cells where next.insert(cell) {
            changed = true
        }
        if changed || cleared == nil { cleared = next }
    }

    @MainActor
    func reset() {
        if cleared != nil { cleared = nil }
    }
}

enum CoverGridShape {
    /// One round hole per cleared cell: centred on the cell, its radius half the cell's diagonal, so it reaches the
    /// cell's corners and neighbouring holes merge into one soft-edged scratch with no cover left between them.
    /// Column 0 sits at the leading edge; `mirrored` puts it on the right for right-to-left layouts.
    static func holes(
        _ cells: CoverCells, columns: Int, rows: Int, in size: CGSize, mirrored: Bool = false
    ) -> Path {
        var path = Path()
        guard columns > 0, rows > 0, size.width > 0, size.height > 0 else { return path }
        let width = size.width / CGFloat(columns)
        let height = size.height / CGFloat(rows)
        let radius = 0.5 * (width * width + height * height).squareRoot()
        cells.forEach(below: columns * rows) { cell in
            let centreX = (CGFloat(cell % columns) + 0.5) * width
            let centre = CGPoint(
                x: mirrored ? size.width - centreX : centreX, y: (CGFloat(cell / columns) + 0.5) * height
            )
            path.addEllipse(in: CGRect(
                x: centre.x - radius, y: centre.y - radius, width: radius * 2, height: radius * 2
            ))
        }
        return path
    }
}

/// Draws its children as a cover, with a round hole cut for every cleared cell: those of the tree, plus those the
/// worker cleared since (until the next tree, which always wins).
struct NodeCoverGridView<Content: View>: View {
    let props: CustomMessageWidgetNode.CoverGridProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    @ViewBuilder let content: () -> Content

    @Environment(\.gestureOffsets) private var updates

    var body: some View {
        content()
            .applyWidgetNodeModifiers(modifiers, alignment: .topLeading)
            .mask { CoverGridMask(props: props, channel: updates?.cover(for: props.key)) }
    }
}

private struct CoverGridMask: View {
    let props: CustomMessageWidgetNode.CoverGridProps
    let channel: CoverGridChannel?

    @Environment(\.layoutDirection) private var layoutDirection

    var body: some View {
        let cells = props.cleared.union(channel?.cleared ?? CoverCells())
        let mirrored = layoutDirection == .rightToLeft
        Canvas { context, size in
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.black))
            guard !cells.isEmpty else { return }
            context.blendMode = .destinationOut
            let holes = CoverGridShape.holes(
                cells, columns: props.columns, rows: props.rows, in: size, mirrored: mirrored
            )
            context.fill(holes, with: .color(.black))
        }
    }
}
