import UIKit
import DesignSystem
import PolkadotUI

private struct ScreenOverride {
    weak var screen: UIViewController?
    var value: TabBarFoldOverride
}

/// Owns the tab bar's visibility state machine and the transform that realises it. The chrome
/// keeps view installation and layout; this decides which state applies and drives the offset.
@MainActor
final class TabBarFoldController {
    private unowned let foldSurface: UIView

    /// Bounds of the chrome's own full-bleed view; every distance derives from it.
    private let chromeBounds: () -> CGRect
    /// The bottom safe area inset the resting bar's gap depends on.
    private let bottomSafeAreaInset: () -> CGFloat
    /// Receives the Edge glow's rect and opacity. The glow is drawn outside the bar's transform.
    private let glowSink: (CGRect, CGFloat) -> Void
    /// Receives the tucked bar's tap target, or `.zero` when it is not tucked.
    private let grabZoneSink: (CGRect) -> Void
    /// A state other than `.shown` must dismiss any open panel.
    private let closePanel: () -> Void
    /// Reports each committed state.
    private let stateSink: (TabBarVisibilityState) -> Void

    private(set) var state: TabBarVisibilityState = .shown
    private(set) var isTabRoot = true
    /// Set together with the offset, so it can lead `state` during an interactive transition.
    private(set) var isTucked = false

    var isSettling: Bool {
        motion.isSettling
    }

    private var foldDerived: TabBarFoldDerived = .none
    private weak var navigationScreen: UIViewController?
    private var screenOverrides: [ScreenOverride] = []
    private var motion = DockletMotion()
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
        glowSink: @escaping (CGRect, CGFloat) -> Void,
        grabZoneSink: @escaping (CGRect) -> Void,
        closePanel: @escaping () -> Void,
        stateSink: @escaping (TabBarVisibilityState) -> Void
    ) {
        self.foldSurface = foldSurface
        self.chromeBounds = chromeBounds
        self.bottomSafeAreaInset = bottomSafeAreaInset
        self.glowSink = glowSink
        self.grabZoneSink = grabZoneSink
        self.closePanel = closePanel
        self.stateSink = stateSink
    }

    deinit {
        MainActor.assumeIsolated {
            foldAnimator?.cancelInPlace()
            displayLink?.invalidate()
        }
    }

    func update(context: TabBarChromeContext) {
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
        return true
    }

    /// Resolves the visibility state a context would settle on, without committing it — used to
    /// drive the offset alongside an interactive navigation transition.
    func resolvedState(for context: TabBarChromeContext) -> TabBarVisibilityState {
        TabBarVisibilityPolicy.state(derived: context.foldDerived, override: override(for: context.screen))
    }

    /// Moves the chrome toward `state` in sync with an interactive transition. Unlike `apply(state:)`
    /// it runs no animator or display link of its own, so the caller's transition coordinator owns the timing.
    func setInteractiveTarget(_ state: TabBarVisibilityState) {
        isFoldInterpolating = true
        isTucked = state == .tucked
        stopSettling()
        cancelFoldAnimator()
        motion.snap(to: state == .tucked ? .tucked : .shown)
        writeTransform(pose(for: state))
    }

    /// Records the user's choice for the current screen and moves the bar to it. Tapping a tucked bar restores it.
    func setUserOverride(_ override: TabBarFoldOverride) {
        guard state != .hidden else {
            return
        }
        storeOverride(override)
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
        motion.end(time: CACurrentMediaTime(), geometry: geometry)
        if motion.isSettling {
            startSettling()
        }
        let tucks = motion.destination == .tucked
        guard tucks != (state == .tucked) else {
            return
        }
        storeOverride(tucks ? .tucked : .shown)
        commit(tucks ? .tucked : .shown)
    }

    func refresh() {
        apply(state: policyState)
    }
}

private extension TabBarFoldController {
    var currentOverride: TabBarFoldOverride {
        override(for: navigationScreen)
    }

    var policyState: TabBarVisibilityState {
        TabBarVisibilityPolicy.state(derived: foldDerived, override: currentOverride)
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

    func override(for screen: UIViewController?) -> TabBarFoldOverride {
        guard let screen else {
            return .none
        }
        return screenOverrides.first { $0.screen === screen }?.value ?? .none
    }

    func storeOverride(_ override: TabBarFoldOverride) {
        guard let navigationScreen else {
            return
        }
        screenOverrides.removeAll { $0.screen == nil || $0.screen === navigationScreen }
        screenOverrides.append(ScreenOverride(screen: navigationScreen, value: override))
    }

    func apply(state newState: TabBarVisibilityState, isUserDriven: Bool = false) {
        guard newState != state || isFoldInterpolating else {
            return
        }
        isFoldInterpolating = false

        let previous = state
        commit(newState)
        applyVisibility(newState, from: previous, animated: true, isUserDriven: isUserDriven)
    }

    func commit(_ newState: TabBarVisibilityState) {
        state = newState
        isTucked = newState == .tucked
        stateSink(newState)
        if newState != .shown {
            closePanel()
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
        guard animated else {
            motion.snap(to: rest)
            writeTransform(pose(for: state))
            return
        }
        guard state != .hidden, previous != .hidden else {
            motion.snap(to: rest)
            animateSlide(to: pose(for: state))
            return
        }
        motion.settle(to: rest, now: CACurrentMediaTime(), isUserDriven: isUserDriven)
        startSettling()
    }

    /// Slides between the hidden pose and a docklet pose with a plain spring, since the display link only drives
    /// the docklet pose.
    func animateSlide(to transform: CGAffineTransform) {
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
        let haptic = motion.tick(now: time)
        writeTransform(dockletTransform())
        if let haptic {
            playHaptic(haptic)
        }
        if !motion.isSettling {
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
        glowSink(geometry.edgeGlowRect, motion.progress)
        grabZoneSink(motion.destination == .tucked ? geometry.grabZoneRect : .zero)
    }

    /// The docklet pose about the centre of the full-bleed chrome, which is where the chrome's transform is anchored.
    func dockletTransform() -> CGAffineTransform {
        let bounds = chromeBounds()
        return geometry.transform(motion.pose(geometry: geometry), anchor: CGPoint(x: bounds.midX, y: bounds.midY))
    }

    func hiddenTransform() -> CGAffineTransform {
        CGAffineTransform(translationX: Self.hiddenTranslationX(availableWidth: chromeBounds().width), y: 0)
    }

    func pose(for state: TabBarVisibilityState) -> CGAffineTransform {
        state == .hidden ? hiddenTransform() : dockletTransform()
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
