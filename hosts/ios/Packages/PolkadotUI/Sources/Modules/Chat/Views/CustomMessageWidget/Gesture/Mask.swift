import QuartzCore
import SwiftUI

public extension CustomMessageWidgetNode {
    /// A node's mask (`Modifier.Mask`): shapes that hide the node, or that are all of it that shows. Places and sizes
    /// are in points here (the wire's hundredths divided by 100), from the node's top-start corner.
    struct Mask: Equatable {
        public enum Mode: Equatable {
            /// The node is hidden inside the shapes.
            case hide
            /// The node shows only inside the shapes.
            case show
        }

        public enum Shape: Equatable {
            /// The whole node.
            case all
            /// A filled circle.
            case circle(center: CGPoint, radius: CGFloat)
            /// A line through `points`, `width` wide, with round caps and joins; one point is a dot.
            case stroke(width: CGFloat, points: [CGPoint])
            /// A line through `points` drawn with copies of `brush`'s tip, `width` long, stamped along it; one point
            /// is one stamp.
            case stamped(width: CGFloat, brush: Brush, points: [CGPoint])
        }

        /// The location pan whose strokes the host draws into the mask itself, their width, and the tip they are
        /// drawn with (nil: round).
        public struct Follow: Equatable {
            public let pan: String
            public let width: CGFloat
            public let brush: Brush?

            public init(pan: String, width: CGFloat, brush: Brush? = nil) {
                self.pan = pan
                self.width = width
                self.brush = brush
            }
        }

        public let key: String?
        public let mode: Mode
        public let shapes: [Shape]
        public let follow: Follow?
        /// Seconds; zero changes the mask at once when a new tree changes it.
        public let fade: TimeInterval
        /// The action the host sends once the shapes cover enough of the node.
        public let clearedShare: ClearedShare?

        public init(
            key: String? = nil,
            mode: Mode = .hide,
            shapes: [Shape] = [],
            follow: Follow? = nil,
            fade: TimeInterval = 0,
            clearedShare: ClearedShare? = nil
        ) {
            self.key = key
            self.mode = mode
            self.shapes = shapes
            self.follow = follow
            self.fade = fade
            self.clearedShare = clearedShare
        }
    }
}

/// Everything a mask draws at one moment, in the masked node's own frame (points, from its top-start corner).
struct MaskPicture: Equatable {
    var mode: CustomMessageWidgetNode.Mask.Mode
    var shapes: [CustomMessageWidgetNode.Mask.Shape]

    /// The tree's shapes, the shapes added since, and the followed pan's strokes (left to right, from the pan's
    /// top-left corner) moved into the node's frame: `place` is where the pan's top-left corner is from the node's on
    /// screen, `scale` how much larger than its own size the node is drawn there (an ancestor's press, say), and a
    /// right-to-left node of width `mirroredWidth` counts them from its right edge, as it does its own shapes.
    init(
        _ mask: CustomMessageWidgetNode.Mask,
        added: [CustomMessageWidgetNode.Mask.Shape] = [],
        strokes: [[CGPoint]] = [],
        place: CGPoint = .zero,
        scale: CGSize = CGSize(width: 1, height: 1),
        mirroredWidth: CGFloat? = nil
    ) {
        mode = mask.mode
        shapes = mask.shapes + added
        guard let follow = mask.follow else { return }
        for stroke in strokes where !stroke.isEmpty {
            let points = stroke.map { point in
                let moved = (point.x + place.x) / scale.width
                return CGPoint(x: mirroredWidth.map { $0 - moved } ?? moved, y: (point.y + place.y) / scale.height)
            }
            if let brush = follow.brush {
                shapes.append(.stamped(width: follow.width, brush: brush, points: points))
            } else {
                shapes.append(.stroke(width: follow.width, points: points))
            }
        }
    }

    init(mode: CustomMessageWidgetNode.Mask.Mode, shapes: [CustomMessageWidgetNode.Mask.Shape]) {
        self.mode = mode
        self.shapes = shapes
    }
}

