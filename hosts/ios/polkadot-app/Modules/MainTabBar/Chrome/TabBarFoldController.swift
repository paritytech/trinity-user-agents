import UIKit
import DesignSystem
import PolkadotUI

/// Owns the tab bar's visibility state machine and the transform that realises it. The chrome
/// keeps view installation and layout; this decides which state applies and drives the offset.
@MainActor
final class TabBarFoldController {
    private unowned let foldSurface: UIView

    /// Bounds of the chrome's own full-bleed view; every distance derives from it.
    private let chromeBounds: () -> CGRect
    /// The bottom safe area inset the resting bar's gap depends on.
    private let bottomSafeAreaInset: () -> CGFloat
    private let outputs: Outputs

    private(set) var state: TabBarVisibilityState = .shown
    private(set) var isTabRoot = true
    /// Set together with the offset, so it can lead `state` during an interactive transition.
    private(set) var isTucked = false

    var isSettling: Bool {
        motion.isSettling
    }

    private var foldDerived: TabBarFoldDerived = .none
    private weak var navigationScreen: UIViewController?
    /// The user's tuck or restore. Only a tucked value carries across navigation.
    private var userOverride: TabBarFoldOverride = .none
    private var motion = DockletMotion()
    /// Decides when the auto-hide tucks the bar. The drag, settle and state blockers are read from this controller.
    private var autoHide = DockletAutoHide(startedAt: CACurrentMediaTime())
    /// The one wake-up that runs the auto-hide tuck at its deadline.
    private var autoHideTask: Task<Void, Never>?
    /// The kind of tuck whose settle is running. It is announced when the settle lands tucked.
    private var pendingTuckKind: DockletTuckKind?
    /// The nudge that plays once per session after the first tuck lands.
    private var nudge = DockletNudge()
    /// The wake-up that starts the nudge after its delay.
    private var nudgeTask: Task<Void, Never>?
    /// A finger is on the bar, from the start of a drag to its release.
    private var isDragging = false
    private var displayLink: CADisplayLink?
    private var foldAnimator: UIViewPropertyAnimator?
    private var foldOffsetWidth: CGFloat?
    private var isFoldInterpolating = false
    private let heavyImpact = UIImpactFeedbackGenerator(style: .heavy)
    private let mediumImpact = UIImpactFeedbackGenerator(style: .medium)
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init(
        foldSurface: UIView,
        chromeBounds: @escaping () -> CGRect,
        bottomSafeAreaInset: @escaping () -> CGFloat,
        outputs: Outputs
    ) {
        self.foldSurface = foldSurface
        self.chromeBounds = chromeBounds
        self.bottomSafeAreaInset = bottomSafeAreaInset
        self.outputs = outputs
    }

    deinit {
        MainActor.assumeIsolated {
            foldAnimator?.cancelInPlace()
            displayLink?.invalidate()
            autoHideTask?.cancel()
            nudgeTask?.cancel()
        }
    }

    func update(context: TabBarChromeContext) {
        if context.screen !== navigationScreen {
            userOverride = TabBarVisibilityPolicy.carriedOverride(from: state)
        }
        isTabRoot = context.isTabRoot
        foldDerived = context.foldDerived
        navigationScreen = context.screen
    }

    /// Re-applies the current state after a width change, unanimated. Returns false when the
    /// width is unchanged and nothing needed doing.
    @discardableResult
    func reapplyForWidthChange() -> Bool {
        guard chromeBounds().width != foldOffsetWidth else {
            return false
        }
        foldOffsetWidth = chromeBounds().width
        applyVisibility(state, from: state, animated: false, isUserDriven: false)
        rescheduleAutoHide()
        return true
    }

    /// Resolves the visibility state a context would settle on, without committing it — used to
    /// drive the offset alongside an interactive navigation transition.
    func resolvedState(for context: TabBarChromeContext) -> TabBarVisibilityState {
        let override = context.screen === navigationScreen
            ? userOverride
            : TabBarVisibilityPolicy.carriedOverride(from: state)
        return TabBarVisibilityPolicy.state(derived: context.foldDerived, override: override)
    }

