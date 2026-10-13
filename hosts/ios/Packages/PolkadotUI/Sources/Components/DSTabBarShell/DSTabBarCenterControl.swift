import DesignSystem
import SwiftUI

/// The button that morphs into a panel, sitting in the tab bar's centre hole. Hosts the caller's
/// `content`. The caller declares its open height, so the control never renders `content` a
/// second time just to measure it.
///
/// Every dimension and radius is read off `progress`, so the shape is valid at any point between
/// collapsed (0) and open (1), including mid-drag, rather than being two states with an animation
/// between them.
public struct DSTabBarCenterControl<Content: View>: View {
    @Binding private var progress: CGFloat
    private let contentHeight: CGFloat
    private let content: Content

    @State private var dragStart: CGFloat?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// `contentHeight` is the height `content` takes when laid out at the panel's open width.
    public init(
        progress: Binding<CGFloat>,
        contentHeight: CGFloat,
        @ViewBuilder content: () -> Content
    ) {
        _progress = progress
        self.contentHeight = contentHeight
        self.content = content()
    }

    public var body: some View {
        GeometryReader { proxy in
            let size = frameSize(availableWidth: proxy.size.width)
            panel(in: size, availableWidth: proxy.size.width)
                .offset(
                    x: (proxy.size.width - size.width) / 2,
                    y: proxy.size.height - size.height - seatInset
                )
        }
    }
}

// MARK: - Geometry

private extension DSTabBarCenterControl {
    /// The lobe is the button itself, so it keeps its size through the morph.
    var lobeRadius: CGFloat { DSTabBarStyle.centerButtonSize / 2 }

    /// Bottom edge of the resting notch, which the morph keeps fixed.
    var bottomFromRowTop: CGFloat { DSTabBarStyle.notchCenterY + DSTabBarStyle.notchRadius }

    /// How far the control's foot sits above the bar row's bottom edge, so the lobe rests in the
    /// notch with an even ring around it.
    var seatInset: CGFloat { DSTabBarStyle.height - bottomFromRowTop + DSTabBarStyle.notchRingGap }

    /// Progress at which content, the grabber and its hit area appear.
    var revealStart: CGFloat { 0.45 }

    /// Movement below which a drag counts as a tap.
    var tapSlop: CGFloat { 4 }

    /// Content appears over the back part of the travel, so it never shows while the panel is too
    /// small to hold it.
    var reveal: CGFloat { max(0, progress - revealStart) / (1 - revealStart) }

    /// Distance the finger travels to open the panel fully.
    var travel: CGFloat { max(1, expandedHeight - DSTabBarStyle.centerButtonSize) }

    var expandedHeight: CGFloat {
        DSTabBarStyle.centerPanelHeight(contentHeight: contentHeight)
    }

    func lerp(_ from: CGFloat, _ to: CGFloat) -> CGFloat {
        from + (to - from) * progress
    }

    func frameSize(availableWidth: CGFloat) -> CGSize {
        let expandedWidth = DSTabBarStyle.centerPanelWidth(inRowOfWidth: availableWidth)
        return CGSize(
            width: lerp(DSTabBarStyle.centerButtonSize, expandedWidth),
            height: lerp(DSTabBarStyle.centerButtonSize, expandedHeight)
        )
    }

    var shape: DSTabBarMorphShape {
        DSTabBarMorphShape(
            topRadius: lerp(lobeRadius, DSTabBarStyle.centerPanelCornerRadius),
            // Collapsed, the lobe spans the whole width and leaves no bottom
            // corners for a radius to apply to.
            bottomRadius: lerp(0, DSTabBarStyle.centerPanelCornerRadius),
            lobeRadius: lobeRadius,
            // Constant, not interpolated: the join keeps one size throughout.
            // The path's own width cap eases it in while the panel is still
            // narrow, and takes it to zero at the circle.
            filletRadius: DSTabBarStyle.centerPanelFillet
        )
    }
}

// MARK: - Body

private extension DSTabBarCenterControl {
    func panel(in size: CGSize, availableWidth: CGFloat) -> some View {
        let openWidth = DSTabBarStyle.centerPanelWidth(inRowOfWidth: availableWidth)
        return ZStack {
            if progress > 0 {
                // Fixed at the open size so it is laid out once; clipShape crops it as the panel morphs.
                content
                    .frame(width: openWidth, height: contentHeight)
                    .animation(reduceMotion ? .easeInOut(duration: 0.2) : nil) { $0.opacity(Double(reveal)) }
            }
        }
        .padding(.bottom, lobeRadius)
        .foregroundStyle(Color.fgPrimary)
        .frame(width: size.width, height: size.height, alignment: progress > 0 ? .top : .center)
        // After the frame, so each lands against the panel rather than against
        // the content, which can be shorter than the panel.
        .overlay(alignment: .bottom) { glyph }
        .overlay(alignment: .top) { grabber }
        .background(DSTabBarMaterial())
        .overlay(rim)
        .clipShape(shape)
        .shadow(color: Color.shadowSoft, radius: DSTabBarStyle.centerShadowRadius, y: DSTabBarStyle.centerShadowY)
        .contentShape(shape)
    }
}

