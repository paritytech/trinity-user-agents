import PolkadotUI
import TipKit
import UIKit
import UIKitExt

@MainActor
final class TabBarTipController {
    private let centerButtonAnchor: any UIPopoverPresentationControllerSourceItem
    private let statusStripAnchor: () -> (any UIPopoverPresentationControllerSourceItem)?
    private let sequence: any TabBarTipSequenceProtocol

    private weak var host: UIViewController?
    private var observation: Task<Void, Never>?

    /// Which tip is currently on screen. A presented `TipUIPopoverViewController` cannot be
    /// asked which tip it holds, and "on screen" differs from `sequence.currentStep` ("should
    /// be on screen") during the window before an anchor resolves.
    private var presentedTip: AnyTip?

    private var isBarLaidOut = false

    init(
        host: UIViewController,
        sequence: any TabBarTipSequenceProtocol,
        centerButtonAnchor: any UIPopoverPresentationControllerSourceItem,
        statusStripAnchor: @escaping () -> (any UIPopoverPresentationControllerSourceItem)?
    ) {
        self.host = host
        self.sequence = sequence
        self.centerButtonAnchor = centerButtonAnchor
        self.statusStripAnchor = statusStripAnchor
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

    func setBarLaidOut(_ isLaidOut: Bool) {
        guard isBarLaidOut != isLaidOut else {
            return
        }

        isBarLaidOut = isLaidOut
        TabBarTips.isBarLaidOut = isLaidOut
    }

    func retireForUserInteraction() {
        guard let presentedTip else {
            return
        }

        presentedTip.invalidate(reason: .actionPerformed)
    }

    func dismissForPanel() {
        dismiss()
    }
}

private extension TabBarTipController {
    func present(_ step: TabBarTipStep) {
        guard let host, host.presentedViewController is TipUIPopoverViewController else {
            show(step)
            return
        }

        presentedTip = nil
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
        controller.popoverPresentationController?.permittedArrowDirections = [.up, .down]
        controller.popoverPresentationController?.backgroundColor = .fgPrimary
        controller.viewStyle = TabBarTipViewStyle()

        host.present(controller, animated: true)
        presentedTip = step.tip
    }

    func anchorItem(for anchor: TabBarTipAnchor) -> (any UIPopoverPresentationControllerSourceItem)? {
        switch anchor {
        case .centerButton:
            centerButtonAnchor
        case .statusStrip:
            statusStripAnchor()
        }
    }

    func dismiss() {
        presentedTip = nil

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

        guard presentedTip?.id != step.tip.id else {
            return
        }

        present(step)
    }
}

extension TipUIPopoverViewController: @retroactive TransientPresentationSkipping {}
