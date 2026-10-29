import DesignSystem
import SwiftUI

/// The button that morphs into a panel, sitting in the tab bar's centre hole. Hosts the caller's
/// `content`. The caller declares its open height, so the control never renders `content` a
/// second time just to measure it.
///
/// Every dimension and radius is read off `progress`, so the shape is valid at any point between
/// collapsed (0) and open (1), including mid-drag, rather than being two states with an animation
/// between them. A drag past open adds a linear `stretch` to the height.
public struct DSTabBarCenterControl<Content: View>: View {
    @Binding private var progress: CGFloat
    private let contentHeight: CGFloat
    private let accessibilityLabel: String
    private let openAccessibilityLabel: String
    private let onSettled: (Bool) -> Void
    private let onCloseRequested: ((TimeInterval?) -> Void)?
    private let onSwipeOpenBegan: (() -> Void)?
    private let onSwipeOpenLanding: (() -> Void)?
    private let content: Content

    @State private var dragStart: CGFloat?
    /// Extra height in points above the fully open panel while a drag pulls past it.
    @State private var stretch: CGFloat = 0
    /// Opacity of the content and grabber, 0 hidden to 1 shown. A drag sets it from progress; a settle animates it.
    @State private var reveal: CGFloat = 0
    /// Whether `content` is in the hierarchy. It is added without animation before the panel opens, so an animated
    /// open never animates the content's first layout, and removed once the panel is closed and the content faded.
    @State private var isContentMounted = false
    /// The progress on screen. It trails `progress` while a settle runs, see `PresentedProgress`.
    @State private var presentedProgress = PresentedProgress()
    /// Whether the press feedback is applied. It holds for `pressMinimumHold` after the finger lifts.
    @State private var isPressed = false
    @State private var pressStart: ContinuousClock.Instant?
    /// The pending release. A new press cancels it so it cannot lift that press.
    @State private var pressRelease: Task<Void, Never>?
    /// Bumped to play the medium impact when the panel opens.
    @State private var mediumFeedback = 0
    /// Bumped to play the light impact when the panel closes.
    @State private var lightFeedback = 0
    @GestureState private var isDragging = false
    /// Whether a swipe open has begun and has not yet reached its landing.
    @State private var isSwipeOpen = false
    /// Whether a swipe into Search is past the halfway point, so crossing it plays a haptic.
    @State private var isSwipePastHalfway = false
    /// Skips the light impact of the close ending a swipe into Search, whose haptics played at the halfway crossings.
    @State private var mutesNextCloseFeedback = false
    /// The pending landing. A new drag cancels it.
    @State private var swipeLanding: Task<Void, Never>?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// `contentHeight` is the height `content` takes when laid out at the panel's open width. `accessibilityLabel`
    /// names the control for VoiceOver. `openAccessibilityLabel` names the control for VoiceOver while the panel is
    /// open. `onSettled` receives whether the panel ended open after each settle the control runs. It is not called
    /// while a drag is in flight. `onCloseRequested` is called instead of animating closed when the control would
    /// settle closed from open. It receives the close's duration, nil for the default, and the host closes the panel
    /// through `progress`. `onSwipeOpenBegan` is called once per drag when a drag on the button that began short
    /// of fully open moves up past the tap slop. `onSwipeOpenLanding` is called when that drag's release settles
    /// open and `swipeLandingFraction` of the settle has run, at once under Reduce Motion.
    public init(
        progress: Binding<CGFloat>,
        contentHeight: CGFloat,
        accessibilityLabel: String,
        openAccessibilityLabel: String,
        onSettled: @escaping (Bool) -> Void,
        onCloseRequested: ((TimeInterval?) -> Void)? = nil,
        onSwipeOpenBegan: (() -> Void)? = nil,
        onSwipeOpenLanding: (() -> Void)? = nil,
        @ViewBuilder content: () -> Content
    ) {
        _progress = progress
        self.contentHeight = contentHeight
        self.accessibilityLabel = accessibilityLabel
        self.openAccessibilityLabel = openAccessibilityLabel
        self.onSettled = onSettled
        self.onCloseRequested = onCloseRequested
        self.onSwipeOpenBegan = onSwipeOpenBegan
        self.onSwipeOpenLanding = onSwipeOpenLanding
        self.content = content()
    }