    /// Moves the chrome toward `state` in sync with an interactive transition. Unlike `apply(state:)`
    /// it runs no animator or display link of its own, so the caller's transition coordinator owns the timing.
    func setInteractiveTarget(_ state: TabBarVisibilityState) {
        isFoldInterpolating = true
        isTucked = state == .tucked
        stopSettling()
        cancelFoldAnimator()
        cancelNudge()
        motion.snap(to: state == .tucked ? .tucked : .shown)
        writeTransform(pose(for: state))
        rescheduleAutoHide()
    }

    /// Records the user's choice for the current screen and moves the bar to it. Tapping a tucked bar restores it.
    func setUserOverride(_ override: TabBarFoldOverride) {
        guard state != .hidden else {
            return
        }
        userOverride = override
        apply(state: policyState, isUserDriven: true)
    }

    func foldPanBegan(location: CGPoint) {
        prepareDrag()
        motion.begin(touch: location, time: CACurrentMediaTime(), geometry: geometry)
    }

    /// A touch in the Grab zone catches a tucked bar, or one still settling toward tucked, and turns it to the finger.
    func grabPanBegan(location: CGPoint) {
        prepareDrag()
        motion.beginAim(touch: location, time: CACurrentMediaTime(), geometry: geometry)
    }

    func foldPanChanged(location: CGPoint, translationX: CGFloat) {
        if let haptic = motion.move(location: location, translationX: translationX, time: CACurrentMediaTime()) {
            playHaptic(haptic)
        }
        writeTransform(dockletTransform())
    }

    /// The release starts a settle that carries the finger's velocity. A change of state is recorded as the
    /// screen's override without restarting that settle.
    func foldPanEnded() {
        isDragging = false
        motion.end(time: CACurrentMediaTime(), geometry: geometry, reduceMotion: isReduceMotionEnabled)
        if motion.isSettling {
            startSettling()
        }
        let tucks = motion.destination == .tucked
        guard tucks != (state == .tucked) else {
            return
        }
        userOverride = tucks ? .tucked : .shown
        pendingTuckKind = tucks ? .user : nil
        commit(tucks ? .tucked : .shown)
    }

    func refresh() {
        apply(state: policyState)
    }

    /// Ends a running settle at its rest when Reduce Motion turns on, so the rest of it does not swing.
    func reduceMotionChanged() {
        guard isReduceMotionEnabled, motion.isSettling else {
            return
        }
        motion.snap(to: motion.destination)
        writeTransform(dockletTransform())
    }

    /// Feeds a touch to the auto-hide. A touch on the Docklet cancels a pending outside-touch tuck.
    func recordTouch(isOnDocklet: Bool) {
        autoHide.touch(isOnDocklet: isOnDocklet, at: CACurrentMediaTime())
        rescheduleAutoHide()
    }

    /// Feeds a hardware key press to the auto-hide, which restarts the idle countdown.
    func recordKeyPress() {
        autoHide.keyPress(at: CACurrentMediaTime())
        rescheduleAutoHide()
    }

    /// Holds or releases an auto-hide blocker that the chrome owns, such as an open panel or the keyboard.
    func setAutoHideBlocked(_ blocker: DockletHideBlocker, _ isBlocked: Bool) {
        autoHide.setBlocked(blocker, isBlocked, at: CACurrentMediaTime())
        rescheduleAutoHide()
    }
}

private extension TabBarFoldController {
    var isReduceMotionEnabled: Bool {
        UIAccessibility.isReduceMotionEnabled
    }

    var policyState: TabBarVisibilityState {
        TabBarVisibilityPolicy.state(derived: foldDerived, override: userOverride)
    }

    var geometry: DockletGeometry {
        let bounds = chromeBounds()
        let rowWidth = DSTabBarStyle.rowWidth(inAvailableWidth: bounds.width)
        let bottomGap = DSTabBarStyle.bottomGap(bottomSafeAreaInset: bottomSafeAreaInset())
        let dockFrame = CGRect(
            x: Self.barMaxX(availableWidth: bounds.width) - rowWidth,
            y: bounds.height - bottomGap - DSTabBarStyle.height,
            width: rowWidth,
            height: DSTabBarStyle.height
        )
        return DockletGeometry(dockFrame: dockFrame, bounds: bounds)
    }

    func apply(state newState: TabBarVisibilityState, isUserDriven: Bool = false) {
        guard newState != state || isFoldInterpolating else {
            return
        }
        isFoldInterpolating = false

        let previous = state
        commit(newState)
        applyVisibility(newState, from: previous, animated: true, isUserDriven: isUserDriven)
        rescheduleAutoHide()
    }

