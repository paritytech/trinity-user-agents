import PolkadotUI
import TipKit
import UIKit
import UIKitExt

@MainActor
final class TabBarTipController: NSObject {
    private let centerButtonAnchor: any UIPopoverPresentationControllerSourceItem
    private let statusStripAnchor: () -> (any UIPopoverPresentationControllerSourceItem)?
    private let sequence: any TabBarTipSequenceProtocol
    private let docklet: DockletHooks

    private weak var host: UIViewController?
    private var observation: Task<Void, Never>?

    /// Which step is currently on screen. A presented `TipUIPopoverViewController` cannot be
    /// asked which tip it holds, and "on screen" differs from `sequence.currentStep` ("should
    /// be on screen") during the window before an anchor resolves.
    private var presentedStep: TabBarTipStep?

    private var isBarLaidOut = false

    init(
        host: UIViewController,
        sequence: any TabBarTipSequenceProtocol,
        centerButtonAnchor: any UIPopoverPresentationControllerSourceItem,
        statusStripAnchor: @escaping () -> (any UIPopoverPresentationControllerSourceItem)?,
        docklet: DockletHooks
    ) {
        self.host = host
        self.sequence = sequence
        self.centerButtonAnchor = centerButtonAnchor
        self.statusStripAnchor = statusStripAnchor
        self.docklet = docklet
        super.init()
    }

    func start() {
        guard observation == nil else {
            return
        }

        let sequence = sequence

        observation = Task { @MainActor in
            await sequence.observeChanges { [weak self] in
                self?.presentCurrent()
            }
        }
    }

    func stop() {
        observation?.cancel()
        observation = nil

        dismiss()
    }

    func setBarShown(_ isShown: Bool) {
        TabBarTips.isBarShownAtRoot = isShown
    }

    /// A tuck landed, so the matching Docklet tip becomes eligible.
    func dockletTuckLanded(_ kind: DockletTuckKind) {
        TabBarTips.isUserTuckLanded = kind == .user
        TabBarTips.isAutoHideLanded = kind == .autoHide
    }

    /// The bar left Tucked or a grab began; a Docklet tip on screen goes away, unseen if it never appeared.
    func dockletTuckEnded() {
        TabBarTips.isUserTuckLanded = false
        TabBarTips.isAutoHideLanded = false
    }

    func setBarLaidOut(_ isLaidOut: Bool) {
        guard isBarLaidOut != isLaidOut else {
            return
        }

        isBarLaidOut = isLaidOut
        TabBarTips.isBarLaidOut = isLaidOut
    }

    func retireForUserInteraction() {
        guard let presentedStep else {
            return
        }

        presentedStep.tip.invalidate(reason: .actionPerformed)
    }

    func dismissForPanel() {
        dismiss()
    }
}

extension TabBarTipController {
    /// What the Docklet tips need from the chrome.
    struct DockletHooks {
        /// The Edge glow the Docklet tips point at.
        let edgeGlowAnchor: UIView
        /// The arrow side the Docklet tips point from, toward the Edge glow.
        let edgeGlowArrowDirection: UIPopoverArrowDirection
        /// Brings the tucked Docklet back when a Docklet tip's body is tapped.
        let onRestoreDocklet: () -> Void
        /// Reports whether any tip is on screen.
        let onTipShown: (Bool) -> Void
    }
}

extension TabBarTipController: UIPopoverPresentationControllerDelegate {
    /// A tap outside the popover dismisses it without a tip action, which counts as the user closing the tip.
    func presentationControllerDidDismiss(_: UIPresentationController) {
        presentedStep?.tip.invalidate(reason: .tipClosed)
        clearPresentedStep()
    }

    /// Keeps every tip a popover on compact width, since the presenter replaces TipKit's popover delegate.
    func adaptivePresentationStyle(
        for _: UIPresentationController,
        traitCollection _: UITraitCollection
    ) -> UIModalPresentationStyle {
        .none
    }
}

private extension TabBarTipController {
    func clearPresentedStep() {
        presentedStep = nil
        docklet.onTipShown(false)
    }

    func present(_ step: TabBarTipStep) {
        guard let host, host.presentedViewController is TipUIPopoverViewController else {
            show(step)
            return
        }

        clearPresentedStep()
        host.dismiss(animated: true) { [weak self] in
            self?.show(step)
        }
    }

    func show(_ step: TabBarTipStep) {
        guard let host,
              host.presentedViewController == nil,
              let anchor = anchorItem(for: step.anchor),
              let frame = anchor.frame(in: host.view),
              !frame.isEmpty
        else {
            return
        }

        let controller = TipUIPopoverViewController(step.tip, sourceItem: anchor)
        configure(controller, for: step, in: host)

        host.present(controller, animated: true)
        presentedStep = step
        step.onShow?()
        docklet.onTipShown(true)
    }

    func configure(_ controller: TipUIPopoverViewController, for step: TabBarTipStep, in host: UIViewController) {
        controller.backgroundColor = .fgPrimary
        let popover = controller.popoverPresentationController
        popover?.backgroundColor = .fgPrimary
        popover?.delegate = self

        switch step.anchor {
        case .centerButton,
             .statusStrip:
            popover?.permittedArrowDirections = [.up, .down]
        case .edgeGlow:
            popover?.permittedArrowDirections = [docklet.edgeGlowArrowDirection]
            popover?.passthroughViews = [host.view]
        }

        controller.viewStyle = TabBarTipViewStyle(onTap: tapHandler(for: step))
    }

    func tapHandler(for step: TabBarTipStep) -> (@MainActor () -> Void)? {
        guard step.tap == .restoreDocklet else {
            return nil
        }
        return { [weak self] in self?.docklet.onRestoreDocklet() }
    }

    func anchorItem(for anchor: TabBarTipAnchor) -> (any UIPopoverPresentationControllerSourceItem)? {
        switch anchor {
        case .centerButton:
            centerButtonAnchor
        case .statusStrip:
            statusStripAnchor()
        case .edgeGlow:
            docklet.edgeGlowAnchor
        }
    }

    func dismiss() {
        clearPresentedStep()

        guard let host, host.presentedViewController is TipUIPopoverViewController else {
            return
        }

        host.dismiss(animated: true)
    }

    /// The single decision point. Idempotent: re-presenting the step already on screen is a
    /// no-op, so a re-emission cannot flicker a live popover.
    func presentCurrent() {
        guard let step = sequence.currentStep else {
            dismiss()
            return
        }

        guard presentedStep?.tip.id != step.tip.id else {
            return
        }

        present(step)
    }
}

extension TipUIPopoverViewController: @retroactive TransientPresentationSkipping {}
