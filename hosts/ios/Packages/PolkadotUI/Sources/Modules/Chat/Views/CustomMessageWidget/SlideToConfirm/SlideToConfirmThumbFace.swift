import DesignSystem
import SwiftUI

/// How a slide-to-confirm control looks at a given progress, one face per variant.
struct SlideToConfirmFace: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let progress: CGFloat
    let isHinting: Bool
    var trackHeight: CGFloat = SlideToConfirmMetrics.trackHeight
    /// The thumb's scale (pressed, popping) and how far past the end the pop stands it.
    var thumbScale: CGFloat = 1
    var thumbPast: CGFloat = 0

    var body: some View {
        switch props.variant {
        case .thumb:
            SlideToConfirmThumbFace(
                props: props,
                progress: progress,
                isHinting: isHinting,
                trackHeight: trackHeight,
                thumbScale: thumbScale,
                thumbPast: thumbPast
            )
        case .reveal:
            SlideToConfirmRevealFace(props: props, progress: progress)
        }
    }
}

/// A pill track, a thumb that travels it, and a label the thumb fades as it passes, drawn as the existing slider is
/// (`SlideStyle`).
struct SlideToConfirmThumbFace: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let progress: CGFloat
    let isHinting: Bool
    /// The product's height for the control, or the track's own; the thumb stays centred in it.
    var trackHeight: CGFloat = SlideToConfirmMetrics.trackHeight
    var thumbScale: CGFloat = 1
    var thumbPast: CGFloat = 0

    @Environment(\.self) private var environment

    var body: some View {
        let inset = SlideToConfirmMetrics.thumbInset(trackHeight: trackHeight)
        label(inset: inset)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .overlay(alignment: .leading) {
                GeometryReader { geometry in
                    thumb
                        .scaleEffect(thumbScale)
                        .offset(x: progress * SlideToConfirmMetrics.thumbTravel(laneWidth: geometry.size.width) + thumbPast)
                }
                .frame(height: SlideToConfirmMetrics.thumbSize)
            }
            .padding(inset)
            .frame(height: trackHeight)
            // The existing slider's pill clips nothing: the thumb's shadow and its pop stand over the track's edge.
            .background(SlideStyle.track, in: Capsule())
    }
}

private extension SlideToConfirmThumbFace {
    /// `#text`: two lines from 84 to 16 short of the pill's ends, centred under a 1-point padding, 4.6 apart; the
    /// existing slider fades them with the thumb's progress, so a control held at the end shows none.
    func label(inset: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: SlideStyle.gap) {
            ValueFontText(text: props.displayedText, font: SlideStyle.line1Font, shimmer: isHinting ? SlideStyle.shimmer : nil)
            if let secondaryText = props.secondaryText, !secondaryText.isEmpty {
                ValueFontText(text: secondaryText, font: SlideStyle.line2Font, shimmer: nil)
            }
        }
        .padding(.top, SlideStyle.textPaddingTop)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(.leading, SlideStyle.textStart - inset)
        .padding(.trailing, SlideStyle.textEnd - inset)
        .opacity(SlideStyle.textOpacity(progress: props.isHeldAtEnd ? 1 : progress))
    }

    /// `#thumb`: 56, round, `--thumb` (or the product's accent) with its shadow; the chevron, or the spinner while
    /// the product works.
    var thumb: some View {
        let fill = props.accent ?? SlideStyle.thumb
        return Circle()
            .fill(fill)
            .background(
                Circle()
                    .fill(fill)
                    .shadow(
                        color: SlideStyle.shadow.color,
                        radius: SlideStyle.shadow.blur / 2,
                        x: SlideStyle.shadow.x,
                        y: SlideStyle.shadow.y
                    )
            )
            .frame(width: SlideToConfirmMetrics.thumbSize, height: SlideToConfirmMetrics.thumbSize)
            .overlay {
                if props.isLoading {
                    SlideSpinner(tint: glyphTint(on: fill))
                } else {
                    SlideChevron()
                        .stroke(glyphTint(on: fill), style: StrokeStyle(lineWidth: 2.6, lineCap: .round, lineJoin: .round))
                        .flipsForRightToLeftLayoutDirection(true)
                }
            }
    }

    /// The existing slider's rule for an accent: a dark chevron on a light thumb, a white one on a dark thumb.
    func glyphTint(on fill: Color) -> Color {
        fill.resolve(in: environment).luminance > 0.55 ? SlideStyle.chevron : .white
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