// MARK: - Grips

private extension DSTabBarCenterControl {
    /// Sits in the lobe in both states, since the lobe is the button. Turning
    /// it 45 degrees makes the same glyph the close control.
    var glyph: some View {
        Image(systemName: "plus")
            .font(.system(size: DSTabBarStyle.centerGlyphSize, weight: .semibold))
            // Set here, not inherited: the overlay sits outside the control's
            // own foregroundStyle, so it would not pick that up.
            .foregroundStyle(Color.fgPrimary)
            .rotationEffect(.degrees(45 * progress))
            .frame(width: lobeRadius * 2, height: lobeRadius * 2)
            .contentShape(Circle())
            .gesture(drag)
    }

    /// Second grip at the top of the panel, for closing without reaching down
    /// to the lobe. Fades in with the content and drags the panel without toggling it.
    var grabber: some View {
        Capsule()
            .fill(Color.fgPrimary.opacity(DSTabBarStyle.centerGrabberOpacity))
            .frame(width: DSTabBarStyle.centerGrabberWidth, height: DSTabBarStyle.centerGrabberThickness)
            .frame(height: DSTabBarStyle.centerGrabberHeight)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .animation(reduceMotion ? .easeInOut(duration: 0.2) : nil) { $0.opacity(Double(reveal)) }
            .allowsHitTesting(progress > revealStart)
            .gesture(grabberDrag)
    }

    /// Drag up to open, down to close; a drag that barely moves is a tap.
    /// `minimumDistance: 0` so one gesture serves both and they cannot conflict.
    var drag: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .global)
            .onChanged { value in
                followDrag(translationHeight: value.translation.height)
            }
            .onEnded { value in
                let start = dragStart ?? progress
                dragStart = nil

                guard abs(value.translation.height) > tapSlop else {
                    settle(to: start > 0.5 ? 0 : 1)
                    return
                }

                // A flick decides on its own; otherwise the halfway point does.
                let projected = start - value.predictedEndTranslation.height / travel
                settle(to: projected > 0.5 ? 1 : 0)
            }
    }

    /// Drag-only gesture for the grabber. The grabber drags the panel but does not toggle it, so a
    /// tap there does nothing. The minimum distance keeps a tap from starting the drag.
    var grabberDrag: some Gesture {
        DragGesture(minimumDistance: tapSlop, coordinateSpace: .global)
            .onChanged { value in
                followDrag(translationHeight: value.translation.height)
            }
            .onEnded { value in
                let start = dragStart ?? progress
                dragStart = nil

                let projected = start - value.predictedEndTranslation.height / travel
                settle(to: projected > 0.5 ? 1 : 0)
            }
    }

    /// Moves the panel with the finger from the progress at which the drag began. Both grips measure in the
    /// global space: the grabber rides the panel's top edge, so in its own space the translation would shrink
    /// as the panel follows the finger and the panel would fight the drag.
    func followDrag(translationHeight: CGFloat) {
        let start = dragStart ?? progress
        dragStart = start
        progress = min(1, max(0, start - translationHeight / travel))
    }

    /// Under Reduce Motion the geometry snaps and only the content cross-fades, so the morph
    /// itself never plays. Dragging is never animated either way, so the control still tracks
    /// the finger.
    func settle(to target: CGFloat) {
        guard !reduceMotion else {
            progress = target
            return
        }
        withAnimation(
            .spring(
                response: DSTabBarStyle.centerSpringResponse,
                dampingFraction: DSTabBarStyle.centerSpringDamping
            )
        ) {
            progress = target
        }
    }
}

// MARK: - Volume

private extension DSTabBarCenterControl {
    /// Hairline that catches the light along the top edge and fades by the
    /// bottom, so the control sits above the bar instead of in it. A border
    /// stroke, so the clip leaves its full width.
    var rim: some View {
        shape
            .strokeBorder(
                LinearGradient(
                    colors: [
                        Color.fgPrimary.opacity(DSTabBarStyle.centerRimTopOpacity),
                        Color.fgPrimary.opacity(DSTabBarStyle.centerRimBottomOpacity)
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                ),
                lineWidth: DSTabBarStyle.strokeWidth
            )
    }
}
