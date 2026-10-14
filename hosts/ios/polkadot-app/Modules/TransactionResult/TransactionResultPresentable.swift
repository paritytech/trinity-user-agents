import Foundation
import SubstrateSdk
import UIKitExt

@MainActor
protocol TransactionResultPresentable {
    func presentTransactionSuccess(
        from view: ControllerBackedProtocol?,
        onDone: TransactionSuccessCompletion?
    )

    func presentTransactionFailure(
        from view: ControllerBackedProtocol?,
        onRetry: TransactionFailureCompletion?
    )
}

@MainActor
extension TransactionResultPresentable {
    func presentTransactionSuccess(
        from view: ControllerBackedProtocol?,
        onDone: TransactionSuccessCompletion?
    ) {
        guard
            let successView = TransactionSuccessViewFactory.create(onDone: onDone)
        else {
            return
        }

        successView.controller.modalPresentationStyle = .fullScreen

        view?.controller.present(successView.controller, animated: true)
    }

    func presentTransactionFailure(
        from view: ControllerBackedProtocol?,
        onRetry: TransactionFailureCompletion? = nil
    ) {
        guard
            let failureView = TransactionFailureViewFactory.createView(for: onRetry)
        else {
            return
        }

        failureView.controller.modalPresentationStyle = .fullScreen

        view?.controller.present(failureView.controller, animated: true)
    }
}