    func commit(_ newState: TabBarVisibilityState) {
        state = newState
        isTucked = newState == .tucked
        if newState != .tucked {
            cancelNudge()
        }
        outputs.state(newState)
        if newState != .shown {
            outputs.closePanel()
        }
    }

    func applyVisibility(
        _ state: TabBarVisibilityState,
        from previous: TabBarVisibilityState,
        animated: Bool,
        isUserDriven: Bool
    ) {
        foldOffsetWidth = chromeBounds().width
        stopSettling()
        cancelFoldAnimator()

        let rest: DockletRest = state == .tucked ? .tucked : .shown
        guard animated, !TabBarVisibilityPolicy.isOffScreenSwap(from: previous, to: state) else {
            motion.snap(to: rest)
            writeTransform(pose(for: state))
            return
        }
        guard state != .hidden, previous != .hidden else {
            motion.snap(to: rest)
            animateSlide(to: pose(for: state))
            return
        }
        motion.settle(
            to: rest,
            now: CACurrentMediaTime(),
            isUserDriven: isUserDriven,
            reduceMotion: isReduceMotionEnabled
        )
        startSettling()
    }

    /// Slides between the hidden pose and a docklet pose with a plain spring, since the display link only drives
    /// the docklet pose.
    func animateSlide(to transform: CGAffineTransform) {
        guard !isReduceMotionEnabled else {
            writeTransform(transform)
            return
        }
        let animator = UIViewPropertyAnimator(duration: 0.45, dampingRatio: 0.85) { [weak self] in
            self?.writeTransform(transform)
        }
        animator.addCompletion { [weak self] _ in
            self?.foldAnimator = nil
        }
        foldAnimator = animator
        animator.startAnimation()
    }

    /// Readies the feedback generators and stops whatever was moving the bar, before a drag takes it over.
    func prepareDrag() {
        heavyImpact.prepare()
        mediumImpact.prepare()
        selectionFeedback.prepare()
        cancelFoldAnimator()
        stopSettling()
        cancelNudge()
        pendingTuckKind = nil
        isDragging = true
        rescheduleAutoHide()
    }

    func cancelFoldAnimator() {
        let previous = foldAnimator
        foldAnimator = nil
        previous?.cancelInPlace()
    }

