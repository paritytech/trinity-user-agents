import Foundation
import UIKitExt

extension ErrorPresentable where Self: AlertPresentable {
    @discardableResult
    func present(error: ErrorContent, from view: ControllerBackedProtocol?) -> Bool {
        present(
            message: error.message,
            title: error.title,
            closeAction: String(localized: .Common.close),
            from: view
        )

        return true
    }

    @discardableResult
    func present(
        error: Error,
        from view: ControllerBackedProtocol?,
        completion: @escaping () -> Void
    ) -> Bool {
        guard let content = errorContent(from: error) else {
            return false
        }

        let closeAction = AlertPresentableAction(
            title: String(localized: .Common.close),
            style: .cancel,
            handler: completion
        )

        let viewModel = AlertPresentableViewModel(
            title: content.title,
            message: content.message,
            actions: [closeAction],
            closeActionTitle: nil
        )

        present(
            viewModel: viewModel,
            style: .alert,
            from: view
        )

        return true
    }
}
