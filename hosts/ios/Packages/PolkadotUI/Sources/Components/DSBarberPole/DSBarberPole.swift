import DesignSystem
import SwiftUI

/// Diagonal stripes sliding rightwards, for depicting work that is still in progress.
///
/// Rightwards with the stripes leaning right, which is the way a barber's pole turns: the pattern
/// reads as travelling up and away rather than back on itself.
///
/// The pattern repeats every `stripeWidth * 2`, so sliding by exactly one period and wrapping back
/// is seamless: at the end of a period the layer is drawing exactly what it drew at the start.
///
/// The offset is derived from the timeline's own clock rather than from a latched piece of state
/// driving a `repeatForever` animation. A repeating animation is armed once, when its `value`
/// changes, and nothing can re-arm it afterwards — so anything that tears down the running
/// animation, such as a layout pass that rebuilds the animated subtree, parks the stripes at the
/// end of their travel for good. Reading the phase from the clock cannot get stuck, and it drops
/// the state entirely.
///
/// Two things have to hold for the wrap to be invisible, and both are about coverage rather than
/// timing: the drawn pattern must extend past the layer's own bounds — a leaning stripe's far
/// corner is clipped, so drawing only to the edge leaves a wedge of bare background — and the
/// layer must be wide enough to still cover the viewport after a full period of travel.
public struct DSBarberPole: View {
    private let stripeWidth: CGFloat
    private let slant: CGFloat
    private let periodDuration: Double
    private let stripeColor: Color
    private let backgroundColor: Color
    private let isAnimated: Bool

    /// - Parameters:
    ///   - slant: Horizontal run per unit of height — the stripes' lean.
    ///   - isAnimated: Pass `false` for a still pattern. Worth doing wherever the pole is a few
    ///     points across, such as a legend key, where the sliding reads as jitter rather than
    ///     progress.
    public init(
        stripeWidth: CGFloat = 5,
        slant: CGFloat = 0.7,
        periodDuration: Double = 0.6,
        stripeColor: Color = .fgError,
        backgroundColor: Color = .fgStaticWhite,
        isAnimated: Bool = true
    ) {
        self.stripeWidth = stripeWidth
        self.slant = slant
        self.periodDuration = periodDuration
        self.stripeColor = stripeColor
        self.backgroundColor = backgroundColor
        self.isAnimated = isAnimated
    }

    public var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let period = Self.period(forStripeWidth: stripeWidth)
            let margin = period + size.height * slant

            if isAnimated {
                TimelineView(.animation) { timeline in
                    stripes(
                        in: size,
                        margin: margin,
                        // Counting the phase down rather than up walks the same interval the
                        // other way, so the stripes travel rightwards over identical coverage.
                        offset: -margin - period * (1 - Self.phase(
                            at: timeline.date,
                            periodDuration: periodDuration
                        ))
                    )
                }
            } else {
                stripes(in: size, margin: margin, offset: -margin)
            }
        }
    }
}

private extension DSBarberPole {
    /// The pattern at one fixed offset. Held apart from ``body`` so the animated and still cases
    /// differ only in where the offset comes from.
    func stripes(in size: CGSize, margin: CGFloat, offset: CGFloat) -> some View {
        ZStack(alignment: .leading) {
            backgroundColor

            Canvas { context, canvasSize in
                Self.drawStripes(
                    in: &context,
                    size: canvasSize,
                    stripeWidth: stripeWidth,
                    slant: slant,
                    color: stripeColor
                )
            }
            .frame(width: size.width + margin * 2, height: size.height)
            .offset(x: offset)
        }
        // Leading, not the default centre: the stripe layer is deliberately wider than the frame,
        // and centring it would shift it half its overdraw before the offset above has applied —
        // which starves the right edge partway through each slide.
        .frame(width: size.width, height: size.height, alignment: .leading)
        .clipped()
    }
}

private extension DSBarberPole {
    static func period(forStripeWidth stripeWidth: CGFloat) -> CGFloat {
        stripeWidth * 2
    }

    /// How far through one period the pattern is, in `0..<1`. Anchored to the clock rather than to
    /// when the view appeared, so every pole on screen slides in step and a pole that is rebuilt
    /// picks up where its neighbours are instead of restarting.
    static func phase(at date: Date, periodDuration: Double) -> CGFloat {
        let periods = date.timeIntervalSinceReferenceDate / periodDuration
        return CGFloat(periods - periods.rounded(.down))
    }

    /// Fills the canvas with stripes, starting and ending a full stripe-and-lean beyond its
    /// bounds. The clip then removes only overshoot, so every point inside is patterned — no
    /// wedge is left where a leaning stripe crosses an edge.
    ///
    /// The pattern's phase is anchored to the canvas, not to the animation, so offsetting the
    /// whole canvas by one period reproduces an identical pattern.
    static func drawStripes(
        in context: inout GraphicsContext,
        size: CGSize,
        stripeWidth: CGFloat,
        slant: CGFloat,
        color: Color
    ) {
        let period = period(forStripeWidth: stripeWidth)
        let lean = size.height * slant
        let overshoot = lean + period

        var origin = -overshoot

        while origin < size.width + overshoot {
            context.fill(
                stripe(at: origin, height: size.height, stripeWidth: stripeWidth, lean: lean),
                with: .color(color)
            )
            origin += period
        }
    }

    static func stripe(
        at origin: CGFloat,
        height: CGFloat,
        stripeWidth: CGFloat,
        lean: CGFloat
    ) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: origin, y: height))
        path.addLine(to: CGPoint(x: origin + lean, y: 0))
        path.addLine(to: CGPoint(x: origin + lean + stripeWidth, y: 0))
        path.addLine(to: CGPoint(x: origin + stripeWidth, y: height))
        path.closeSubpath()

        return path
    }
}

#if DEBUG
    #Preview("DSBarberPole") {
        VStack(spacing: 16) {
            DSBarberPole()
                .frame(width: 200, height: 20)
                .clipShape(Capsule())

            DSBarberPole(stripeWidth: 10, slant: 0, periodDuration: 1.2)
                .frame(width: 200, height: 20)
                .clipShape(Capsule())
        }
        .padding()
        .background(Color.bgSurfaceContainer)
    }
#endif