    func startSettling() {
        guard displayLink == nil else {
            return
        }
        let frameTarget = DisplayLinkFrameTarget { [weak self] link in
            self?.tickSettle(at: link.targetTimestamp)
        }
        let link = CADisplayLink(target: frameTarget, selector: #selector(DisplayLinkFrameTarget.frame(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    func stopSettling() {
        displayLink?.invalidate()
        displayLink = nil
    }

    func tickSettle(at time: CFTimeInterval) {
        let wasSettling = motion.isSettling
        let haptic = motion.tick(now: time)
        writeTransform(dockletTransform())
        if let haptic {
            playHaptic(haptic)
        }
        if nudge.isOver(at: CACurrentMediaTime()) {
            nudge.stop()
        }
        if wasSettling, !motion.isSettling {
            settleLanded()
        }
        if !motion.isSettling, !nudge.isPlaying {
            stopSettling()
        }
    }

    func playHaptic(_ haptic: DockletHaptic) {
        switch haptic {
        case .heavy:
            heavyImpact.impactOccurred()
        case .medium:
            mediumImpact.impactOccurred()
        case .selection:
            selectionFeedback.selectionChanged()
        }
    }

    /// The only writer of the chrome's transform. The Edge glow and Grab zone follow the same progress.
    func writeTransform(_ transform: CGAffineTransform) {
        let geometry = geometry
        foldSurface.transform = transform
        outputs.glow(geometry.edgeGlowRect, glowOpacity)
        outputs.grabZone(motion.destination == .tucked ? geometry.grabZoneRect : .zero)
        outputs.contentOpacity(motion.contentOpacity)
    }

    /// The Edge glow follows the hide progress while a finger drags the bar or it settles toward tucked. A settle
    /// toward shown fades it out instead. A Reduce Motion fade crosses the glow with the content.
    var glowOpacity: TabBarEdgeGlowOpacity {
        if motion.isFading {
            return .progress(1 - motion.contentOpacity)
        }
        return isDragging || motion.destination == .tucked ? .progress(motion.progress) : .fadeOut
    }

    /// The docklet pose about the centre of the full-bleed chrome, which is where the chrome's transform is anchored.
    func dockletTransform() -> CGAffineTransform {
        let bounds = chromeBounds()
        let pose = motion.pose(geometry: geometry)
        let nudgedPose = DockletPose(
            angle: pose.angle,
            offset: CGVector(dx: pose.offset.dx - nudge.inset(at: CACurrentMediaTime()), dy: pose.offset.dy)
        )
        return geometry.transform(nudgedPose, anchor: CGPoint(x: bounds.midX, y: bounds.midY))
    }

    func hiddenTransform() -> CGAffineTransform {
        CGAffineTransform(translationX: Self.hiddenTranslationX(availableWidth: chromeBounds().width), y: 0)
    }

    func pose(for state: TabBarVisibilityState) -> CGAffineTransform {
        state == .hidden ? hiddenTransform() : dockletTransform()
    }

    /// Tucks the bar on the quiet settle, with no haptic. The tuck is recorded as the screen's override, as a swipe is.
    func tuckQuietly() {
        userOverride = .tucked
        pendingTuckKind = .autoHide
        commit(.tucked)
        cancelFoldAnimator()
        motion.quietSettle(to: .tucked, now: CACurrentMediaTime(), reduceMotion: isReduceMotionEnabled)
        startSettling()
        rescheduleAutoHide()
    }

    /// A settle has landed. A tuck that lands tucked is announced, and the session's first one arms the nudge.
    func settleLanded() {
        if let kind = pendingTuckKind, state == .tucked {
            outputs.tuckLanded(kind)
            armNudge()
        }
        pendingTuckKind = nil
        rescheduleAutoHide()
    }

    func armNudge() {
        guard !isReduceMotionEnabled, nudge.arm() else {
            return
        }
        nudgeTask = Task.delayed(by: DockletNudge.delaySeconds) { [weak self] in
            self?.startNudge()
        }
    }

    func startNudge() {
        nudgeTask = nil
        nudge.start(at: CACurrentMediaTime())
        startSettling()
    }

    /// Ends the nudge, including one still waiting for its delay. An armed nudge never plays again.
    func cancelNudge() {
        nudgeTask?.cancel()
        nudgeTask = nil
        nudge.stop()
    }

    /// Cancels the pending wake-up and, unless a blocker holds, sleeps one wake-up until the auto-hide deadline.
    func rescheduleAutoHide() {
        autoHideTask?.cancel()
        autoHideTask = nil
        syncAutoHideBlockers()
        guard let deadline = autoHide.deadline else {
            return
        }
        autoHideTask = Task.delayed(by: max(0, deadline - CACurrentMediaTime())) { [weak self] in
            self?.autoHideFired()
        }
    }

    func autoHideFired() {
        autoHideTask = nil
        syncAutoHideBlockers()
        guard autoHide.isDue(at: CACurrentMediaTime()) else {
            rescheduleAutoHide()
            return
        }
        tuckQuietly()
    }

    /// The drag, settle and state blockers, which change inside this controller.
    func syncAutoHideBlockers() {
        let now = CACurrentMediaTime()
        autoHide.setBlocked(.motionRunning, isDragging || motion.isSettling, at: now)
        autoHide.setBlocked(.notShown, state != .shown, at: now)
    }

    static func hiddenTranslationX(availableWidth: CGFloat) -> CGFloat {
        -barMaxX(availableWidth: availableWidth)
    }

    /// Trailing edge of the bar, which is centred and capped at `DSTabBarStyle.maxWidth`.
    static func barMaxX(availableWidth: CGFloat) -> CGFloat {
        let width = DSTabBarStyle.rowWidth(inAvailableWidth: availableWidth)
        return ((availableWidth - width) / 2).rounded() + width
    }
}

/// Relays display link frames to a closure. A display link retains its target, so the link is given this object
/// rather than the controller.
@MainActor
private final class DisplayLinkFrameTarget: NSObject {
    private let onFrame: (CADisplayLink) -> Void

    init(onFrame: @escaping (CADisplayLink) -> Void) {
        self.onFrame = onFrame
    }

    @objc func frame(_ link: CADisplayLink) {
        onFrame(link)
    }
}
