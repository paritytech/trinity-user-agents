import SwiftUI

public extension CustomMessageWidgetNode {
    /// A cover cut into `columns` x `rows` cells (`RendererNode.CoverGrid`), with the cells its tree already clears,
    /// the brush, if any, that draws the cells cleared since, and the reveal, if any, it sends when cleared enough.
    struct CoverGridProps {
        public let key: String
        public let columns: Int
        public let rows: Int
        public let cleared: CoverCells
        public let brush: CoverBrush?
        public let reveal: CoverReveal?

        public init(
            key: String,
            columns: Int,
            rows: Int,
            cleared: CoverCells = CoverCells(),
            brush: CoverBrush? = nil,
            reveal: CoverReveal? = nil
        ) {
            self.key = key
            self.columns = min(max(columns, 1), CoverCells.maxSide)
            self.rows = min(max(rows, 1), CoverCells.maxSide)
            self.cleared = cleared
            self.brush = brush
            self.reveal = reveal
        }

        /// From a tree's node: its sides as sent, and its cleared cells as a bitset (`CoverCells(bits:)`).
        public init(
            key: String,
            columns: some BinaryInteger,
            rows: some BinaryInteger,
            bits: Data,
            brush: CoverBrush? = nil,
            reveal: CoverReveal? = nil
        ) {
            self.init(
                key: key,
                columns: Int(columns),
                rows: Int(rows),
                cleared: CoverCells(bits: bits),
                brush: brush,
                reveal: reveal
            )
        }
    }

    /// The brush a cover draws its newly cleared cells with (`RendererNode.CoverGrid`'s `brush`): a line `width`
    /// points wide, with round caps and joins, along the finger on the location pan keyed `pan`.
    struct CoverBrush: Equatable {
        public let pan: String
        public let width: CGFloat

