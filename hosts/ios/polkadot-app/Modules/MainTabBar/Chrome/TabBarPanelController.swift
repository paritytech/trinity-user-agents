import UIKit
import DesignSystem

/// Owns the panel open/close state machine and the animation that realises it. The chrome
/// keeps view installation and layout; this decides which panel applies and drives the transition.
@MainActor
final class TabBarPanelController {
    private static let openDuration: TimeInterval = 0.18
    private static let openDampingRatio: CGFloat = 0.9

    /// Receives each committed panel state; fires only when the state actually changes.
    private let onPanelChanged: (TabBarPanelKind?) -> Void
    /// Receives the open panel and triggers active action index or outside-tap state updates.
    private let onOpenPanelChanged: (TabBarPanelKind?) -> Void
    /// Updates backdrop view visibility and animation.
    private let setBackdropOpen: (Bool, UIViewPropertyAnimator?) -> Void

    private(set) var open: TabBarPanelKind?

    private var panelAnimator: UIViewPropertyAnimator?
    private var pendingPanel: TabBarPanelKind?

    init(
        onPanelChanged: @escaping (TabBarPanelKind?) -> Void,
        onOpenPanelChanged: @escaping (TabBarPanelKind?) -> Void,
        setBackdropOpen: @escaping (Bool, UIViewPropertyAnimator?) -> Void
    ) {
        self.onPanelChanged = onPanelChanged
        self.onOpenPanelChanged = onOpenPanelChanged
        self.setBackdropOpen = setBackdropOpen
    }

    deinit {
        MainActor.assumeIsolated {
            panelAnimator?.cancelInPlace()
        }
    }

    func setPanel(_ kind: TabBarPanelKind?, animated: Bool) {
        pendingPanel = nil

        // `open` is cleared the moment a close starts, so a second close during that animation
        // would cancel it in place and leave the backdrop and panel frozen mid-way. The pending
        // reopen above is still cancelled, which is all a repeated close can mean.
        guard kind != nil || open != nil else {
            return
        }

        let previousPanel = open
        let animator = animated ? makePanelAnimator() : nil

        setBackdropOpen(kind != nil, animator)

        open = kind
        onOpenPanelChanged(kind)

        if previousPanel != kind {
            onPanelChanged(kind)
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
}

private extension TabBarPanelController {
    /// Drives the backdrop. The panel contents animate themselves, in SwiftUI.
    func makePanelAnimator() -> UIViewPropertyAnimator {
        let previousPanelAnimator = panelAnimator
        panelAnimator = nil
        previousPanelAnimator?.cancelInPlace()

        let animator = UIViewPropertyAnimator(
            duration: Self.openDuration,
            dampingRatio: Self.openDampingRatio
        )
        animator.addCompletion { [weak self] _ in
            self?.panelAnimator = nil
        }
        panelAnimator = animator

        return animator
    }
}