/// How a mask is drawn: one path for its filled shapes and stamps, one per stroke width, all in left-to-right points.
enum MaskDrawing {
    /// A shape's place from the top-start corner, as drawn: right-to-left layouts count from the right edge.
    static func place(_ point: CGPoint, width: CGFloat, mirrored: Bool) -> CGPoint {
        mirrored ? CGPoint(x: width - point.x, y: point.y) : point
    }

    /// The filled shapes (whole node, circles, one-point strokes as dots, and every stamp of a stamped line) as one
    /// path.
    static func fills(_ shapes: [CustomMessageWidgetNode.Mask.Shape], in size: CGSize, mirrored: Bool) -> Path {
        var path = Path()
        for shape in shapes {
            switch shape {
            case .all:
                path.addRect(CGRect(origin: .zero, size: size))
            case let .circle(center, radius) where radius > 0:
                path.addEllipse(in: circle(place(center, width: size.width, mirrored: mirrored), radius))
            case let .stroke(width, points) where points.count == 1 && width > 0:
                path.addEllipse(in: circle(place(points[0], width: size.width, mirrored: mirrored), width / 2))
            case let .stamped(width, brush, points) where width > 0:
                for centre in brush.stamps(width: width, along: points) {
                    let at = place(centre, width: size.width, mirrored: mirrored)
                    path.addPath(brush.outline(width: width, at: at, mirrored: mirrored))
                }
            default:
                break
            }
        }
        return path
    }

    /// Strokes of two points or more, one path per width, in the order the widths first appear.
    static func lines(
        _ shapes: [CustomMessageWidgetNode.Mask.Shape], in size: CGSize, mirrored: Bool
    ) -> [(width: CGFloat, path: Path)] {
        var widths: [CGFloat] = []
        var paths: [CGFloat: Path] = [:]
        for case let .stroke(width, points) in shapes where points.count > 1 && width > 0 {
            if paths[width] == nil { widths.append(width) }
            var path = paths[width] ?? Path()
            path.move(to: place(points[0], width: size.width, mirrored: mirrored))
            for point in points.dropFirst() {
                path.addLine(to: place(point, width: size.width, mirrored: mirrored))
            }
            paths[width] = path
        }
        return widths.map { ($0, paths[$0] ?? Path()) }
    }

    /// Draws where the node shows: opaque where it does, clear where the mask hides it.
    static func draw(_ picture: MaskPicture, in context: inout GraphicsContext, size: CGSize, mirrored: Bool) {
        let ink = GraphicsContext.Shading.color(.black)
        if picture.mode == .hide {
            context.fill(Path(CGRect(origin: .zero, size: size)), with: ink)
            guard !picture.shapes.isEmpty else { return }
            context.blendMode = .destinationOut
        }
        let filled = fills(picture.shapes, in: size, mirrored: mirrored)
        if !filled.isEmpty { context.fill(filled, with: ink) }
        for line in lines(picture.shapes, in: size, mirrored: mirrored) {
            let style = StrokeStyle(lineWidth: line.width, lineCap: .round, lineJoin: .round)
            context.stroke(line.path, with: ink, style: style)
        }
    }

    private static func circle(_ center: CGPoint, _ radius: CGFloat) -> CGRect {
        CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
    }

    /// The CSS `ease` curve.
    static let ease = UnitCurve.bezier(
        startControlPoint: UnitPoint(x: 0.25, y: 0.1), endControlPoint: UnitPoint(x: 0.25, y: 1)
    )
}

/// A fade from what a keyed mask drew before a tree to what the tree draws.
struct MaskFade: Equatable {
    var from: MaskPicture
    var start: TimeInterval
    var duration: TimeInterval

    /// How far along the fade is at `now`, eased, 0 through 1.
    func progress(at now: TimeInterval) -> Double {
        guard duration > 0 else { return 1 }
        return MaskDrawing.ease.value(at: min(max((now - start) / duration, 0), 1))
    }

    func isDone(at now: TimeInterval) -> Bool {
        now - start >= duration
    }
}

