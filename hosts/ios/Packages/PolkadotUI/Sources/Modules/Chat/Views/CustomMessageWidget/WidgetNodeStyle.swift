import SwiftUI
import UIKit

// The modifiers that style a node by value (renderer modifiers 12 to 19): each draws what the CSS it copies draws.

// MARK: - Shadow

/// CSS `box-shadow`: the node's shape, moved and blurred, under the node and taking no room. CSS's blur length is twice
/// the Gaussian's deviation, as is SwiftUI's shadow radius doubled, so the radius is half the blur.
struct NodeShadowModifier: ViewModifier {
    let shadow: CustomMessageWidgetNode.Shadow?
    let background: CustomMessageWidgetNode.Background?

    func body(content: Content) -> some View {
        if let shadow {
            let shape = background?.shape ?? AnyShape(Rectangle())
            content.background(
                shape
                    .fill(background?.color ?? shadow.color)
                    .shadow(
                        color: shadow.color,
                        radius: shadow.blur / 2,
                        x: shadow.offset.width,
                        y: shadow.offset.height
                    )
            )
        } else {
            content
        }
    }
}

// MARK: - Fade with a pan

/// Where a keyed gesture's children stand along its travel, for the nodes that fade with it. Kept by the gesture view
/// (its own drag, and where the worker's tree drew its first child) and read with the worker's offset for the key.
@Observable
public final class GesturePanPlace {
    /// The host's own drag offset along the axis.
    public var drag: CGFloat = 0
    /// Where the worker's tree drew the children: the first child's leading margin.
    public var base: CGFloat = 0
    /// How far the drawn children can travel.
    public var travel: CGFloat = 0

    public init() {}
}

/// `1 - rate * progress`, clamped, where progress follows the children however they move. Animatable on progress, so a
/// spring or ease that moves the children moves the fade with them, frame by frame, through the clamp.
struct PanFadeModifier: ViewModifier {
    let fade: CustomMessageWidgetNode.PanFade?

    @Environment(\.gestureOffsets) private var gestureOffsets

    func body(content: Content) -> some View {
        if let fade, let gestureOffsets {
            content.modifier(PanFadeOpacity(progress: progress(fade.key, gestureOffsets), rate: fade.rate))
        } else {
            content
        }
    }

    private func progress(_ key: String, _ offsets: GestureOffsets) -> CGFloat {
        let place = offsets.place(for: key)
        guard place.travel > 0 else { return 0 }
        let worker = offsets.channel(for: key).offset?.x ?? 0
        return (place.base + place.drag + worker) / place.travel
    }
}

struct PanFadeOpacity: ViewModifier, Animatable {
    var progress: CGFloat
    let rate: CGFloat

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    func body(content: Content) -> some View {
        content.opacity(PanFadeOpacity.opacity(progress: progress, rate: rate))
    }

    static func opacity(progress: CGFloat, rate: CGFloat) -> CGFloat {
        min(max(1 - rate * min(max(progress, 0), 1), 0), 1)
    }
}

// MARK: - Spin

/// One turn every `period`, about the centre, while shown; still where the person asked for less motion.
struct NodeSpinModifier: ViewModifier {
    let period: TimeInterval?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        if let period, !reduceMotion {
            TimelineView(.animation) { timeline in
                let turn = timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period) / period
                content.rotationEffect(.degrees(turn * 360))
            }
        } else {
            content
        }
    }
}

// MARK: - Exit fade

/// A node a tree dropped stays as it last drew and fades out with CSS's `ease`.
struct NodeExitFadeModifier: ViewModifier {
    let duration: TimeInterval?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        if let duration, duration > 0, !reduceMotion {
            content.transition(
                .asymmetric(insertion: .identity, removal: .opacity)
                    .animation(.timingCurve(0.25, 0.1, 0.25, 1, duration: duration))
            )
        } else {
            content
        }
    }
}

// MARK: - Type by value

extension CustomMessageWidgetNode.TextFont {
    /// The design system's Inter at the weight asked for: the nearest of the cuts the app carries.
    var font: Font {
        let name =
            switch weight {
            case ..<450: "Inter-Regular"
            case ..<550: "Inter-Medium"
            default: "Inter-SemiBold"
            }
        return .custom(name, fixedSize: size)
    }
}

