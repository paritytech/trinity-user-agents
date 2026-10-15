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
    /// Width of the folded bar that stays on screen at the leading edge.
    private static let foldedVisibleWidth: CGFloat = 19
    /// Width of the strip at the leading edge that unfolds the folded bar.
    private static let grabZoneWidth: CGFloat = 44

    private unowned let foldSurface: UIView

    /// Bounds of the chrome's own full-bleed view; every fold distance derives from it.
    private let chromeBounds: () -> CGRect
    /// Receives the folded bar's tap target, or `.zero` when it is not folded.
    private let grabZoneSink: (CGRect) -> Void
    /// A state other than `.shown` must dismiss any open panel.
    private let closePanel: () -> Void
    /// Reports each committed state.
    private let stateSink: (TabBarVisibilityState) -> Void

    private(set) var state: TabBarVisibilityState = .shown
    private(set) var isTabRoot = true
    /// Set together with the offset, so it can lead `state` during an interactive transition.
    private(set) var isFolded = false

    private var foldDerived: TabBarFoldDerived = .none
    private weak var navigationScreen: UIViewController?
    private var screenOverrides: [ScreenOverride] = []
    private var pendingFoldVelocity: CGFloat = 0
    private var isFoldInterpolating = false
    private var foldAnimator: UIViewPropertyAnimator?
    private var foldOffsetWidth: CGFloat?
    private var foldOffset: CGFloat = 0
    /// The lift is owned here because this controller writes the transform that carries it.
    private(set) var keyboardLift: CGFloat = 0

    init(
        foldSurface: UIView,
        chromeBounds: @escaping () -> CGRect,
        grabZoneSink: @escaping (CGRect) -> Void,
        closePanel: @escaping () -> Void,
        stateSink: @escaping (TabBarVisibilityState) -> Void
    ) {
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
        isFolded = state == .folded
        applyFoldOffset(translationX(for: state))
    }

    /// Lifts the chrome above the keyboard. It composes with the fold translation rather than replacing it.
    func setKeyboardLift(_ lift: CGFloat) {
        keyboardLift = lift
        updateFoldTransform()
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

    /// Folds or unfolds from a horizontal pan on the bar. The fold distance comes from the chrome's own width,
    /// which is the width the fold travels across.
    func foldPanEnded(translationX: CGFloat, velocityX: CGFloat) {
        let decision = TabBarFoldDecision.decide(
            velocityX: velocityX,
            translationX: translationX,
            isFolded: isFolded,
            foldDistance: Self.foldedTranslationX(availableWidth: chromeBounds().width)
        )

        switch decision {
        case .fold:
            setUserOverride(.folded, velocityX: velocityX)
        case .unfold:
            setUserOverride(.shown, velocityX: velocityX)
        // Neither a fold nor an unfold. The bar takes no action on it, and taps are handled by the slots.
        case .select,
             .settle:
            break
        }
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
        stateSink(newState)
        if newState != .shown {
            closePanel()
        }
        applyVisibility(newState, animated: true, initialVelocity: foldVelocity)
    }

    func applyVisibility(_ state: TabBarVisibilityState, animated: Bool, initialVelocity: CGFloat) {
        isFolded = state == .folded
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
            Self.foldedTranslationX(availableWidth: chromeBounds().width)
        case .hidden:
            Self.hiddenTranslationX(availableWidth: chromeBounds().width)
        }
    }

    /// Normalises gesture velocity by the state's travel distance; `nil` when there is no travel.
    func foldReferenceDistance(for state: TabBarVisibilityState) -> CGFloat? {
        let distance = state == .hidden
            ? abs(Self.hiddenTranslationX(availableWidth: chromeBounds().width))
            : abs(Self.foldedTranslationX(availableWidth: chromeBounds().width))
        return distance > 0 ? distance : nil
    }

    /// The whole chrome surface is translated, so the bar and any open panel move together.
    func applyFoldOffset(_ offset: CGFloat) {
        foldOffset = offset
        updateFoldTransform()
        updateFoldGrabZone()
    }

    /// The only writer of the chrome's transform. Fold and keyboard are both translations, so one transform
    /// carries both and each setter keeps the other's value.
    func updateFoldTransform() {
        foldSurface.transform = CGAffineTransform(translationX: foldOffset, y: -keyboardLift)
    }

    /// The inset container no longer reaches the screen edge, so the folded bar's tap target
    /// lives on the full-bleed chrome view instead.
    func updateFoldGrabZone() {
        guard isFolded else {
            grabZoneSink(.zero)
            return
        }

        let height = DSTabBarStyle.height
        grabZoneSink(CGRect(
            x: 0,
            y: chromeBounds().height - DSTabBarStyle.bottomGap - height,
            width: Self.grabZoneWidth,
            height: height
        ))
    }

    static func foldedTranslationX(availableWidth: CGFloat) -> CGFloat {
        foldedVisibleWidth - barMaxX(availableWidth: availableWidth)
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
