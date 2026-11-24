import SwiftUI

public extension CustomMessageWidgetNode.Mask {
    /// When a mask's shapes cover enough of its node (`Mask.clearedShare`): `atPermille` thousandths of it, cleared for
    /// a `hide` mask and shown for a `show` mask. The host then sends `action`, with no payload.
    struct ClearedShare: Equatable {
        public let atPermille: Int
        public let action: String

        public init(atPermille: some BinaryInteger, action: String) {
            self.atPermille = Int(atPermille)
            self.action = action
        }

        /// Whether `covered` samples of `samples` reach the share, as the core counts it:
        /// `covered * 1000 >= atPermille * samples`.
        public func reached(covered: Int, samples: Int) -> Bool {
            covered * 1_000 >= atPermille * samples
        }
    }
}

/// How much of a node a mask's shapes cover, by the rule the core gives (`cleared_share`): the node is laid with
/// 2 by 2 point squares from its top-start corner, a part square counting as whole, and a square is covered when its
/// centre lies in any shape (on the edge counts).
enum MaskCoverage {
    /// The side of a sample square, in points.
    static let side: CGFloat = 2

    /// Covered samples, and all samples, of a node `size` points large; `shapes` are from its top-start corner.
    static func count(_ shapes: [CustomMessageWidgetNode.Mask.Shape], in size: CGSize) -> (covered: Int, total: Int) {
        // The core counts in hundredths of a point; rounding the size there keeps the sample count the same.
        let columns = samples(along: size.width)
        let rows = samples(along: size.height)
        let total = columns * rows
        if shapes.contains(.all) { return (total, total) }
        guard total > 0 else { return (0, 0) }
        var covered = [Bool](repeating: false, count: total)

        func mark(_ box: CGRect, _ inside: (CGFloat, CGFloat) -> Bool) {
            let first = { (edge: CGFloat) in max(Int(((edge - side / 2) / side).rounded(.up)), 0) }
            let last = { (edge: CGFloat, count: Int) -> Int? in
                let at = ((edge - side / 2) / side).rounded(.down)
                return at < 0 ? nil : min(Int(at), count - 1)
            }
            guard let i1 = last(box.maxX, columns), let j1 = last(box.maxY, rows) else { return }
            let i0 = first(box.minX), j0 = first(box.minY)
            guard i0 <= i1, j0 <= j1 else { return }
            for j in j0 ... j1 {
                for i in i0 ... i1 where inside(CGFloat(i) * side + side / 2, CGFloat(j) * side + side / 2) {
                    covered[j * columns + i] = true
                }
            }
        }

        for shape in shapes {
            switch shape {
            case .all:
                break
            case let .circle(center, radius):
                let box = CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
                mark(box) { x, y in square(x - center.x) + square(y - center.y) <= radius * radius }
            case let .stroke(width, points):
                let r = width / 2
                let segments: [(CGPoint, CGPoint)] =
                    switch points.count {
                    case 0: []
                    case 1: [(points[0], points[0])]
                    default: zip(points, points.dropFirst()).map { ($0, $1) }
                    }
                for (a, b) in segments {
                    let box = CGRect(
                        x: min(a.x, b.x) - r, y: min(a.y, b.y) - r,
                        width: abs(b.x - a.x) + 2 * r, height: abs(b.y - a.y) + 2 * r
                    )
                    mark(box) { x, y in distanceSquared(x, y, a, b) <= r * r }
                }
            }
        }
        return (covered.filter { $0 }.count, total)
    }

    private static func samples(along length: CGFloat) -> Int {
        let hundredths = max(Int(length * 100), 0)
        return (hundredths + 199) / 200
    }

    private static func square(_ value: CGFloat) -> CGFloat {
        value * value
    }