    public var body: some View {
        GeometryReader { proxy in
            let size = frameSize(availableWidth: proxy.size.width)
            panel(in: size, availableWidth: proxy.size.width)
                .scaleEffect(isPressed && progress == 0 ? pressScale : 1)
                .offset(
                    x: (proxy.size.width - size.width) / 2,
                    y: proxy.size.height - size.height - seatInset
                )
        }
        .onChange(of: isDragging) { _, dragging in
            if dragging {
                press()
            } else {
                scheduleRelease()
            }
            // A cancelled drag never reaches `onEnded`, so it settles here to the nearer end.
            guard !dragging, dragStart != nil else {
                return
            }
            cancelDrag()
        }
        .onChange(of: progress) { old, new in
            if new > 0 {
                mountContent()
            } else if dragStart == nil {
                unmountContentIfClosed()
            }
            if new == 0, old > 0, dragStart == nil {
                playCloseFeedback()
            }
            guard dragStart == nil, old != new else {
                return
            }
            revealContent(open: new > old)
        }
        .sensoryFeedback(.impact(weight: .medium), trigger: mediumFeedback)
        .sensoryFeedback(.impact(weight: .light), trigger: lightFeedback)
    }
}

// MARK: - Geometry

private extension DSTabBarCenterControl {
    /// Bottom edge of the resting notch, which the morph keeps fixed.
    var bottomFromRowTop: CGFloat { DSTabBarStyle.notchCenterY + DSTabBarStyle.notchRadius }

    /// How far the control's foot sits above the bar row's bottom edge, so the lobe rests in the
    /// notch with an even ring around it.
    var seatInset: CGFloat { DSTabBarStyle.height - bottomFromRowTop + DSTabBarStyle.notchRingGap }

    /// How far the control has moved from the closed button's colours to the open panel's, 0 to 1.
    var openBlend: CGFloat {
        let blendRange = DSTabBarStyle.centerBlendEnd - DSTabBarStyle.centerBlendStart
        return min(1, max(0, (progress - DSTabBarStyle.centerBlendStart) / blendRange))
    }

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

    /// Scale of the closed control while a finger is down on it.
    var pressScale: CGFloat { 0.96 }

    /// Duration of the ease-out that presses the closed control in.
    var pressInDuration: Double { 0.077 }

    /// Duration of the ease-out that releases the press.
    var pressOutDuration: Double { 0.221 }

    /// Shortest time a press stays down from touch-down, so a quick tap still shows it.
    var pressMinimumHold: Double { 0.15 }

    /// Distance the finger travels to open the panel fully.
    var travel: CGFloat { max(minimumTravel, expandedHeight - DSTabBarStyle.centerButtonSize) }

    var expandedHeight: CGFloat {
        DSTabBarStyle.centerPanelHeight(contentHeight: contentHeight)
    }

    /// Movement below which a drag that ends counts as a tap.
    var tapSlop: CGFloat { 14 }

    /// Movement the finger makes before the panel starts to follow it, so a tap never nudges the panel.
    var dragStartDistance: CGFloat { 10 }

    /// Shortest distance the finger travels to open the panel fully.
    var minimumTravel: CGFloat { 120 }

    /// Share of a swipe's settle open after which the host is told it is landing.
    var swipeLandingFraction: Double { 0.6 }

    /// Share of the finger's travel past fully open that the panel stretches by.
    var stretchFraction: CGFloat { 1.0 / 3 }

    /// Speed, in pt/s, above which a release is a flick that decides the settle by where it would carry the panel.
    var flickSpeed: CGFloat { 500 }

    /// How far ahead, in seconds, a flick is projected when it decides the settle.
    var flickLookahead: CGFloat { 0.1 }

    /// Seconds of coasting a release at 1 pt/s adds to its predicted end. It is the distance `UIScrollView`'s normal
    /// deceleration projects, per pt/s, so the predicted end reads back as a release speed.
    var releaseProjectionSeconds: CGFloat { 0.499 }

    /// Speed, in pt/s, above which a release settles over a duration that follows its speed.
    var releaseSpeed: CGFloat { 300 }

    /// Scale from a release's remaining distance over its speed to the duration it settles over.
    var releaseDurationScale: Double { 2.5 }

    /// Shortest and longest duration a release settles over, in seconds.
    var releaseDurationRange: ClosedRange<Double> { 0.18 ... 0.42 }

    /// Duration of a slow release, in seconds.
    var slowReleaseDuration: Double { 0.34 }

    /// Progress at which a release lands open, or closed when below it.
    var halfwayProgress: CGFloat { 0.5 }