/// What one keyed mask gained since its tree (`RendererNode.ExtendMask`), and how it fades into the next tree. Its
/// mask view alone observes it, so an update redraws only that mask.
@Observable
public final class MaskChannel {
    /// Shapes added since the current tree, in the node's frame.
    public private(set) var added: [CustomMessageWidgetNode.Mask.Shape] = []
    private(set) var fade: MaskFade?
    /// How many times shapes were added, and how many trees arrived, since the channel was made: when to check a
    /// cleared share.
    private(set) var additions = 0
    private(set) var trees = 0

    /// What the mask's view draws from besides the channel: its tree's mask, the trail it follows, and where the
    /// trail's frame is in its own. Kept without redrawing anything, to know what was drawn when a tree arrives.
    @ObservationIgnored var source: MaskSource?

    @MainActor
    func extend(_ shapes: [CustomMessageWidgetNode.Mask.Shape]) {
        guard !shapes.isEmpty else { return }
        added.append(contentsOf: shapes)
        additions += 1
    }

    /// A new tree arrived whose mask with this key is `next` (nil when it has none): it sets the shapes itself. A
    /// change fades from what was drawn (strokes included, so this comes before the trails are dropped) when `next`
    /// asks for a fade; no change leaves a running fade alone.
    @MainActor
    func treeArrived(next: CustomMessageWidgetNode.Mask?, now: TimeInterval = CACurrentMediaTime()) {
        let before = source.map { $0.picture(added: added) }
        if !added.isEmpty { added = [] }
        trees += 1
        guard let next else {
            fade = nil
            return
        }
        let after = MaskPicture(next)
        guard let before, before != after else { return }
        fade = next.fade > 0 ? MaskFade(from: before, start: now, duration: next.fade) : nil
    }

    /// Ends `fade` once it is over, so the mask stops redrawing every frame.
    @MainActor
    func endFade(at now: TimeInterval = CACurrentMediaTime()) {
        if let fade, fade.isDone(at: now) { self.fade = nil }
    }
}

/// Where a masked node is drawn on screen, and its own size: they differ while an ancestor scales it, as a pan's press
/// does to the pan's children.
struct MaskGeometry: Equatable {
    var frame: CGRect = .zero
    var size: CGSize = .zero

    init(frame: CGRect = .zero, size: CGSize = .zero) {
        self.frame = frame
        self.size = size
    }

    init(_ proxy: GeometryProxy) {
        self.init(frame: proxy.frame(in: .global), size: proxy.size)
    }

    /// How much larger than its own size the node is drawn, per axis; 1 when either is empty.
    var scale: CGSize {
        CGSize(width: Self.ratio(frame.width, size.width), height: Self.ratio(frame.height, size.height))
    }

    private static func ratio(_ drawn: CGFloat, _ own: CGFloat) -> CGFloat {
        drawn > 0 && own > 0 ? drawn / own : 1
    }
}

/// What a mask's view draws from, besides its channel.
struct MaskSource {
    var mask: CustomMessageWidgetNode.Mask
    var trail: PanTrail?
    /// The pan's top-left corner from the node's, on screen.
    var place: CGPoint
    /// How much larger than its own size the node is drawn on screen.
    var scale = CGSize(width: 1, height: 1)
    var mirroredWidth: CGFloat?

    /// For a mask drawn at `geometry` that follows `trail`: the finger's places are moved into the node's own frame,
    /// so its strokes stay under the finger however an ancestor scales the node.
    @MainActor
    static func of(
        mask: CustomMessageWidgetNode.Mask, trail: PanTrail?, geometry: MaskGeometry, mirrored: Bool
    ) -> MaskSource {
        let frame = geometry.frame
        let place = trail.map { CGPoint(x: $0.origin.x - frame.minX, y: $0.origin.y - frame.minY) } ?? .zero
        return MaskSource(
            mask: mask,
            trail: trail,
            place: place,
            scale: geometry.scale,
            mirroredWidth: mirrored ? geometry.size.width : nil
        )
    }

    @MainActor
    func picture(added: [CustomMessageWidgetNode.Mask.Shape]) -> MaskPicture {
        MaskPicture(
            mask,
            added: added,
            strokes: trail?.strokes ?? [],
            place: place,
            scale: scale,
            mirroredWidth: mirroredWidth
        )
    }
}

