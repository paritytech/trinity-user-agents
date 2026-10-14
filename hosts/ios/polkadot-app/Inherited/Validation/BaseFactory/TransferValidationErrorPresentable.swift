import Foundation
import UIKitExt

@MainActor
protocol TransferValidationErrorPresentable: ValidationErrorPresentable {
    func presentReceiverBalanceTooLow(from view: ControllerBackedProtocol, locale: Locale?)
    func presentSameReceiver(from view: ControllerBackedProtocol, locale: Locale?)
}

extension TransferValidationErrorPresentable {
    func presentReceiverBalanceTooLow(from view: ControllerBackedProtocol, locale _: Locale?) {
        presentIssue(
            with: String(localized: .Validation.amountTooLow),
            on: view
        )
    }

    func presentSameReceiver(from view: ControllerBackedProtocol, locale _: Locale?) {
        presentIssue(
            with: String(localized: .Validation.invalidRecipient),
            on: view
        )
    }
}
