import DesignSystem
import SwiftUI

/// How a slide-to-confirm control looks at a given progress, one face per variant.
struct SlideToConfirmFace: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let progress: CGFloat
    let isHinting: Bool

    var body: some View {
        switch props.variant {
        case .thumb:
            SlideToConfirmThumbFace(props: props, progress: progress, isHinting: isHinting)
        case .reveal:
            SlideToConfirmRevealFace(props: props, progress: progress)
        }
    }
}

/// A pill track, a thumb that travels it, and a label the thumb fades as it passes.
struct SlideToConfirmThumbFace: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let progress: CGFloat
    let isHinting: Bool

    @Environment(\.self) private var environment

    var body: some View {
        label
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .overlay(alignment: .leading) {
                GeometryReader { geometry in
                    thumb.offset(x: progress * max(geometry.size.width - SlideToConfirmMetrics.thumbSize, 0))
                }
                .frame(height: SlideToConfirmMetrics.thumbSize)
            }
            .padding(DSSpacings.small)
            .frame(height: SlideToConfirmMetrics.trackHeight)
            .background(Color.bgSurfaceNested, in: Capsule())
            .clipShape(Capsule())
    }
}

private extension SlideToConfirmThumbFace {
    var label: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(props.displayedText)
                .typography(.bodyLargeEmphasized)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(1)
                .modifier(SlideHintShimmer(isActive: isHinting))
            if let secondaryText = props.secondaryText {
                Text(secondaryText)
                    .typography(.bodyLarge)
                    .foregroundStyle(Color.fgTertiary)
                    .lineLimit(1)
            }
        }
        // A control drawn as slid has its thumb at the end, so the label takes the start.
        .padding(.leading, props.isHeldAtEnd ? DSSpacings.medium : thumbClearance)
        .padding(.trailing, props.isHeldAtEnd ? thumbClearance : 0)
        .opacity(props.isConfirmed ? 1 : max(0, 1 - progress / SlideToConfirmMetrics.labelGoneAt))
    }

    var thumbClearance: CGFloat {
        SlideToConfirmMetrics.thumbSize + DSSpacings.extraMedium
    }

    var thumb: some View {
        let top = (props.accent ?? .fgPrimary).resolve(in: environment)
        let bottom = top.mixed(with: Color.fgSecondary.resolve(in: environment), by: 0.3)
        return Circle()
            .fill(LinearGradient(colors: [Color(top), Color(bottom)], startPoint: .top, endPoint: .bottom))
            .frame(width: SlideToConfirmMetrics.thumbSize, height: SlideToConfirmMetrics.thumbSize)
            .overlay { SlideToConfirmGlyph(props: props, tint: .fgPrimaryInverted) }
    }
}

/// What the thumb, or a reveal's button, carries: an arrow to slide, a spinner
/// while the product works, a check once confirmed.
struct SlideToConfirmGlyph: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let tint: Color

    var body: some View {
        Group {
            if props.isConfirmed {
                Image(systemName: "checkmark")
            } else if props.isLoading {
                ProgressView().tint(tint)
            } else {
                Image(systemName: "arrow.right")
                    .flipsForRightToLeftLayoutDirection(true)
            }
        }
        .font(.system(size: 20, weight: .semibold))
        .foregroundStyle(tint)
        .frame(width: SlideToConfirmMetrics.iconSize, height: SlideToConfirmMetrics.iconSize)
    }
}

/// A light band sweeping the label while the control waits for a slide.
private struct SlideHintShimmer: ViewModifier {
    let isActive: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let sweep: TimeInterval = 2.2

    func body(content: Content) -> some View {
        if isActive, !reduceMotion {
            content.overlay {
                TimelineView(.animation) { timeline in
                    GeometryReader { geometry in
                        band(width: geometry.size.width, phase: phase(at: timeline.date))
                    }
                }
                .mask(content)
                .allowsHitTesting(false)
            }
        } else {
            content
        }
    }

    private func phase(at date: Date) -> CGFloat {
        CGFloat(date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: sweep) / sweep)
    }

    private func band(width: CGFloat, phase: CGFloat) -> some View {
        let bandWidth = width * 0.4
        return LinearGradient(
            colors: [.clear, Color.fgStaticWhite.opacity(0.7), .clear],
            startPoint: .leading,
            endPoint: .trailing
        )
        .frame(width: bandWidth)
        .offset(x: -bandWidth + (width + bandWidth) * phase)
    }
}

extension Color.Resolved {
    /// Mixes towards `other` in linear light, as Compose's `lerp` does on Android.
    func mixed(with other: Color.Resolved, by fraction: Float) -> Color.Resolved {
        func mix(_ from: Float, _ to: Float) -> Float { from + (to - from) * fraction }
        return Color.Resolved(
            colorSpace: .sRGBLinear,
            red: mix(linearRed, other.linearRed),
            green: mix(linearGreen, other.linearGreen),
            blue: mix(linearBlue, other.linearBlue),
            opacity: mix(opacity, other.opacity)
        )
    }

    var luminance: Float {
        0.2126 * linearRed + 0.7152 * linearGreen + 0.0722 * linearBlue
    }
}
