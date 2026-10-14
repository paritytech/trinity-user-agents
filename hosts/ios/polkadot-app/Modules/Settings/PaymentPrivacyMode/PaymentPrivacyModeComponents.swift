import SwiftUI
import UIKit
import DesignSystem
import Coinage

// Presentation pieces for `PaymentPrivacyModeCard`: the layout constants, the lit sphere, the stepped
// scale, and the accent-derived shading. Split from the card so each stays a small, focused unit.

// MARK: - Layout constants

/// Geometry from the Settings mock-up (Figma node 758:2203), in points.
enum PrivacyModeMetrics {
    static let circle: CGFloat = 28
    static let selectedCircle: CGFloat = 50
    static let trackHeight: CGFloat = 40
    static let glowBlur: CGFloat = 12
    static let glyphSize: CGFloat = 16
    static let selectedGlyphSize: CGFloat = 28
    static let tickWidth: CGFloat = 2
    static let tickHeight: CGFloat = 6
    static let tickStep: CGFloat = 8
    static let selectedRing: CGFloat = 62
    static let ringCornerRadius: CGFloat = 24
    static let ringWidth: CGFloat = 2
    static let glowOpacity: Double = 0.5
    static let boxHeight: CGFloat = 78
    static let headerMinHeight: CGFloat = 52
    static let inset: CGFloat = 20
    static let selectionAnimation: Animation = .easeInOut(duration: 0.2)
    static let slowDragFade: Double = 0.28
    static let fastDragFade: Double = 0.14
    static let tapFade: Double = fastDragFade
    static let fastDragSpeed: CGFloat = 3

    /// How much of the recess shadow a fully lit floor removes; a dark floor keeps nearly all of it.
    static let lightSurfaceFalloff: CGFloat = 0.8
}

// MARK: - Mode circle

enum ModeCircleState {
    case resting
    case grown
    case settled
}

struct ModeCircleView: View {
    let mode: RecyclingStrategyType
    let state: ModeCircleState

    var body: some View {
        let grown = state != .resting
        let diameter = grown ? PrivacyModeMetrics.selectedCircle : PrivacyModeMetrics.circle
        let accent = mode.displayAccentColor

        ZStack {
            RoundedRectangle(cornerRadius: mode.glowCornerRadius, style: .circular)
                .fill(accent)
                .frame(width: mode.glowSize, height: mode.glowSize)
                .blur(radius: PrivacyModeMetrics.glowBlur)
                .opacity(state == .settled ? PrivacyModeMetrics.glowOpacity : 0)

            Circle()
                .fill(mode.circleGradient(accent: accent, selected: grown))
                .overlay(Circle().strokeBorder(mode.circleBorderGradient(accent: accent), lineWidth: 1))
                .frame(width: diameter, height: diameter)
                .shadow(color: .shadowMedium.opacity(0.7), radius: 2, y: 4)
                .overlay(glyph(size: grown ? PrivacyModeMetrics.selectedGlyphSize : PrivacyModeMetrics.glyphSize))
        }
        .frame(width: PrivacyModeMetrics.boxHeight, height: PrivacyModeMetrics.boxHeight)
        .animation(PrivacyModeMetrics.selectionAnimation, value: state)
    }

    private func glyph(size: CGFloat) -> some View {
        Image(systemName: mode.displayIconName)
            .resizable()
            .scaledToFit()
            .fontWeight(.semibold)
            .foregroundStyle(.fgStaticWhite)
            .frame(width: size, height: size)
    }
}

// MARK: - Selection ring

struct SelectionRingView: View {
    let mode: RecyclingStrategyType
    let fadeDuration: Double

    @State private var fadeFrom: Color
    @State private var fadeTo: RecyclingStrategyType
    @State private var fadeProgress: CGFloat = 1

    init(mode: RecyclingStrategyType, fadeDuration: Double) {
        self.mode = mode
        self.fadeDuration = fadeDuration
        _fadeFrom = State(initialValue: mode.displayAccentColor)
        _fadeTo = State(initialValue: mode)
    }

    var body: some View {
        BlendedRing(source: fadeFrom, target: fadeTo.displayAccentColor, progress: fadeProgress)
            .frame(width: PrivacyModeMetrics.boxHeight, height: PrivacyModeMetrics.boxHeight)
            .onChange(of: mode) { _, newMode in
                guard newMode != fadeTo else { return }

                fadeFrom = fadeFrom.blended(with: fadeTo.displayAccentColor, fraction: fadeProgress)
                fadeTo = newMode
                fadeProgress = 0

                withAnimation(.linear(duration: fadeDuration)) { fadeProgress = 1 }
            }
    }
}