/// A line of text set as a CSS line box: `lineHeight` tall, the glyphs centred in it, an ellipsis when too long.
struct ValueFontText: View {
    let text: String
    let font: CustomMessageWidgetNode.TextFont
    let shimmer: CustomMessageWidgetNode.Shimmer?

    var body: some View {
        line
            .modifier(TextShimmer(shimmer: shimmer, mask: line))
            .frame(height: font.lineHeight)
    }

    private var line: some View {
        Text(text)
            .font(font.font)
            .tracking(font.letterSpacing)
            .foregroundStyle(font.color)
            .lineLimit(1)
            .truncationMode(.tail)
    }
}

// MARK: - Shimmer

/// A band of light across the text's glyphs only: from `lead` before the text to `lead` past it at `speed`, then rest.
struct TextShimmer<Mask: View>: ViewModifier {
    let shimmer: CustomMessageWidgetNode.Shimmer?
    let mask: Mask

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()

    func body(content: Content) -> some View {
        if let shimmer, !reduceMotion {
            content.overlay {
                TimelineView(.animation) { timeline in
                    GeometryReader { geometry in
                        let center = TextShimmer.center(
                            shimmer,
                            width: geometry.size.width,
                            elapsed: timeline.date.timeIntervalSince(start)
                        )
                        if let center {
                            LinearGradient(stops: TextShimmer.stops(shimmer), startPoint: .leading, endPoint: .trailing)
                                .frame(width: shimmer.band, height: geometry.size.height)
                                .offset(x: center - shimmer.band / 2)
                        }
                    }
                }
                .mask(mask)
                .allowsHitTesting(false)
            }
        } else {
            content
        }
    }

    static func stops(_ shimmer: CustomMessageWidgetNode.Shimmer) -> [Gradient.Stop] {
        let last = max(shimmer.stops.count - 1, 1)
        return shimmer.stops.enumerated().map { index, alpha in
            Gradient.Stop(color: shimmer.color.opacity(alpha), location: CGFloat(index) / CGFloat(last))
        }
    }

    /// The band's centre from the text's start, or nil while it rests.
    static func center(_ shimmer: CustomMessageWidgetNode.Shimmer, width: CGFloat, elapsed: TimeInterval) -> CGFloat? {
        guard shimmer.speed > 0 else { return nil }
        let sweep = (width + 2 * shimmer.lead) / shimmer.speed
        let period = sweep + shimmer.rest
        var time = elapsed.truncatingRemainder(dividingBy: period)
        if time < 0 { time += period }
        return time < sweep ? -shimmer.lead + shimmer.speed * time : nil
    }
}

// MARK: - Feel

extension CustomMessageWidgetNode.PanFeel {
    /// A quadratic ease-out, `1 - (1 - u)^2`, which is exactly this cubic Bézier.
    static func easeOutQuad(_ duration: TimeInterval) -> Animation {
        .timingCurve(1.0 / 3, 2.0 / 3, 2.0 / 3, 1, duration: duration)
    }

    var press: Animation { Self.easeOutQuad(pressDuration) }
    var unpress: Animation { Self.easeOutQuad(unpressDuration) }

    /// The damped spring home, from rest: SwiftUI's response is one undamped period.
    var springHome: Animation {
        .spring(response: 2 * .pi / max(springOmega, 0.01), dampingFraction: springDamping)
    }

    /// How long the way to the end takes: the approach, longer over a longer way.
    func approach(distance: CGFloat) -> TimeInterval {
        approachDuration * max(1, sqrt(max(distance, 1) / approachReference))
    }

    var popPeak: CGFloat { pop.max() ?? 1 }

    /// The pop at `frames` sixtieths after it starts, from `start` (the scale the children had): the scale, and how far
    /// past the end they stand. Nil once it is over.
    func pop(at frames: Double, from start: CGFloat) -> (scale: CGFloat, past: CGFloat)? {
        guard pop.count > 1 else { return nil }
        if frames >= Double(pop.count - 1) { return nil }
        let scale: CGFloat
        if frames <= 0 {
            scale = start
        } else {
            let index = Int(frames), fraction = CGFloat(frames - Double(index))
            let from = index == 0 ? start : pop[index], to = pop[index + 1]
            scale = from + (to - from) * fraction
        }
        let peak = popPeak
        let past = scale > 1 && peak > 1 ? popOffset * pow(min(max((scale - 1) / (peak - 1), 0), 1), 1.5) : 0
        return (scale, past)
    }
}
