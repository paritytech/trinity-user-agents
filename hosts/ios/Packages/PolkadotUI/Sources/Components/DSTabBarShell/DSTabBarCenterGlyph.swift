import DesignSystem
import SwiftUI

// MARK: - Volume

/// Marks the top of the open panel as draggable. Fades in with the content and takes no touches,
/// so a drag there reaches the content, which moves the panel through `centerPanelDrag`.
struct DSTabBarCenterGrabber: View {
    let reveal: CGFloat

    var body: some View {
        Capsule()
            .fill(DSTabBarStyle.Palette.grabber)
            .frame(width: DSTabBarStyle.centerGrabberWidth, height: DSTabBarStyle.centerGrabberThickness)
            .padding(.top, DSTabBarStyle.centerGrabberTopInset)
            .frame(maxWidth: .infinity, alignment: .top)
            .opacity(reveal)
            .allowsHitTesting(false)
    }
}

/// Hairline along the edge. A border stroke, so the clip leaves its full width.
struct DSTabBarCenterRim: View {
    var body: some View {
        DSTabBarMorphShape()
            .strokeBorder(DSTabBarStyle.Palette.strokeGlass, lineWidth: DSTabBarStyle.strokeWidth)
    }
}

/// The scan frame, which sits in the lobe in both states, since the lobe is the button. It shrinks slightly and drops
/// a little as the panel opens. The touch area shrinks to `openTouchSize` with it, and moves with it because the
/// offset comes after the content shape. It applies the content fade itself, so a fade re-runs only this view.
struct DSTabBarCenterGlyph: View {
    let progress: CGFloat
    let contentFade: DSTabBarContentFade?

    var body: some View {
        DSTabBarScanFrameShape()
            .stroke(
                .fgSecondary,
                style: StrokeStyle(
                    lineWidth: DSTabBarScanFrameShape.strokeWidth * glyphSize / DSTabBarScanFrameShape.gridSide,
                    lineCap: .round,
                    lineJoin: .round
                )
            )
            .frame(width: glyphSize, height: glyphSize)
            .scaleEffect(lerp(1, openGlyphScale))
            .frame(width: DSTabBarStyle.centerButtonSize, height: DSTabBarStyle.centerButtonSize)
            .contentShape(Circle().inset(by: lerp(0, (DSTabBarStyle.centerButtonSize - openTouchSize) / 2)))
            .offset(y: lerp(0, openGlyphDrop))
            .opacity(fadeOpacity)
    }
}

private extension DSTabBarCenterGlyph {
    var fadeOpacity: CGFloat { contentFade?.opacity ?? 1 }

    /// Side of the square canvas the centre glyph SVG is drawn on.
    var glyphSize: CGFloat { 30 }

    /// Side of the glyph once the panel is fully open.
    var openGlyphSize: CGFloat { 22 }

    /// Scale of the glyph once the panel is fully open, which takes it to `openGlyphSize`.
    var openGlyphScale: CGFloat { openGlyphSize / glyphSize }

    /// Distance the glyph's centre sits below the panel body's bottom edge once the panel is fully open.
    var openGlyphCenterBelowBody: CGFloat { 8 }

    /// Drop that carries the glyph from its resting centre to `openGlyphCenterBelowBody` below the body. The resting
    /// centre is half a button above the panel's bottom edge, and the body's edge is a lobe depth above that bottom.
    var openGlyphDrop: CGFloat {
        openGlyphCenterBelowBody + DSTabBarStyle.centerButtonSize / 2 - DSTabBarStyle.centerLobeDepth
    }

    /// Side of the touch area once the panel is fully open. It is centred on the glyph.
    var openTouchSize: CGFloat { 48 }

    func lerp(_ from: CGFloat, _ to: CGFloat) -> CGFloat {
        from + (to - from) * progress
    }
}
