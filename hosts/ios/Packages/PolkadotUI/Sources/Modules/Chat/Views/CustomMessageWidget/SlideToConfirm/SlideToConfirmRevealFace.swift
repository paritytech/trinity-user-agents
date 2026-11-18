import DesignSystem
import SwiftUI

/// A face peeled away from the leading edge as the user slides, rolling up as
/// it goes and uncovering the accent surface and the confirmed text beneath.
struct SlideToConfirmRevealFace: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let progress: CGFloat

    @Environment(\.self) private var environment

    var body: some View {
        let palette = RevealPalette(accent: props.accent ?? .fgWarning, environment: environment)
        ZStack {
            uncovered(palette)
            if !props.isHeldAtEnd {
                GeometryReader { geometry in
                    let peel = RevealPeel(progress: progress, width: geometry.size.width)
                    face(palette)
                        .mask(alignment: .leading) { Rectangle().padding(.leading, peel.rollEnd) }
                    roll(peel, height: geometry.size.height)
                }
            }
        }
        .frame(height: SlideToConfirmMetrics.revealHeight)
        .background(palette.fill, in: shape)
        .overlay { dashedOutline(palette.deep, inset: RevealMetrics.outlineInset) }
        .clipShape(shape)
        .shadow(color: Color(palette.accent).opacity(0.5), radius: RevealMetrics.glowRadius)
    }
}

private extension SlideToConfirmRevealFace {
    var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: DSRadii.large)
    }

    func uncovered(_ palette: RevealPalette) -> some View {
        HStack(spacing: 0) {
            Text(props.confirmedText ?? props.text)
                .typography(.titleSmallEmphasized)
                .foregroundStyle(Color(palette.onAccent))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            Group {
                if props.isHeldAtEnd {
                    SlideToConfirmGlyph(props: props, tint: Color(palette.onAccent))
                }
            }
            .frame(width: SlideToConfirmMetrics.thumbSize, height: SlideToConfirmMetrics.thumbSize)
        }
        .padding(.horizontal, DSSpacings.mediumIncreased + RevealMetrics.border)
    }

    func face(_ palette: RevealPalette) -> some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 0) {
                Text(props.text)
                    .typography(.bodyMediumEmphasized)
                    .lineLimit(1)
                if let secondaryText = props.secondaryText {
                    Text(secondaryText)
                        .typography(.titleSmallEmphasized)
                        .lineLimit(1)
                }
            }
            .foregroundStyle(Color.fgTertiary)
            .frame(maxWidth: .infinity, alignment: .leading)
            Circle()
                .fill(palette.fill)
                .frame(width: SlideToConfirmMetrics.thumbSize, height: SlideToConfirmMetrics.thumbSize)
                .overlay { SlideToConfirmGlyph(props: props, tint: Color(palette.onAccent)) }
        }
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            Color.bgSurfaceNested,
            in: RoundedRectangle(cornerRadius: DSRadii.large - RevealMetrics.border)
        )
        .overlay {
            dashedOutline(Color.fgTertiary.opacity(0.5), inset: RevealMetrics.outlineInset - RevealMetrics.border)
        }
        .padding(RevealMetrics.border)
    }

    /// The rolled-up part of the face, with the shade it casts on what it uncovered.
    func roll(_ peel: RevealPeel, height: CGFloat) -> some View {
        let rollHeight = height - 2 * RevealMetrics.border
        let surface = Color.bgSurfaceNested.resolve(in: environment)
        let lift = Color.fgPrimary.resolve(in: environment)
        return ZStack(alignment: .topLeading) {
            if peel.rollWidth > 0.5 {
                LinearGradient(colors: [.clear, .black.opacity(0.35)], startPoint: .leading, endPoint: .trailing)
                    .frame(width: RevealMetrics.shadeWidth, height: rollHeight)
                    .offset(x: peel.start - RevealMetrics.shadeWidth)
                RoundedRectangle(cornerRadius: min(DSRadii.large - RevealMetrics.border, peel.rollWidth / 2))
                    .fill(LinearGradient(
                        colors: [Color(surface.mixed(with: lift, by: 0.3)), Color(surface.mixed(with: lift, by: 0.06))],
                        startPoint: .leading,
                        endPoint: .trailing
                    ))
                    .frame(width: peel.rollWidth, height: rollHeight)
                    .offset(x: peel.start)
            }
        }
        .offset(y: RevealMetrics.border)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .scaleEffect(x: environment.layoutDirection == .rightToLeft ? -1 : 1)
        .environment(\.layoutDirection, .leftToRight)
        .allowsHitTesting(false)
    }

    func dashedOutline(_ color: Color, inset: CGFloat) -> some View {
        RoundedRectangle(cornerRadius: DSRadii.large - inset)
            .inset(by: inset)
            .stroke(color, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
            .allowsHitTesting(false)
    }
}

/// Where the peel stands across a face `width` wide, as the Android face computes it.
struct RevealPeel {
    let start: CGFloat
    let rollEnd: CGFloat

    init(progress: CGFloat, width: CGFloat) {
        let clamped = min(max(progress, 0), 1)
        let inset = RevealMetrics.border
        let span = max(width - 2 * inset, 0)
        let minWidth = RevealMetrics.rollMinWidth
        start = inset + clamped * span
        let gathered = minWidth + (span / 3 - minWidth) * clamped
        let emerged = min(clamped / 0.04, 1)
        rollEnd = min(start + gathered * emerged, width - inset)
    }

    var rollWidth: CGFloat {
        rollEnd - start
    }
}

private struct RevealPalette {
    let accent: Color.Resolved
    let onAccent: Color.Resolved
    let deep: Color

    var fill: LinearGradient {
        LinearGradient(colors: [Color(accent), deep], startPoint: .top, endPoint: .bottom)
    }

    init(accent accentColor: Color, environment: EnvironmentValues) {
        accent = accentColor.resolve(in: environment)
        let primary = Color.fgPrimary.resolve(in: environment)
        let inverted = Color.fgPrimaryInverted.resolve(in: environment)
        // The text colour further from the accent in luminance reads on it.
        onAccent = abs(primary.luminance - accent.luminance) >= abs(inverted.luminance - accent.luminance)
            ? primary
            : inverted
        deep = Color(accent.mixed(with: onAccent, by: 0.3))
    }
}

private enum RevealMetrics {
    static let border: CGFloat = 2
    static let outlineInset: CGFloat = 8
    static let glowRadius: CGFloat = 12
    static let rollMinWidth: CGFloat = 28
    static let shadeWidth: CGFloat = 8
}