    /// Share of the width's blend that follows a smoothstep. The rest follows an ease-out.
    var widthSmoothWeight: CGFloat { 0.35 }

    /// Fraction of the travel the width waits before it starts to open.
    var widthStartDelay: CGFloat { 0.04 }

    /// Progress at which a drag starts to show the content. The content is fully shown at fully open.
    var revealDragStart: CGFloat { 0.88 }

    /// Distance the content sits below its place while hidden, which it rises through as it fades in.
    var revealOffset: CGFloat { 8 }

    /// Blur the content has while hidden, which it sheds as it fades in.
    var revealBlurRadius: CGFloat { 8 }

    func lerp(_ from: CGFloat, _ to: CGFloat) -> CGFloat {
        from + (to - from) * progress
    }

    func clampUnit(_ value: CGFloat) -> CGFloat {
        min(1, max(0, value))
    }

    /// The width's share of the way from closed to open, 0 closed and 1 open. It recovers the linear time behind the
    /// eased progress, trails it by `widthStartDelay`, and blends a smoothstep with an ease-out.
    var widthProgress: CGFloat {
        let linearTime = clampUnit(1 - cbrt(1 - progress))
        let delayed = clampUnit((linearTime - widthStartDelay) / (1 - widthStartDelay))
        let smoothstep = delayed * delayed * (3 - 2 * delayed)
        let easeOut = 1 - pow(1 - delayed, 3)
        return widthSmoothWeight * smoothstep + (1 - widthSmoothWeight) * easeOut
    }

    func frameSize(availableWidth: CGFloat) -> CGSize {
        let closedWidth = DSTabBarStyle.centerButtonSize
        let expandedWidth = DSTabBarStyle.centerPanelWidth(inRowOfWidth: availableWidth)
        return CGSize(
            width: closedWidth + (expandedWidth - closedWidth) * widthProgress,
            height: lerp(DSTabBarStyle.centerButtonSize, expandedHeight) + stretch
        )
    }
}

// MARK: - Body

private extension DSTabBarCenterControl {
    func panel(in size: CGSize, availableWidth: CGFloat) -> some View {
        let openWidth = DSTabBarStyle.centerPanelWidth(inRowOfWidth: availableWidth)
        return ZStack {
            if isContentMounted {
                // Fixed at the open size so it is laid out once; clipShape crops it as the panel morphs.
                content
                    .frame(width: openWidth, height: contentHeight)
                    .environment(\.centerPanelDrag, centerPanelDrag)
                    .opacity(reveal)
                    .blur(radius: contentBlur)
                    .offset(y: contentLift)
            }
        }
        .padding(.bottom, DSTabBarStyle.centerLobeDepth)
        .foregroundStyle(Color.fgPrimary)
        .frame(width: size.width, height: size.height, alignment: progress > 0 ? .top : .center)
        // After the frame, so the grabber lands against the panel rather than against
        // the content, which can be shorter than the panel.
        .overlay(alignment: .top) { grabber }
        .background(DSTabBarCenterMaterial(openBlend: openBlend))
        .overlay(rim)
        .clipShape(DSTabBarMorphShape())
        .contentShape(DSTabBarMorphShape())
        // Outside the content shape, so the glyph's touch area can hang below the lobe.
        .overlay(alignment: .bottom) { glyph }
        .modifier(PresentedProgressTracker(progress: progress, presented: presentedProgress))
    }
}

// MARK: - Grips

private extension DSTabBarCenterControl {
    /// Sits in the lobe in both states, since the lobe is the button. The scan frame shrinks
    /// slightly and drops a little as the panel opens. The touch area shrinks to `openTouchSize`
    /// with it, and moves with it because the offset comes after the content shape.
    var glyph: some View {
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
            .gesture(drag)
            .accessibilityElement()
            .accessibilityLabel(progress > 0.5 ? openAccessibilityLabel : accessibilityLabel)
            .accessibilityAddTraits(.isButton)
            .accessibilityAction {
                if progress <= 0.5 {
                    mediumFeedback += 1
                }
                settle(to: progress > 0.5 ? 0 : 1)
            }
            .accessibilityAction(.escape) {
                guard progress > 0.5 else {
                    return
                }
                settle(to: 0)
            }
    }

