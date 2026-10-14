import UIKit
import DesignSystem
import PolkadotUI

private struct ScreenOverride {
    weak var screen: UIViewController?
    var value: TabBarFoldOverride
}

/// Owns the tab bar's fold/hide state machine and the transform that realises it. The chrome
/// keeps view installation and layout; this decides which state applies and drives the offset.
@MainActor
final class TabBarFoldController {
    private unowned let barView: DSTabBarView
    private unowned let foldSurface: UIView

    /// Bounds of the chrome's own full-bleed view; every fold distance derives from it.
    private let chromeBounds: () -> CGRect
    /// Receives the folded bar's tap target, or `.zero` when it is not folded.
    private let grabZoneSink: (CGRect) -> Void
    /// A state other than `.shown` must dismiss any open panel.
    private let closePanel: () -> Void
    /// Receives each committed visibility state; fires only when the state actually changes.
    private let stateSink: (TabBarVisibilityState) -> Void

    private(set) var state: TabBarVisibilityState = .shown
    private(set) var isTabRoot = true

    private var foldDerived: TabBarFoldDerived = .none
    private weak var navigationScreen: UIViewController?
    private var screenOverrides: [ScreenOverride] = []
    private var pendingFoldVelocity: CGFloat = 0
    private var isFoldInterpolating = false
    private var foldAnimator: UIViewPropertyAnimator?
    private var foldOffsetWidth: CGFloat?

    init(
        barView: DSTabBarView,
        foldSurface: UIView,
        chromeBounds: @escaping () -> CGRect,
        grabZoneSink: @escaping (CGRect) -> Void,
        closePanel: @escaping () -> Void,
        stateSink: @escaping (TabBarVisibilityState) -> Void
    ) {
        self.barView = barView
        self.foldSurface = foldSurface
        self.chromeBounds = chromeBounds
        self.grabZoneSink = grabZoneSink
        self.closePanel = closePanel
        self.stateSink = stateSink
    }

    deinit {
        MainActor.assumeIsolated {
            foldAnimator?.cancelInPlace()
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
        applyVisibility(state, animated: false, initialVelocity: 0)
        return true
    }

    /// Resolves the visibility state a context would settle on, without committing it — used to
    /// drive the offset alongside an interactive navigation transition.
    func resolvedState(for context: TabBarChromeContext) -> TabBarVisibilityState {
        TabBarVisibilityPolicy.state(
            isTabRoot: context.isTabRoot,
            derived: context.foldDerived,
            override: override(for: context.screen)
        )
    }

    /// Moves the chrome toward `state` in sync with an interactive transition. Unlike `apply(state:)`
    /// it runs no animator of its own, so the caller's transition coordinator owns the timing.
    func setInteractiveTarget(_ state: TabBarVisibilityState) {
        isFoldInterpolating = true
        barView.isFolded = state == .folded
        applyFoldOffset(translationX(for: state))
    }

    func setUserOverride(_ override: TabBarFoldOverride, velocityX: CGFloat) {
        guard state != .hidden, !isTabRoot, let navigationScreen else {
            return
        }

        screenOverrides.removeAll { $0.screen == nil || $0.screen === navigationScreen }
        screenOverrides.append(ScreenOverride(screen: navigationScreen, value: override))

        pendingFoldVelocity = velocityX
        refresh()
    }

    func refresh() {
        apply(state: TabBarVisibilityPolicy.state(
            isTabRoot: isTabRoot,
            derived: foldDerived,
            override: currentOverride
        ))
    }
}

private extension TabBarFoldController {
    var currentOverride: TabBarFoldOverride {
        override(for: navigationScreen)
    }

    func override(for screen: UIViewController?) -> TabBarFoldOverride {
        guard let screen else {
            return .none
        }
        return screenOverrides.first { $0.screen === screen }?.value ?? .none
    }

    func apply(state newState: TabBarVisibilityState) {
        let foldVelocity = pendingFoldVelocity
        pendingFoldVelocity = 0

        guard newState != state || isFoldInterpolating else {
            return
        }
        isFoldInterpolating = false

        state = newState
        if newState != .shown {
            closePanel()
        }
        applyVisibility(newState, animated: true, initialVelocity: foldVelocity)
        stateSink(newState)
    }

    func applyVisibility(_ state: TabBarVisibilityState, animated: Bool, initialVelocity: CGFloat) {
        barView.isFolded = state == .folded
        foldOffsetWidth = chromeBounds().width

        let previousFoldAnimator = foldAnimator
        foldAnimator = nil
        previousFoldAnimator?.cancelInPlace()

        let offset = translationX(for: state)
        let apply = { self.applyFoldOffset(offset) }

        guard animated else {
            apply()
            return
        }

        let normalized = foldReferenceDistance(for: state).map { abs(initialVelocity) / $0 } ?? 0
        let timing = UISpringTimingParameters(
            dampingRatio: 0.85,
            initialVelocity: CGVector(dx: normalized, dy: 0)
        )
        let animator = UIViewPropertyAnimator(duration: 0.45, timingParameters: timing)
        animator.addAnimations(apply)
        animator.addCompletion { [weak self] _ in
            self?.foldAnimator = nil
        }
        foldAnimator = animator
        animator.startAnimation()
    }

    func translationX(for state: TabBarVisibilityState) -> CGFloat {
        switch state {
        case .shown:
            0
        case .folded:
            DSTabBarView.foldedTranslationX(availableWidth: chromeBounds().width)
        case .hidden:
            DSTabBarView.hiddenTranslationX(availableWidth: chromeBounds().width)
        }
    }

    /// Normalises gesture velocity by the state's travel distance; `nil` when there is no travel.
    func foldReferenceDistance(for state: TabBarVisibilityState) -> CGFloat? {
        let distance = state == .hidden
            ? abs(DSTabBarView.hiddenTranslationX(availableWidth: chromeBounds().width))
            : abs(DSTabBarView.foldedTranslationX(availableWidth: chromeBounds().width))
        return distance > 0 ? distance : nil
    }

    /// The whole chrome surface is translated because the bar and both panels are siblings
    /// of the glass, so translating the glass alone would leave them behind.
    func applyFoldOffset(_ offset: CGFloat) {
        foldSurface.transform = CGAffineTransform(translationX: offset, y: 0)
        updateFoldGrabZone()
    }

    /// The inset container no longer reaches the screen edge, so the folded bar's tap target
    /// lives on the full-bleed chrome view instead.
    func updateFoldGrabZone() {
        guard barView.isFolded else {
            grabZoneSink(.zero)
            return
        }

        let height = DSTabBarView.capsuleHeight
        grabZoneSink(CGRect(
            x: 0,
            y: chromeBounds().height - DSTabBarView.bottomGap - height,
            width: DSTabBarView.foldGrabZoneWidth,
            height: height
        ))
    }
}
