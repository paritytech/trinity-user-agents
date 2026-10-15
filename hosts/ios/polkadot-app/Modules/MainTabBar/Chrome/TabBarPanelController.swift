import UIKit
import DesignSystem
import PolkadotUI

/// Owns the panel open/close state machine and the animation that realises it. The chrome
/// keeps view installation and layout; this decides which panel applies and drives the transition.
@MainActor
final class TabBarPanelController {
    private unowned let surface: TabBarChromeSurfaceView

    /// Receives each committed panel state; fires only when the state actually changes.
    private let onPanelChanged: (TabBarPanelKind?) -> Void
    /// Receives the open panel and triggers active action index or outside-tap state updates.
    private let onOpenPanelChanged: (TabBarPanelKind?) -> Void
    /// Dismisses tips when a panel opens.
    private let dismissTips: () -> Void
    /// Detaches content controller and clears content from surface.
    private let tearDownContent: () -> Void
    /// Updates backdrop view visibility and animation.
    private let setBackdropOpen: (Bool, UIViewPropertyAnimator?) -> Void

    private(set) var open: TabBarPanelKind?

    private var panelAnimator: UIViewPropertyAnimator?
    private var pendingPanel: TabBarPanelKind?
    private var isApplyingPanel = false
    private var hasPendingContentPanelResize = false

    init(
        surface: TabBarChromeSurfaceView,
        onPanelChanged: @escaping (TabBarPanelKind?) -> Void,
        onOpenPanelChanged: @escaping (TabBarPanelKind?) -> Void,
        dismissTips: @escaping () -> Void,
        tearDownContent: @escaping () -> Void,
        setBackdropOpen: @escaping (Bool, UIViewPropertyAnimator?) -> Void
    ) {
        self.surface = surface
        self.onPanelChanged = onPanelChanged
        self.onOpenPanelChanged = onOpenPanelChanged
        self.dismissTips = dismissTips
        self.tearDownContent = tearDownContent
        self.setBackdropOpen = setBackdropOpen
    }

    deinit {
        MainActor.assumeIsolated {
            panelAnimator?.cancelInPlace()
        }
    }

    func setPanel(_ kind: TabBarPanelKind?, animated: Bool) {
        pendingPanel = nil
        // A resize owed by the outgoing content must not land on whatever replaces it.
        hasPendingContentPanelResize = false

        // `open` is cleared the moment a close starts, so a second close during that animation
        // would cancel it in place and leave the backdrop and panel frozen mid-way. The pending
        // reopen above is still cancelled, which is all a repeated close can mean.
        guard kind != nil || open != nil else {
            return
        }

        let previousPanel = open
        let animator = animated ? makePanelAnimator() : nil

        setBackdropOpen(kind != nil, animator)
        surface.setPanelsOpen(kind, animator: animator)

        if kind != nil {
            dismissTips()
        }

        open = kind
        onOpenPanelChanged(kind)

        // Content is requested before the height is measured, so the open animates
        // straight to its final size and the scanner's capture session warms up during
        // the animation rather than after it. `isApplyingPanel` stops that push starting
        // a rival animator.
        if previousPanel != kind {
            isApplyingPanel = true
            onPanelChanged(kind)
            isApplyingPanel = false
        }

        surface.updateHeight(for: kind, animator: animator)

        // The scanner's capture session must be released once the panel is gone, so the teardown
        // rides the same animator and still runs when there is none (a fold closes unanimated).
        if previousPanel?.contentAction != nil, kind?.contentAction == nil {
            let teardown = { [weak self] in self?.tearDownContent() }
            if let animator {
                animator.addCompletion { _ in teardown() }
            } else {
                teardown()
            }
        }

        // `togglePanel` sets `pendingPanel` after this close returns, so the reopen is read at
        // completion time: a fold or another tap in between clears it and cancels the switch.
        if kind == nil, let animator {
            animator.addCompletion { [weak self] _ in
                guard let self, let pendingPanel else {
                    return
                }
                self.pendingPanel = nil
                setPanel(pendingPanel, animated: true)
            }
        }

        animator?.startAnimation()
    }

    /// Selecting a different action closes the open panel before opening the new one, so the
    /// change reads as a close followed by an open instead of a silent content swap.
    func togglePanel(_ kind: TabBarPanelKind) {
        guard let open else {
            setPanel(kind, animated: true)
            return
        }

        guard open != kind else {
            setPanel(nil, animated: true)
            return
        }

        setPanel(nil, animated: true)
        pendingPanel = kind
    }

    /// A push that arrives while `setPanel` is applying is already covered by the
    /// open animation.
    ///
    /// One that arrives while an animation is running waits for it. Resizing there would cancel
    /// the open and strand the container at whatever height it had reached, and the size it would
    /// aim for is measured before SwiftUI has laid out the content that just changed, so the panel
    /// settles on the previous content's height.
    func resizeForContentPanel() {
        guard !isApplyingPanel, open?.contentAction != nil else {
            return
        }

        guard panelAnimator == nil else {
            deferResizeForContentPanel()
            return
        }

        let animator = makePanelAnimator()

        // An animator with no animation blocks never completes, so it would block every later
        // resize behind a completion that never fires.
        guard surface.updateHeight(for: open, animator: animator) else {
            panelAnimator = nil
            return
        }

        animator.startAnimation()
    }

    /// Re-measures the open panel after the chip list changed. Only an open SPA-tabs panel
    /// animates; any other state settles the height without an animator.
    func refreshHeightAfterChipsChange() {
        let animator = open == .spaTabs ? makePanelAnimator() : nil
        surface.updateHeight(for: open, animator: animator)
        animator?.startAnimation()
    }

    /// Animates the container back to the open panel's height after an interrupted interactive drag.
    func restoreOpenHeight(completion: @escaping () -> Void) {
        let animator = makePanelAnimator()

        // An animator with no animation blocks never completes, so it would block every later
        // resize behind a completion that never fires.
        guard surface.updateHeight(for: open, animator: animator) else {
            panelAnimator = nil
            completion()
            return
        }

        animator.addCompletion { _ in completion() }
        animator.startAnimation()
    }

    /// Hands the container over to an interactive drag by ending any animation still running,
    /// leaving it at whatever height it had reached.
    func cancelHeightAnimation() {
        panelAnimator?.cancelInPlace()
    }

    /// Settles the open panel's height on a layout pass, unanimated.
    func refreshHeightAfterLayout() {
        surface.updateHeight(for: open, animator: nil)
    }
}

private extension TabBarPanelController {
    /// One animator drives the panel contents and the container resize so they cannot drift apart.
    func makePanelAnimator() -> UIViewPropertyAnimator {
        let previousPanelAnimator = panelAnimator
        panelAnimator = nil
        previousPanelAnimator?.cancelInPlace()

        let animator = UIViewPropertyAnimator(
            duration: DSTabBarTabsPanelView.openDuration,
            dampingRatio: DSTabBarTabsPanelView.openDampingRatio
        )
        animator.addCompletion { [weak self] _ in
            self?.panelAnimator = nil
        }
        panelAnimator = animator

        return animator
    }

    /// Every push during one animation is owed the same single resize, measured once the
    /// animation — and with it the pending SwiftUI layout — has settled.
    func deferResizeForContentPanel() {
        guard !hasPendingContentPanelResize, let panelAnimator else {
            return
        }

        hasPendingContentPanelResize = true
        panelAnimator.addCompletion { [weak self] _ in
            guard let self, hasPendingContentPanelResize else {
                return
            }
            hasPendingContentPanelResize = false
            resizeForContentPanel()
        }
    }
}