    /// Marks the top of the open panel as draggable. Fades in with the content and takes no touches,
    /// so a drag there reaches the content, which moves the panel through `centerPanelDrag`.
    var grabber: some View {
        Capsule()
            .fill(DSTabBarStyle.Palette.grabber)
            .frame(width: DSTabBarStyle.centerGrabberWidth, height: DSTabBarStyle.centerGrabberThickness)
            .padding(.top, DSTabBarStyle.centerGrabberTopInset)
            .frame(maxWidth: .infinity, alignment: .top)
            .opacity(reveal)
            .allowsHitTesting(false)
    }

    /// Drag up to open, down to close; a drag that barely moves is a tap.
    /// `minimumDistance: 0` so one gesture serves both and they cannot conflict.
    var drag: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .global)
            .updating($isDragging) { _, dragging, _ in
                dragging = true
            }
            .onChanged { value in
                followDrag(translationHeight: value.translation.height)
                beginSwipeOpenIfNeeded(translationHeight: value.translation.height)
                playSwipeHalfwayCrossing()
            }
            .onEnded { value in
                let start = dragStart ?? progress
                guard abs(value.translation.height) >= tapSlop else {
                    dragStart = nil
                    if start <= 0.5 {
                        mediumFeedback += 1
                    }
                    settle(to: start > 0.5 ? 0 : 1)
                    return
                }
                endDrag(velocityHeight: releaseVelocity(value))
            }
    }

    /// The panel's content drags it through these callbacks, with the same settle as the lobe.
    var centerPanelDrag: DSTabBarCenterDrag {
        DSTabBarCenterDrag(
            onChanged: { followDrag(translationHeight: $0) },
            onEnded: { endDrag(velocityHeight: $0) },
            onCancelled: { cancelDrag() }
        )
    }

    /// SwiftUI's drag value carries no velocity, so the release speed is read back from its predicted end.
    func releaseVelocity(_ value: DragGesture.Value) -> CGFloat {
        let predictedTravel = value.predictedEndTranslation.height - value.translation.height
        return predictedTravel / releaseProjectionSeconds
    }
}

// MARK: - Settle

private extension DSTabBarCenterControl {
    /// Moves the panel with the finger from the progress at which the drag began. Translations are in the
    /// global space: the panel moves under the finger, so in its own space the translation would shrink
    /// as the panel follows it and the panel would fight the drag.
    func followDrag(translationHeight: CGFloat) {
        if dragStart == nil {
            adoptPresentedProgress()
            swipeLanding?.cancel()
            swipeLanding = nil
            isSwipeOpen = false
            mutesNextCloseFeedback = false
        }
        let start = dragStart ?? progress
        dragStart = start
        guard abs(translationHeight) > dragStartDistance else {
            return
        }
        let unclamped = start - followedTranslation(translationHeight) / travel
        let clamped = clampUnit(unclamped)
        progress = clamped
        stretch = max(0, unclamped - 1) * travel * stretchFraction
        reveal = dragReveal(at: clamped)
    }

    /// Begins a swipe open once a button drag that began short of fully open moves up past the tap slop.
    func beginSwipeOpenIfNeeded(translationHeight: CGFloat) {
        guard onSwipeOpenBegan != nil,
              !isSwipeOpen,
              let start = dragStart,
              start < 1,
              translationHeight <= -tapSlop
        else {
            return
        }
        isSwipeOpen = true
        isSwipePastHalfway = progress >= halfwayProgress
        onSwipeOpenBegan?()
    }

    /// Plays a medium impact each time a swipe into Search carries the panel across the halfway point.
    func playSwipeHalfwayCrossing() {
        guard isSwipeOpen else {
            return
        }
        let isPast = progress >= halfwayProgress
        guard isPast != isSwipePastHalfway else {
            return
        }
        isSwipePastHalfway = isPast
        mediumFeedback += 1
    }

