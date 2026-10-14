import UIKitExt

@MainActor
protocol CoinagePrivacyPresenting {
    func showGainingPrivacyConfirmation(
        from view: (any ControllerBackedProtocol)?,
        amount: String,
        onSendAnyway: @escaping () -> Void,
        onCancel: @escaping () -> Void
    )
}

extension CoinagePrivacyPresenting {
    func showGainingPrivacyConfirmation(
        from view: (any ControllerBackedProtocol)?,
        amount: String,
        onSendAnyway: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        let sheetView = TransferPrivacyViewFactory.createGainingPrivacyConfirmation(
            amount: amount,
            onSendAnyway: onSendAnyway,
            onCancel: onCancel
        )
        view?.controller.present(sheetView.controller, animated: true)
    }
}