private struct BlendedRing: View, Animatable {
    let source: Color
    let target: Color
    var progress: CGFloat

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    var body: some View {
        RoundedRectangle(cornerRadius: PrivacyModeMetrics.ringCornerRadius, style: .circular)
            .strokeBorder(source.blended(with: target, fraction: progress), lineWidth: PrivacyModeMetrics.ringWidth)
            .frame(width: PrivacyModeMetrics.selectedRing, height: PrivacyModeMetrics.selectedRing)
            .shadow(color: .shadowMedium.opacity(0.7), radius: 2, y: 4)
    }
}

// MARK: - Tick scale

/// The stepped speed-to-privacy scale cut into the groove: upright bars coloured by a horizontal gradient
/// across the mode accents, spanning centre-to-centre of the outer modes.
struct TickScale: View {
    let start: CGFloat
    let end: CGFloat

    var body: some View {
        Canvas { context, size in
            var path = Path()
            let top = size.height / 2 - PrivacyModeMetrics.tickHeight / 2
            var tickX = start
            while tickX + PrivacyModeMetrics.tickWidth <= end {
                path.addRect(CGRect(
                    x: tickX,
                    y: top,
                    width: PrivacyModeMetrics.tickWidth,
                    height: PrivacyModeMetrics.tickHeight
                ))
                tickX += PrivacyModeMetrics.tickStep
            }
            context.fill(
                path,
                with: .linearGradient(
                    Gradient(colors: [.fgWarning, .fgSuccess, .avatarBgSapphire, .avatarBgAmethyst]),
                    startPoint: CGPoint(x: start, y: 0),
                    endPoint: CGPoint(x: end, y: 0)
                )
            )
        }
    }
}

// MARK: - Mode appearance

/// Per-mode shades derived from the mode's flat accent token rather than hardcoded, so the lit sphere stays
/// self-consistent: the fill is a vertical gradient, tinted towards static white at the top when selected
/// and shaded towards onyx below; the rim shares the same derivation.
private extension RecyclingStrategyType {
    var glowSize: CGFloat {
        self == .balanced ? 40 : 56
    }

    var glowCornerRadius: CGFloat {
        self == .balanced ? 20 : 24
    }

    func circleGradient(accent: Color, selected: Bool) -> LinearGradient {
        let top = selected
            ? accent.blended(with: .fgStaticWhite, fraction: 0.1)
            : accent.blended(with: .avatarBgOnyx, fraction: 0.25)
        let bottom = selected
            ? accent.blended(with: .avatarBgOnyx, fraction: 0.35)
            : accent.blended(with: .avatarBgOnyx, fraction: 0.55)
        return LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom)
    }

    func circleBorderGradient(accent: Color) -> LinearGradient {
        LinearGradient(
            colors: [
                accent.blended(with: .fgStaticWhite, fraction: 0.4),
                accent.blended(with: .avatarBgOnyx, fraction: 0.55)
            ],
            startPoint: .top,
            endPoint: .bottom
        )
    }
}

// MARK: - Colour blending

extension Color {
    /// Linear RGBA blend towards `other`. The privacy card's shades are derived from a single accent token,
    /// which needs mixing the design system does not expose. Kept **dynamic**: both inputs are re-resolved
    /// against the render-time traits, so the blend follows the active design-system theme (the `appTheme`
    /// trait each token reads) instead of being frozen to one appearance.
    func blended(with other: Color, fraction: CGFloat) -> Color {
        let clamped = min(max(fraction, 0), 1)
        let base = UIColor(self)
        let target = UIColor(other)

        return Color(uiColor: UIColor { traits in
            let resolvedBase = base.resolvedColor(with: traits)
            let resolvedTarget = target.resolvedColor(with: traits)

            var baseRed: CGFloat = 0, baseGreen: CGFloat = 0, baseBlue: CGFloat = 0, baseAlpha: CGFloat = 0
            var tintRed: CGFloat = 0, tintGreen: CGFloat = 0, tintBlue: CGFloat = 0, tintAlpha: CGFloat = 0
            resolvedBase.getRed(&baseRed, green: &baseGreen, blue: &baseBlue, alpha: &baseAlpha)
            resolvedTarget.getRed(&tintRed, green: &tintGreen, blue: &tintBlue, alpha: &tintAlpha)

            return UIColor(
                red: baseRed + (tintRed - baseRed) * clamped,
                green: baseGreen + (tintGreen - baseGreen) * clamped,
                blue: baseBlue + (tintBlue - baseBlue) * clamped,
                alpha: baseAlpha + (tintAlpha - baseAlpha) * clamped
            )
        })
    }
}