    /// Stops a running settle at the progress on screen, so a new drag or tap starts from the shape the user sees.
    /// Nothing animates the jump, and the settle's completion sees `progress` no longer at its target.
    func adoptPresentedProgress() {
        guard presentedProgress.value != progress else {
            return
        }
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            progress = presentedProgress.value
            stretch = 0
        }
    }

    /// The translation the panel follows: none until the finger passes `dragStartDistance`, then the distance past it.
    func followedTranslation(_ translationHeight: CGFloat) -> CGFloat {
        translationHeight > 0 ? translationHeight - dragStartDistance : translationHeight + dragStartDistance
    }

    /// Content opacity while a drag moves the panel: hidden below `revealDragStart`, shown fully at fully open.
    func dragReveal(at position: CGFloat) -> CGFloat {
        clampUnit((position - revealDragStart) / (1 - revealDragStart))
    }

    /// Settles the panel where the release leaves it. `velocityHeight` is positive downward, like the translations.
    func endDrag(velocityHeight: CGFloat) {
        let isSwipe = isSwipeOpen
        let upwardVelocity = -velocityHeight
        let start = dragStart ?? progress
        dragStart = nil
        let target = releaseTarget(upwardVelocity: upwardVelocity)
        if isSwipe {
            if target == 0, progress > 0 {
                mutesNextCloseFeedback = true
            }
        } else {
            if target == 1, start < 1 {
                mediumFeedback += 1
            }
            if target == 0, progress == 0, start > 0 {
                lightFeedback += 1
            }
        }
        settle(to: target, duration: releaseDuration(to: target, upwardVelocity: upwardVelocity))
    }

    /// A cancelled drag never reaches `endDrag`, so it settles to the nearer end.
    func cancelDrag() {
        dragStart = nil
        settle(to: progress > 0.5 ? 1 : 0)
    }

    /// Plays the light impact of a close, unless the close ends a swipe into Search.
    func playCloseFeedback() {
        guard !mutesNextCloseFeedback else {
            mutesNextCloseFeedback = false
            return
        }
        lightFeedback += 1
    }

    /// A fast flick decides by where it would carry the panel in `flickLookahead` seconds. A slow release decides
    /// by the halfway point.
    func releaseTarget(upwardVelocity: CGFloat) -> CGFloat {
        let decidingProgress = abs(upwardVelocity) > flickSpeed
            ? progress + upwardVelocity * flickLookahead / travel
            : progress
        return decidingProgress >= halfwayProgress ? 1 : 0
    }

    /// A fast release settles over the time its remaining distance takes at its speed, clamped to a range. A slow
    /// release takes `slowReleaseDuration`.
    func releaseDuration(to target: CGFloat, upwardVelocity: CGFloat) -> Double {
        let speed = abs(upwardVelocity)
        guard speed > releaseSpeed else {
            return slowReleaseDuration
        }
        let remainingDistance = abs(target - progress) * travel
        let duration = releaseDurationScale * Double(remainingDistance / speed)
        return min(max(duration, releaseDurationRange.lowerBound), releaseDurationRange.upperBound)
    }

    /// Under Reduce Motion the geometry snaps and only the content cross-fades, so the morph
    /// itself never plays. Dragging is never animated either way, so the control still tracks
    /// the finger. `onSettled` runs once the move has logically finished, or at once under Reduce
    /// Motion, and only if no later move has taken the progress elsewhere. A close from open goes to
    /// `onCloseRequested` when the host provides it, so the host's own animation moves the progress.
    /// `duration` is nil for the default settle.
    func settle(to target: CGFloat, duration: Double? = nil) {
        if target == 1 {
            mountContent()
            scheduleSwipeLanding(duration: duration)
        } else {
            isSwipeOpen = false
            swipeLanding?.cancel()
            swipeLanding = nil
        }
        if target == 0, progress > 0 || stretch > 0, let onCloseRequested {
            requestClose(duration: duration, onCloseRequested: onCloseRequested)
            return
        }
        revealContent(open: target == 1, closeDuration: duration)
        guard !reduceMotion else {
            progress = target
            stretch = 0
            onSettled(target == 1)
            unmountContentIfClosed()
            return
        }
        let curve = settleCurve(duration: duration)
        withAnimation(curve, completionCriteria: .logicallyComplete) {
            progress = target
            stretch = 0
        } completion: {
            guard progress == target else {
                return
            }
            onSettled(target == 1)
            unmountContentIfClosed()
        }
    }

    /// Tells the host a swipe is landing once `swipeLandingFraction` of its open has run, at once under Reduce Motion.
    func scheduleSwipeLanding(duration: Double?) {
        guard isSwipeOpen else {
            return
        }
        isSwipeOpen = false
        let delay = reduceMotion ? 0 : swipeLandingFraction * (duration ?? DSTabBarStyle.centerDuration)
        swipeLanding = Task {
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled, progress == 1, dragStart == nil else {
                return
            }
            onSwipeOpenLanding?()
        }
    }

    /// Hands the close to the host, whose move fades the content over its last part. Under Reduce Motion the content
    /// cross-fades first and the host's move starts once it is gone.
    func requestClose(duration: Double?, onCloseRequested: @escaping (TimeInterval?) -> Void) {
        revealContent(open: false, closeDuration: duration)
        guard reduceMotion else {
            withAnimation(settleCurve(duration: duration)) {
                stretch = 0
            }
            onCloseRequested(duration)
            return
        }
        Task {
            try? await Task.sleep(for: .seconds(DSTabBarStyle.centerReduceMotionFade))
            stretch = 0
            onCloseRequested(duration)
        }
    }

    /// The ease a settle moves on: the centre control's ease-out over `duration`, or the default.
    func settleCurve(duration: Double?) -> Animation {
        duration.map { DSTabBarStyle.centerCurve(duration: $0) } ?? DSTabBarStyle.centerEaseOut
    }
}

