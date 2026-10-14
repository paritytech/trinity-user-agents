import Foundation
import Foundation_iOS
import PolkadotUI

@MainActor
extension ValidationErrorPresentable {
    func presentIssue(
        with title: String,
        on view: ControllerValidationResultPresentable?
    ) {
        view?.didReceiveValidation(result: .issue(message: title, context: nil))
    }

    func presentInsufficientBalance(
        with message: String,
        on view: ControllerValidationResultPresentable?
    ) {
        let title = String(localized: .Validation.insufficientBalance)
        view?.didReceiveValidation(
            result: .issue(message: title, context: CommonValidationIssueContext.insufficientBalance(message))
        )
    }

    func presentEnterAmount(
        from view: ControllerValidationResultPresentable,
        locale _: Locale?
    ) {
        presentIssue(
            with: String(localized: .Validation.enterAmount),
            on: view
        )
    }

    func presentAmountTooHigh(
        from view: ControllerValidationResultPresentable,
        maxBalance _: String,
        locale _: Locale?
    ) {
        let title = String(localized: .Validation.insufficientBalance)
        let message = String(localized: .Validation.transferNotEnough)
        view.didReceiveValidation(
            result: .issue(message: title, context: CommonValidationIssueContext.insufficientBalance(message))
        )
    }

    func presentFeeNotReceived(from view: ControllerValidationResultPresentable, locale _: Locale?) {
        presentIssue(
            with: String(localized: .Validation.calculatingFee),
            on: view
        )
    }

    func presentFeeNotCalculatedYet(from view: ControllerValidationResultPresentable, locale _: Locale?) {
        view.didReceiveValidation(
            result: .issue(message: "", context: CommonValidationIssueContext.calculatingFee)
        )
    }

    func presentFeeTooHigh(
        from view: ControllerValidationResultPresentable,
        balance _: String,
        fee _: String,
        locale _: Locale?
    ) {
        let message = String(localized: .Validation.feeTooHigh)

        view.didReceiveValidation(
            result: .issue(
                message: String(localized: .Validation.insufficientBalance),
                context: CommonValidationIssueContext.insufficientBalance(message)
            )
        )
    }

    func presentExistentialDepositWarning(
        from view: ControllerValidationResultPresentable,
        amountLost: String,
        minBalance: String,
        action: @escaping () -> Void,
        locale _: Locale?
    ) {
        let viewModel = TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in
                String(localized: .Validation.sendAllWarningTitle).uppercased()
            },
            message: LocalizableResource { _ in
                let string = String(localized: .Validation.sendAllWarningMessage(amountLost, minBalance))

                return .normal(string)
            },
            mainAction: .init(
                title: LocalizableResource { _ in
                    String(localized: .Common.proceed).uppercased()
                },
                handler: action
            ),
            secondaryAction: .init(
                title: LocalizableResource { _ in
                    String(localized: .Common.cancel).uppercased()
                },
                handler: {}
            )
        )

        let infoView = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: MessageSheetStyler(),
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: infoView.controller, preferredHeight: nil)

        view.controller.present(infoView.controller, animated: true)
    }

    func presentIsSystemAccount(
        from view: ControllerValidationResultPresentable?,
        onContinue _: @escaping () -> Void,
        locale _: Locale?
    ) {
        presentIssue(
            with: String(localized: .Validation.invalidRecipient),
            on: view
        )
    }

    func presentMinBalanceViolated(
        from view: ControllerValidationResultPresentable,
        minBalanceForOperation _: String,
        currentBalance _: String,
        needToAddBalance _: String,
        locale _: Locale?
    ) {
        let message = String(localized: .Validation.minBalance)
        presentInsufficientBalance(
            with: message,
            on: view
        )
    }
}
