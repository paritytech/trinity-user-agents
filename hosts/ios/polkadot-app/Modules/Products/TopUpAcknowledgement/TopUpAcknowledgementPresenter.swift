import Coinage
import Foundation
import Products
import SubstrateSdk
import UIKit
import UIKitExt

/// Surfaces a top-up's unhappy ending to the user, raised from the durable operation on settle (the
/// `paymentTopUp` call has long since returned). Only the two unhappy terminal outcomes reach here.
///
/// The prompt is app-global: it can settle while any product context — or none — is on screen, so it
/// presents on the key window's topmost controller rather than a per-product router.
final class TopUpAcknowledgementPresenter: IncomingPaymentAcknowledging, @unchecked Sendable {
    private let logger: LoggerProtocol

    init(logger: LoggerProtocol = Logger.shared) {
        self.logger = logger
    }

    func acknowledge(
        productId: String,
        paymentId _: IncomingPaymentId,
        requestedAmount: Balance,
        outcome: IncomingPaymentTerminalOutcome
    ) async {
        switch outcome {
        case let .claimedPartially(actualClaimed):
            await presentMismatch(
                productId: productId,
                claimedAmount: actualClaimed,
                requestedAmount: requestedAmount
            )
        case .notClaimed:
            await presentError(productId: productId)
        case .claimed:
            break
        }
    }
}

private extension TopUpAcknowledgementPresenter {
    @MainActor
    func presentMismatch(productId: ProductId, claimedAmount: Balance, requestedAmount: Balance) {
        let view = TopUpAcknowledgementViewFactory.createMismatchView(
            productId: productId,
            claimedAmount: claimedAmount,
            requestedAmount: requestedAmount
        )
        present(view)
    }

    @MainActor
    func presentError(productId: ProductId) {
        present(TopUpAcknowledgementViewFactory.createErrorView(productId: productId))
    }

    @MainActor
    func present(_ view: (any ControllerBackedProtocol)?) {
        guard let view else {
            logger.warning("Top-up acknowledgement could not build its screen")
            return
        }

        guard let anchor = UIWindow.keyWindow?.topmostViewController else {
            logger.warning("Top-up acknowledgement has no window to present on")
            return
        }

        anchor.present(view.controller, animated: true)
    }
}