// MARK: - Press

private extension DSTabBarCenterControl {
    func press() {
        pressRelease?.cancel()
        pressStart = ContinuousClock.now
        withAnimation(.easeOut(duration: pressInDuration)) {
            isPressed = true
        }
    }

    /// Releases once the press has been down for `pressMinimumHold`.
    func scheduleRelease() {
        let releaseDeadline = (pressStart ?? .now) + .seconds(pressMinimumHold)
        pressRelease = Task {
            try? await Task.sleep(until: releaseDeadline, clock: .continuous)
            guard !Task.isCancelled else {
                return
            }
            withAnimation(.easeOut(duration: pressOutDuration)) {
                isPressed = false
            }
        }
    }
}

// MARK: - Reveal

private extension DSTabBarCenterControl {
    /// Adds the content without animation, so its first layout lands in place even inside an animated open.
    func mountContent() {
        guard !isContentMounted else {
            return
        }
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            isContentMounted = true
        }
    }

    /// Removes the content once the panel is closed and the content has faded out. Never mid-drag: a drag that reaches
    /// closed leaves it to the settle, which removes it after reporting, so the host prepares the content first.
    func unmountContentIfClosed() {
        guard isContentMounted, progress == 0, reveal == 0 else {
            return
        }
        isContentMounted = false
    }

    /// Moves the content's reveal to the end of a settle toward `open`. Opening shows the content at once, so it rides
    /// the panel's edge as the shape grows. Closing keeps it shown and fades it over the last part of the close, which
    /// lasts `closeDuration`, or the default settle when nil.
    func revealContent(open: Bool, closeDuration: Double? = nil) {
        let target: CGFloat = open ? 1 : 0
        guard reveal != target else {
            return
        }
        if reduceMotion {
            withAnimation(.easeInOut(duration: DSTabBarStyle.centerReduceMotionFade)) {
                reveal = target
            }
        } else if open {
            reveal = 1
        } else {
            let duration = closeDuration ?? DSTabBarStyle.centerDuration
            withAnimation(
                .easeInOut(duration: duration * DSTabBarStyle.contentCloseFadeLength)
                    .delay(duration * DSTabBarStyle.contentCloseFadeStart),
                completionCriteria: .logicallyComplete
            ) {
                reveal = 0
            } completion: {
                unmountContentIfClosed()
            }
        }
    }

    /// Blur the content sheds as it fades in. Reduce Motion skips it, so the content only cross-fades.
    var contentBlur: CGFloat {
        reduceMotion ? 0 : revealBlurRadius * (1 - reveal)
    }

    /// Distance the content sits below its place while it fades in. Reduce Motion skips it.
    var contentLift: CGFloat {
        reduceMotion ? 0 : revealOffset * (1 - reveal)
    }
}

// MARK: - Presented progress

/// The progress the control is drawn at. `progress` takes a settle's target as soon as the settle starts, so a
/// drag that begins mid-settle would read the target. `PresentedProgressTracker` writes the interpolated value here
/// on every frame. It is a reference, so writing it during rendering does not invalidate the view.
private final class PresentedProgress {
    var value: CGFloat = 0
}

/// Records the progress the control is drawn at and leaves the geometry unchanged. SwiftUI interpolates `progress`
/// through `animatableData` and evaluates `effectValue` on every frame of a settle, including the last.
private struct PresentedProgressTracker: GeometryEffect {
    var progress: CGFloat
    let presented: PresentedProgress

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    func effectValue(size _: CGSize) -> ProjectionTransform {
        presented.value = progress
        return ProjectionTransform()
    }
}

// MARK: - Volume

private extension DSTabBarCenterControl {
    /// Hairline along the edge. A border stroke, so the clip leaves its full width.
    var rim: some View {
        DSTabBarMorphShape()
            .strokeBorder(DSTabBarStyle.Palette.strokeGlass, lineWidth: DSTabBarStyle.strokeWidth)
    }
}
