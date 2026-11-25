import Foundation
import Products
import SubstrateSdk
import UIKit
import UIKitExt

/// Presents the same gaining-privacy sheet transfers use, on the products presentation anchor.
/// Never allowlisted: losing earned privacy is the user's call for every product. The answer is
/// given once: by a button, as a decline when the presentation fails, or as a decline when the
/// sheet goes away unanswered.
final class PaymentPrivacyConfirmer: PaymentPrivacyConfirming, @unchecked Sendable {
    private let router: ProductsRouting

    init(router: ProductsRouting) {
        self.router = router
    }

    func confirmGainingPrivacySpend(amount: Balance) async -> Bool {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                present(amount: amount, on: router, answer: PrivacyAnswer { continuation.resume(returning: $0) })
            }
        }
    }
}

private extension PaymentPrivacyConfirmer {
    @MainActor
    func present(amount: Balance, on router: ProductsRouting, answer: PrivacyAnswer) {
        guard let chainAsset = PaymentRequestViewFactory.mainChainAsset() else {
            answer.deliver(false)
            return
        }

        let sheet = TransferPrivacyViewFactory.createGainingPrivacyConfirmation(
            amount: PaymentRequestViewFactory.formatAmount(amount, chainAsset: chainAsset),
            onSendAnyway: { answer.deliver(true) },
            onCancel: { answer.deliver(false) }
        )

        if !router.present(view: sheet) {
            answer.deliver(false)
        }
    }
}

/// Answers the privacy prompt once, and declines when the sheet holding it
/// goes away unanswered.
private final class PrivacyAnswer {
    private var resume: ((Bool) -> Void)?

    init(_ resume: @escaping (Bool) -> Void) {
        self.resume = resume
    }

    deinit {
        resume?(false)
    }

    func deliver(_ decision: Bool) {
        resume?(decision)
        resume = nil
    }
}
