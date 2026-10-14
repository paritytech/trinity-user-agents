import UIKit
import UIKitExt

/// Wireframe for the interactive signing flow. Owns the signing `context` so
/// the dismiss completion is the single place that delivers the result to the
/// requester — a successful response is sent only after the sheet is gone.
/// A failed delivery (e.g. a rejected host post) surfaces as an alert on the
/// bottom sheet's presentation controller, matching the old in-sheet handling.
@MainActor
final class PolkadotSigningWireframe: PolkadotSigningWireframeProtocol {
    private let signingContext: PolkadotSigningContextProtocol
    private let logger: LoggerProtocol

    init(
        signingContext: PolkadotSigningContextProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.signingContext = signingContext
        self.logger = logger
    }

    func hide(view: PolkadotSigningViewProtocol?, decision: PolkadotSigningDecision) {
        guard let controller = view?.controller else {
            deliver(decision, presentedFrom: nil)
            return
        }
        let presentationController = controller.presentingViewController
        // Retain self and the presentation controller strongly: the VIPER module
        // deallocates as the sheet dismisses, so a weak capture would skip
        // delivery and the submission-error alert. The strong refs are released
        // once the delivery task finishes.
        controller.dismiss(animated: true) {
            self.deliver(decision, presentedFrom: presentationController)
        }
    }
}

private extension PolkadotSigningWireframe {
    func deliver(_ decision: PolkadotSigningDecision, presentedFrom presenter: UIViewController?) {
        Task {
            do {
                switch decision {
                case let .signed(result):
                    try await self.signingContext.sendResult(result)
                case .rejected:
                    try await self.signingContext.rejectRequest()
                }
            } catch {
                self.handleDeliveryFailure(error, presentedFrom: presenter)
            }
        }
    }

    func handleDeliveryFailure(_ error: Error, presentedFrom presenter: UIViewController?) {
        guard let message = submissionErrorMessage(for: error) else {
            logger.error("Signing delivery failed: \(error)")
            return
        }

        guard let controller = presenter ?? UIWindow.topWindow?.rootViewController else {
            return
        }

        UIAlertController.present(
            message: message,
            title: String(localized: .Common.error),
            closeAction: String(localized: .Common.close),
            with: controller
        )
    }

    func submissionErrorMessage(for error: Error) -> String? {
        guard let error = error as? PolkadotHostMessageError else {
            return nil
        }

        switch error {
        case .submissionFailed:
            return String(localized: .polkadotHostPostError)
        case let .messageTooBig(maxSize, actualSize):
            return String(
                localized: .polkadotHostMessageTooBigError(
                    actualSize: ByteSizeFormatter.string(fromBytes: actualSize),
                    maxSize: ByteSizeFormatter.string(fromBytes: maxSize)
                )
            )
        case .deliveryFailed,
             .timeout,
             .other,
             .serviceExpected:
            return nil
        }
    }
}
