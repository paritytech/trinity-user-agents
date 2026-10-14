import Foundation
import Products
import SubstrateSdk
import UIKit
import UIKitExt

/// Presents the same gaining-privacy sheet transfers use, on the products presentation anchor.
/// Never allowlisted: losing earned privacy is the user's call for every product. The sheet closes
/// only through its buttons, so exactly one of the callbacks answers; a failed presentation is a
/// decline.
final class PaymentPrivacyConfirmer: PaymentPrivacyConfirming, @unchecked Sendable {
    private let router: ProductsRouting

    init(router: ProductsRouting) {
        self.router = router
    }

    func confirmGainingPrivacySpend(amount: Balance) async -> Bool {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                present(amount: amount, on: router) { continuation.resume(returning: $0) }
            }
        }
    }
}

private extension PaymentPrivacyConfirmer {
    @MainActor
    func present(amount: Balance, on router: ProductsRouting, decision: @escaping (Bool) -> Void) {
        guard let chainAsset = PaymentRequestViewFactory.mainChainAsset() else {
            decision(false)
            return
        }

        let sheet = TransferPrivacyViewFactory.createGainingPrivacyConfirmation(
            amount: PaymentRequestViewFactory.formatAmount(amount, chainAsset: chainAsset),
            onSendAnyway: { decision(true) },
            onCancel: { decision(false) }
        )

        if !router.present(view: sheet) {
            decision(false)
        }
    }
}
