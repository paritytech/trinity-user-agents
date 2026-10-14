import Foundation
import UIKitExt

extension CommonRetryable where Self: AlertPresentable {
    func presentRequestStatus(
        on view: ControllerBackedProtocol?,
        title: String,
        message: String,
        locale: Locale? = nil,
        retryAction: @escaping () -> Void
    ) {
        presentRequestStatus(
            on: view,
            title: title,
            message: message,
            cancelAction: String(localized: .Common.skip),
            locale: locale,
            retryAction: retryAction
        )
    }

    func presentRequestStatus(
        on view: ControllerBackedProtocol?,
        locale: Locale? = nil,
        retryAction: @escaping () -> Void
    ) {
        let title = String(localized: .Common.error)
        let message = String(localized: .Common.requestRetry)

        presentRequestStatus(
            on: view,
            title: title,
            message: message,
            cancelAction: String(localized: .Common.skip),
            locale: locale,
            retryAction: retryAction
        )
    }

    func presentRequestStatus(
        on view: ControllerBackedProtocol?,
        title: String,
        message: String,
        cancelAction: String,
        locale _: Locale?,
        retryAction: @escaping () -> Void
    ) {
        let retryViewModel = AlertPresentableAction(
            title: String(localized: .Common.retry),
            handler: retryAction
        )

        let viewModel = AlertPresentableViewModel(
            title: title,
            message: message,
            actions: [retryViewModel],
            closeActionTitle: cancelAction
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }

    func presentTryAgainOperation(
        on view: ControllerBackedProtocol?,
        title: String,
        message: String,
        actionTitle: String,
        retryAction: @escaping () -> Void
    ) {
        let retryViewModel = AlertPresentableAction(
            title: actionTitle,
            handler: retryAction
        )

        let viewModel = AlertPresentableViewModel(
            title: title,
            message: message,
            actions: [retryViewModel]
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }
}
