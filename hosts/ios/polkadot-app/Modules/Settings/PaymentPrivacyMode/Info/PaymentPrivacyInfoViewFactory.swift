import SwiftUI
import UIKit

/// The coin-quality explainer behind the Payments Privacy Mode info button: a two-page bottom sheet,
/// presented by `SettingsWireframe`.
@MainActor
enum PaymentPrivacyInfoViewFactory {
    static func createView() -> UIViewController {
        let controller = PaymentPrivacyInfoViewController()
        BottomSheetViewFacade.setupBottomSheet(from: controller)
        return controller
    }
}

/// Hosts ``PaymentPrivacyInfoSheet``. Static content, so no presenter: the only events are closing the
/// sheet and the page's natural height changing, which re-fits the autosized sheet.
final class PaymentPrivacyInfoViewController: UIHostingController<PaymentPrivacyInfoSheet> {
    init() {
        super.init(rootView: PaymentPrivacyInfoSheet())
        rootView = PaymentPrivacyInfoSheet(
            onDismiss: { [weak self] in self?.dismiss(animated: true) },
            onHeightChange: { [weak self] height in self?.fitSheet(to: height) }
        )
        view.backgroundColor = .clear
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Setting the preferred size notifies the autosized sheet, which re-lays itself out to that height,
    /// capped at the screen.
    private func fitSheet(to height: CGFloat) {
        guard height > 0, preferredContentSize.height != height else { return }
        preferredContentSize = CGSize(width: 0, height: height)
    }
}
