import Foundation
import Keystore_iOS

/// Shows the Docklet's two tips beside the Edge glow, once each per device, and restores the bar when the tip is used.
@MainActor
final class DockletTipController {
    /// Seconds between a tuck landing and its tip appearing.
    private static let appearDelaySeconds: TimeInterval = 0.42

    /// The tip view. The chrome adds it to its root.
    let tipView = DockletTipView()

    private let settingsManager: SettingsManagerProtocol
    private let glowFrame: () -> CGRect
    private let isAnotherTipShowing: () -> Bool
    private let foldController: TabBarFoldController
    private var pendingTask: Task<Void, Never>?
    private var shownTip: DockletTip?

    init(
        settingsManager: SettingsManagerProtocol,
        glowFrame: @escaping () -> CGRect,
        isAnotherTipShowing: @escaping () -> Bool,
        foldController: TabBarFoldController
    ) {
        self.settingsManager = settingsManager
        self.glowFrame = glowFrame
        self.isAnotherTipShowing = isAnotherTipShowing
        self.foldController = foldController
        tipView.onTap = { [weak self] in
            self?.restore(translationX: 0)
        }
        tipView.onDragEnded = { [weak self] translationX in
            self?.finishDrag(translationX: translationX)
        }
    }

    /// Schedules the tip for a tuck that just landed. A drag or a restore before it appears cancels it.
    func tuckLanded(_ kind: DockletTuckKind) {
        cancelPending()
        pendingTask = Task.delayed(by: Self.appearDelaySeconds) { [weak self] in
            self?.showTip(for: kind)
        }
    }

    /// Stops a tip that has not appeared yet. Its flags stay unseen, so a later tuck can still show it.
    func cancelPending() {
        pendingTask?.cancel()
        pendingTask = nil
    }

    /// A bar that leaves the tucked state by any path other than the tip ends the tip.
    func barStateChanged(_ state: TabBarVisibilityState) {
        if state != .tucked {
            cancelPending()
            dismiss(translationX: 0)
        }
    }
}

private extension DockletTipController {
    func showTip(for kind: DockletTuckKind) {
        pendingTask = nil
        guard let decision = DockletTipPolicy.decision(
            for: kind,
            seen: seenTips(),
            isAnotherTipShowing: isAnotherTipShowing()
        ) else {
            return
        }
        for tip in decision.tipsSeen {
            settingsManager.set(value: true, for: tip.settingsKey)
        }
        shownTip = decision.tip
        foldController.setAutoHideBlocked(.tipShown, true)
        tipView.present(title: decision.tip.title, message: decision.tip.message, beside: glowFrame())
    }

    func seenTips() -> Set<DockletTip> {
        Set(DockletTip.allCases.filter { settingsManager.value(for: $0.settingsKey) })
    }

    func dismiss(translationX: CGFloat) {
        guard shownTip != nil else {
            return
        }
        shownTip = nil
        foldController.setAutoHideBlocked(.tipShown, false)
        tipView.flyOff(translationX: translationX)
    }

    /// Restores the bar the way a Grab-zone tap does, as the user's own restore.
    func restore(translationX: CGFloat) {
        dismiss(translationX: translationX)
        foldController.setUserOverride(.shown)
    }

    func finishDrag(translationX: CGFloat) {
        guard DockletTipPolicy.restores(afterDragging: translationX) else {
            tipView.snapBack()
            return
        }
        restore(translationX: translationX)
    }
}

private extension DockletTip {
    var settingsKey: SettingsKey {
        switch self {
        case .tucked:
            .dockletTuckedTipShown
        case .autoHidden:
            .dockletAutoHiddenTipShown
        }
    }

    var title: String {
        switch self {
        case .tucked:
            String(localized: .Tips.tuckedTitle)
        case .autoHidden:
            String(localized: .Tips.autoHiddenTitle)
        }
    }

    var message: String {
        switch self {
        case .tucked:
            String(localized: .Tips.tuckedMessage)
        case .autoHidden:
            String(localized: .Tips.autoHiddenMessage)
        }
    }
}
