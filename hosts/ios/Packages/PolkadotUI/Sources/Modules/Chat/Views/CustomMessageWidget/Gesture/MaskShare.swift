import SwiftUI

public extension CustomMessageWidgetNode.Mask {
    /// When a mask's shapes cover enough of its node (`Mask.clearedShare`): `atPermille` thousandths of it, cleared for
    /// a `hide` mask and shown for a `show` mask. The host then sends `action`, with no payload.
    struct ClearedShare: Equatable {
        public let atPermille: Int
        public let action: String
        /// Also checked while the followed finger is down, so the action is sent mid-stroke.
        public let whileDown: Bool
        public let rescue: Rescue?

        public init(atPermille: some BinaryInteger, action: String, whileDown: Bool = false, rescue: Rescue? = nil) {
            self.atPermille = Int(atPermille)
            self.action = action
            self.whileDown = whileDown
            self.rescue = rescue
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
/// centre lies in any shape (on the edge counts); a stamped line covers what its stamps do (`MaskBrush::stamps`).
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
            case let .stamped(width, brush, points):
                let reach = width / 2
                for centre in brush.stamps(width: width, along: points) {
                    let box = CGRect(x: centre.x - reach, y: centre.y - reach, width: width, height: width)
                    mark(box) { x, y in brush.covers(width: width, centre: centre, point: CGPoint(x: x, y: y)) }
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
/// down again, so shapes a worker adds in answer to the pan's end count; while the finger is down only with
/// `whileDown`, as it moves; at most once per lift. A mask with no pan to follow is checked whenever shapes are added,
/// at most once per tree. A tree's own shapes are never a reason to check.
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
        /// The places the followed finger has passed since the tree, counted only where a check while down needs them.
        var moves = 0
    }

    private var seen: Input
    private var armed = false
    private var sentThisTree = false
    /// The lift whose check was spent by a send while the finger was down.
    private var spentLift: Int?

    /// What happened before the mask was drawn, which must not send its action.
    init(_ input: Input = Input()) {
        seen = input
    }

    /// Whether to check the share now. `reached` is asked only then.
    mutating func update(_ input: Input, follows: Bool, whileDown: Bool = false, reached: () -> Bool) -> Bool {
        let lifted = input.lifts != seen.lifts
        let added = input.additions != seen.additions
        let moved = input.moves != seen.moves
        if input.trees != seen.trees { sentThisTree = false }
        seen = input
        if follows {
            if lifted { armed = spentLift != input.lifts }
            guard !input.down else {
                armed = false
                guard whileDown, spentLift != input.lifts + 1, moved || added, reached() else { return false }
                spentLift = input.lifts + 1
                return true
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

/// Sends a mask's cleared-share action, rescues a share that stopped growing, and plays the mask's haptics: an empty
/// view that reads the shapes added since the tree, the followed pan's touch and the node's frame, so only it draws
/// again when they change, never the masked node. It reads the finger's places only where a check while down, a rescue
/// or haptics need the cover as it grows.
struct MaskShareWatcher: View {
    let mask: CustomMessageWidgetNode.Mask
    let channel: MaskChannel?
    let trail: PanTrail?
    let touch: PanTouch?

    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.layoutDirection) private var layoutDirection
    @State private var watch: MaskShareWatch?
    @State private var cover = MaskCoverWatch()
    @State private var geometry = MaskGeometry()
    @State private var rescueAt: TimeInterval?

    var body: some View {
        let input = MaskShareWatch.Input(
            down: touch?.isDown ?? false,
            lifts: touch?.lifts ?? 0,
            trees: channel?.trees ?? 0,
            additions: channel?.additions ?? 0,
            moves: readsMoves ? trail?.points ?? 0 : 0
        )
        Color.clear
            .onGeometryChange(for: MaskGeometry.self) { MaskGeometry($0) } action: { geometry = $0 }
            .onAppear {
                watch = MaskShareWatch(input)
                startTree()
            }
            .onChange(of: geometry.size) { before, _ in
                if before == .zero { startTree() }
            }
            .onChange(of: input) { before, input in
                if input.trees != before.trees { startTree() } else { measure() }
                guard let share = mask.clearedShare else { return }
                var next = watch ?? MaskShareWatch()
                let send = next.update(
                    input, follows: mask.follow != nil, whileDown: share.whileDown, reached: { reached(share) }
                )
                watch = next
                if send { sendAction(share) }
            }
            .task(id: rescueAt) { await rescueWhenDue() }
    }
}

private extension MaskShareWatcher {
    /// The cover is measured as it grows only for a rescue or haptics.
    var measuresCover: Bool {
        mask.haptics != nil || mask.clearedShare?.rescue != nil
    }

    var readsMoves: Bool {
        mask.follow != nil && (measuresCover || mask.clearedShare?.whileDown == true)
    }

    func count() -> (covered: Int, total: Int) {
        let source = MaskSource.of(
            mask: mask, trail: trail, geometry: geometry, mirrored: layoutDirection == .rightToLeft
        )
        return MaskCoverage.count(source.picture(added: channel?.added ?? []).shapes, in: geometry.size)
    }

    func reached(_ share: CustomMessageWidgetNode.Mask.ClearedShare) -> Bool {
        let count = count()
        return share.reached(covered: count.covered, samples: count.total)
    }

    /// The tree's own cover is where haptic steps count from and where the rescue's clock starts.
    func startTree() {
        guard measuresCover else { return }
        let count = count()
        cover.tree(covered: count.covered, total: count.total, at: CACurrentMediaTime())
        rescueAt = mask.clearedShare.flatMap { cover.rescueDue(share: $0) }
    }

    func measure() {
        guard measuresCover else { return }
        let count = count()
        let step = cover.measured(
            covered: count.covered, total: count.total, at: CACurrentMediaTime(), haptics: mask.haptics
        )
        if step, let haptics = mask.haptics { HapticPlayer.play(haptics.step) }
        rescueAt = mask.clearedShare.flatMap { cover.rescueDue(share: $0) }
    }

    func sendAction(_ share: CustomMessageWidgetNode.Mask.ClearedShare) {
        actionHandler?(share.action, nil)
        cover.sent()
        rescueAt = nil
        if let reach = mask.haptics?.reach { HapticPlayer.play(reach) }
    }

    func rescueWhenDue() async {
        guard let rescueAt, let share = mask.clearedShare else { return }
        let wait = rescueAt - CACurrentMediaTime()
        if wait > 0 { try? await Task.sleep(for: .seconds(wait)) }
        guard !Task.isCancelled, self.rescueAt == rescueAt else { return }
        sendAction(share)
    }
}
