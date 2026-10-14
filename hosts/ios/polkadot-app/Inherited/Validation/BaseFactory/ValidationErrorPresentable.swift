import Foundation

@MainActor
protocol ValidationErrorPresentable {
    func presentEnterAmount(
        from view: ControllerValidationResultPresentable,
        locale: Locale?
    )
    func presentAmountTooHigh(
        from view: ControllerValidationResultPresentable,
        maxBalance: String,
        locale: Locale?
    )
    func presentFeeNotCalculatedYet(
        from view: ControllerValidationResultPresentable,
        locale: Locale?
    )
    func presentFeeNotReceived(
        from view: ControllerValidationResultPresentable,
        locale: Locale?
    )
    func presentFeeTooHigh(
        from view: ControllerValidationResultPresentable,
        balance: String,
        fee: String,
        locale: Locale?
    )
    func presentExistentialDepositWarning(
        from view: ControllerValidationResultPresentable,
        amountLost: String,
        minBalance: String,
        action: @escaping () -> Void,
        locale: Locale?
    )

    func presentIsSystemAccount(
        from view: ControllerValidationResultPresentable?,
        onContinue: @escaping () -> Void,
        locale: Locale?
    )

    func presentMinBalanceViolated(
        from view: ControllerValidationResultPresentable,
        minBalanceForOperation: String,
        currentBalance: String,
        needToAddBalance: String,
        locale: Locale?
    )
}