    /// From `(x, y)` to the segment `a`-`b`, squared.
    private static func distanceSquared(_ x: CGFloat, _ y: CGFloat, _ a: CGPoint, _ b: CGPoint) -> CGFloat {
        let dx = b.x - a.x, dy = b.y - a.y
        let length = dx * dx + dy * dy
        let t = length == 0 ? 0 : min(max(((x - a.x) * dx + (y - a.y) * dy) / length, 0), 1)
        return square(x - a.x - t * dx) + square(y - a.y - t * dy)
    }
}

/// Whether a finger is on one keyed pan, and how many times it has lifted, for a mask that checks its share on a
/// lift. A tree leaves it as it is: the finger has not moved.
@Observable
public final class PanTouch {
    public private(set) var isDown = false
    public private(set) var lifts = 0

    @MainActor
    func set(down: Bool) {
        guard down != isDown else { return }
        isDown = down
        if !down { lifts += 1 }
    }
}

/// Decides when a mask's cleared share is checked, and so when its action may be sent.
///
/// A mask that follows a pan is checked when the finger lifts, and again whenever shapes are added before it touches
/// down again, so shapes a worker adds in answer to the pan's end count; never while the finger is down; at most once
/// per lift. A mask with no pan to follow is checked whenever shapes are added, at most once per tree. A tree's own
/// shapes are never a reason to check.
struct MaskShareWatch: Equatable {
    struct Input: Equatable {
        /// Whether the followed pan's finger is down.
        var down = false
        /// The followed pan's lifts so far.
        var lifts = 0
        /// The trees this mask has seen.
        var trees = 0
        /// How many times shapes were added to this mask.
        var additions = 0
    }

    private var seen: Input
    private var armed = false
    private var sentThisTree = false

    /// What happened before the mask was drawn, which must not send its action.
    init(_ input: Input = Input()) {
        seen = input
    }

    /// Whether to check the share now. `reached` is asked only then.
    mutating func update(_ input: Input, follows: Bool, reached: () -> Bool) -> Bool {
        let lifted = input.lifts != seen.lifts
        let added = input.additions != seen.additions
        if input.trees != seen.trees { sentThisTree = false }
        seen = input
        if follows {
            if lifted { armed = true }
            guard !input.down else {
                armed = false
                return false
            }
            guard armed, lifted || added, reached() else { return false }
            armed = false
            return true
        }
        guard added, !sentThisTree, reached() else { return false }
        sentThisTree = true
        return true
    }
}

/// Sends a mask's cleared-share action: an empty view that reads the shapes added since the tree, the followed pan's
/// touch and the node's frame, so only it draws again when they change, never the masked node.
struct MaskShareWatcher: View {
    let share: CustomMessageWidgetNode.Mask.ClearedShare
    let mask: CustomMessageWidgetNode.Mask
    let channel: MaskChannel?
    let trail: PanTrail?
    let touch: PanTouch?

    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.layoutDirection) private var layoutDirection
    @State private var watch: MaskShareWatch?
    @State private var geometry = MaskGeometry()

    var body: some View {
        let input = MaskShareWatch.Input(
            down: touch?.isDown ?? false,
            lifts: touch?.lifts ?? 0,
            trees: channel?.trees ?? 0,
            additions: channel?.additions ?? 0
        )
        Color.clear
            .onGeometryChange(for: MaskGeometry.self) { MaskGeometry($0) } action: { geometry = $0 }
            .onAppear { watch = MaskShareWatch(input) }
            .onChange(of: input) { _, input in
                var next = watch ?? MaskShareWatch()
                let send = next.update(input, follows: mask.follow != nil, reached: reached)
                watch = next
                if send { actionHandler?(share.action, nil) }
            }
    }

    private func reached() -> Bool {
        let source = MaskSource.of(
            mask: mask, trail: trail, geometry: geometry, mirrored: layoutDirection == .rightToLeft
        )
        let count = MaskCoverage.count(source.picture(added: channel?.added ?? []).shapes, in: geometry.size)
        return share.reached(covered: count.covered, samples: count.total)
    }
}