        public init(pan: String, width: some BinaryInteger) {
            self.pan = pan
            self.width = CGFloat(width)
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

    /// How many cleared cells lie below `limit`: those inside a grid of `limit` cells.
    public func count(below limit: Int) -> Int {
        var total = 0
        forEach(below: limit) { _ in total += 1 }
        return total
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

/// The finger's strokes on one keyed location pan since the current tree, in the pan's own frame (from its top-left
/// corner, whatever the layout direction), for a cover whose brush follows that pan. A tree drops them, as it drops
/// the cells cleared since.
@Observable
public final class PanTrail {
    public private(set) var strokes: [[CGPoint]] = []

    /// The name of the pan's frame, which a cover inside it reads its own place in.
    static func space(_ pan: String) -> String { "coverBrush.pan.\(pan)" }

    public var isEmpty: Bool { strokes.isEmpty }

    /// A new stroke, from where the finger touched down.
    @MainActor
    func begin(at point: CGPoint) {
        strokes.append([point])
    }

    /// The finger reached `point` on the current stroke; one with no stroke begun starts one.
    @MainActor
    func extend(to point: CGPoint) {
        guard let last = strokes.indices.last else { strokes.append([point]); return }
        if strokes[last].last != point { strokes[last].append(point) }
    }

    @MainActor
    func reset() {
        if !strokes.isEmpty { strokes = [] }
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

    /// How far the holes' edge is softened before it is sharpened again: half the cell's shorter side. That melts the
    /// steps between neighbouring cells and the scallops between round holes into one round edge, and the edge stays
    /// where the softened holes are half clear, so a scratch keeps its width.
    static func softening(columns: Int, rows: Int, in size: CGSize) -> CGFloat {
        guard columns > 0, rows > 0, size.width > 0, size.height > 0 else { return 0 }
        return 0.5 * min(size.width / CGFloat(columns), size.height / CGFloat(rows))
    }

    /// How steeply the softened edge is sharpened: opacity is stretched this many times about one half, so the edge
    /// ends about a point wide, smooth but not blurred.
    static let edgeGain: Float = 16

    /// Leaves colour alone and stretches opacity `edgeGain` times about one half.
    static var sharpenEdge: ColorMatrix {
        var matrix = ColorMatrix()
        matrix.a4 = edgeGain
        matrix.a5 = 0.5 * (1 - edgeGain)
        return matrix
    }

    /// Where a brush may cut: a circle one cell's diagonal wide around each of `cells`, so a stroke whose worker
    /// cleared every cell within half the brush of the finger's path is cut whole, and nothing is cut more than a
    /// diagonal from a cleared cell.
    static func brushGate(
        _ cells: CoverCells, columns: Int, rows: Int, in size: CGSize, mirrored: Bool = false
    ) -> Path {
        var path = Path()
        guard columns > 0, rows > 0, size.width > 0, size.height > 0 else { return path }
        let width = size.width / CGFloat(columns)
        let height = size.height / CGFloat(rows)
        let radius = (width * width + height * height).squareRoot()
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

    /// The finger's strokes as one path, moved from the pan's frame into the cover's (`origin` is the cover's top-left
    /// corner in the pan's frame). A stroke of one point is a dot: a line too short to see, which round caps draw as a
    /// circle the brush's width, as the existing card does.
    static func brushStrokes(_ strokes: [[CGPoint]], origin: CGPoint = .zero) -> Path {
        var path = Path()
        for stroke in strokes {
            guard let first = stroke.first else { continue }
            let start = CGPoint(x: first.x - origin.x, y: first.y - origin.y)
            path.move(to: stroke.count == 1 ? CGPoint(x: start.x - 0.1, y: start.y) : start)
            if stroke.count == 1 {
                path.addLine(to: start)
            } else {
                for point in stroke.dropFirst() {
                    path.addLine(to: CGPoint(x: point.x - origin.x, y: point.y - origin.y))
                }
            }
        }
        return path
    }

    /// The brush's line: its width, with round caps and joins.
    static func brushStyle(_ brush: CustomMessageWidgetNode.CoverBrush) -> StrokeStyle {
        StrokeStyle(lineWidth: brush.width, lineCap: .round, lineJoin: .round)
    }
}

/// Draws its children as a cover, with a round hole cut for every cleared cell: those of the tree, plus those the
/// worker cleared since (until the next tree, which always wins). The holes' edge is smoothed as it is drawn
/// (`CoverGridShape.softening`), so cells read as one scratch, not as steps. With a brush, the cells cleared since the
/// tree are drawn as the brush's strokes along the finger instead, cut only near those cells
/// (`CoverGridShape.brushGate`), while the host holds a stroke of the brush's pan.
struct NodeCoverGridView<Content: View>: View {
    let props: CustomMessageWidgetNode.CoverGridProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    @ViewBuilder let content: () -> Content

    @Environment(\.gestureOffsets) private var updates
    @State private var origin = CGPoint.zero

    var body: some View {
        content()
            .applyWidgetNodeModifiers(modifiers, alignment: .topLeading)
            .onGeometryChange(for: CGPoint.self) { proxy in
                props.brush.map { proxy.frame(in: .named(PanTrail.space($0.pan))).origin } ?? .zero
            } action: { origin = $0 }
            .mask {
                CoverGridMask(
                    props: props,
                    channel: updates?.cover(for: props.key),
                    trail: props.brush.flatMap { updates?.trail(for: $0.pan) },
                    origin: origin
                )
            }
            .background {
                if let reveal = props.reveal {
                    CoverRevealWatcher(
                        reveal: reveal,
                        props: props,
                        channel: updates?.cover(for: props.key),
                        touch: updates?.touch(for: reveal.pan)
                    )
                }
            }
    }
}

private struct CoverGridMask: View {
    let props: CustomMessageWidgetNode.CoverGridProps
    let channel: CoverGridChannel?
    let trail: PanTrail?
    let origin: CGPoint

    @Environment(\.layoutDirection) private var layoutDirection

    var body: some View {
        let since = channel?.cleared ?? CoverCells()
        let brushed = props.brush != nil && trail?.isEmpty == false && !since.isEmpty
        let holes = brushed ? props.cleared : props.cleared.union(since)
        let strokes = brushed ? trail?.strokes ?? [] : []
        let mirrored = layoutDirection == .rightToLeft
        Canvas { context, size in
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.black))
            guard !holes.isEmpty || brushed else { return }
            context.blendMode = .destinationOut
            if !holes.isEmpty {
                let shape = CoverGridShape.holes(
                    holes, columns: props.columns, rows: props.rows, in: size, mirrored: mirrored
                )
                let softening = CoverGridShape.softening(columns: props.columns, rows: props.rows, in: size)
                context.drawLayer { layer in
                    // A layer's filters run last added first: the holes are softened, then their edge is sharpened.
                    layer.addFilter(.colorMatrix(CoverGridShape.sharpenEdge))
                    layer.addFilter(.blur(radius: softening))
                    layer.fill(shape, with: .color(.black))
                }
            }
            if brushed, let brush = props.brush {
                context.clip(to: CoverGridShape.brushGate(
                    since, columns: props.columns, rows: props.rows, in: size, mirrored: mirrored
                ))
                context.stroke(
                    CoverGridShape.brushStrokes(strokes, origin: origin),
                    with: .color(.black),
                    style: CoverGridShape.brushStyle(brush)
                )
            }
        }
    }
}
