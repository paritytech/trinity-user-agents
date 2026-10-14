import Foundation
import UIKitExt

extension FeeRetryable where Self: AlertPresentable {
    func presentFeeStatus(
        on view: ControllerBackedProtocol?,
        locale _: Locale?,
        retryAction: @escaping () -> Void
    ) {
        let retryViewModel = AlertPresentableAction(
            title: String(localized: .Common.retry),
            handler: retryAction
        )

        let title = String(localized: .Common.error)
        let message = String(localized: .Common.feeCalcFailed)

        let viewModel = AlertPresentableViewModel(
            title: title,
            message: message,
            actions: [retryViewModel],
            closeActionTitle: String(localized: .Common.skip)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }
}