/// The finger's strokes on one keyed location pan since the current tree, in the pan's own frame (points from its
/// top-left corner, whatever the layout direction), and where that frame is on screen, for masks that follow the pan.
/// A tree drops them, as it drops the shapes added since.
@Observable
public final class PanTrail {
    public private(set) var strokes: [[CGPoint]] = []
    /// The pan's top-left corner on screen.
    var origin: CGPoint = .zero

    public var isEmpty: Bool { strokes.isEmpty }

    /// A new stroke, from where the finger touched down.
    @MainActor
    func begin(at point: CGPoint) {
        strokes.append([point])
    }

    /// The finger reached `point` on the current stroke; with no stroke begun, it starts one.
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

/// Applies a node's mask, if it has one.
struct NodeMaskModifier: ViewModifier {
    let mask: CustomMessageWidgetNode.Mask?

    @Environment(\.gestureOffsets) private var updates

    func body(content: Content) -> some View {
        if let mask {
            let channel = mask.key.flatMap { updates?.mask(for: $0) }
            let trail = mask.follow.flatMap { updates?.trail(for: $0.pan) }
            content
                .mask { MaskLayer(mask: mask, channel: channel, trail: trail) }
                .background {
                    if let share = mask.clearedShare {
                        MaskShareWatcher(
                            share: share,
                            mask: mask,
                            channel: channel,
                            trail: trail,
                            touch: mask.follow.flatMap { updates?.touch(for: $0.pan) }
                        )
                    }
                }
        } else {
            content
        }
    }
}

/// Where the masked node shows, as the mask's alpha: the tree's shapes, those added since, the followed pan's strokes,
/// and during a fade the picture before the tree, blended away.
private struct MaskLayer: View {
    let mask: CustomMessageWidgetNode.Mask
    let channel: MaskChannel?
    let trail: PanTrail?

    @Environment(\.layoutDirection) private var layoutDirection
    @State private var geometry = MaskGeometry()

    var body: some View {
        let source = source
        let picture = source.picture(added: channel?.added ?? [])
        let mirrored = layoutDirection == .rightToLeft
        Group {
            if let fade = channel?.fade {
                TimelineView(.animation) { timeline in
                    let progress = fade.progress(at: CACurrentMediaTime())
                    Canvas { context, size in
                        context.opacity = 1 - progress
                        context.drawLayer { MaskDrawing.draw(fade.from, in: &$0, size: size, mirrored: mirrored) }
                        context.opacity = progress
                        context.blendMode = .plusLighter
                        context.drawLayer { MaskDrawing.draw(picture, in: &$0, size: size, mirrored: mirrored) }
                    }
                    .onChange(of: timeline.date) { channel?.endFade() }
                }
            } else {
                Canvas { context, size in
                    MaskDrawing.draw(picture, in: &context, size: size, mirrored: mirrored)
                }
            }
        }
        .onGeometryChange(for: MaskGeometry.self) { MaskGeometry($0) } action: { geometry = $0 }
        .onAppear { channel?.source = self.source }
        .onChange(of: geometry) { channel?.source = self.source }
        .onChange(of: mask) { channel?.source = self.source }
    }

    private var source: MaskSource {
        MaskSource.of(mask: mask, trail: trail, geometry: geometry, mirrored: layoutDirection == .rightToLeft)
    }
}

public extension CustomMessageWidgetNode {
    /// Every keyed mask in the tree, by key; of two with the same key, the first.
    var keyedMasks: [String: Mask] {
        var found: [String: Mask] = [:]
        collectMasks(into: &found)
        return found
    }

    private func collectMasks(into found: inout [String: Mask]) {
        if let mask = modifiers.mask, let key = mask.key, found[key] == nil { found[key] = mask }
        for child in children {
            child.collectMasks(into: &found)
        }
    }

    private var children: [CustomMessageWidgetNode] {
        switch content {
        case let .box(_, children),
             let .column(_, children),
             let .row(_, children),
             let .gesture(_, children):
            children
        default:
            []
        }
    }
}
