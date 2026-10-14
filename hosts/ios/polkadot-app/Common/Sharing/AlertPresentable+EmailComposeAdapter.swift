import Foundation
import UIKitExt

extension AlertPresentable {
    func presentMailNotAvailableError(from view: ControllerBackedProtocol?) {
        present(
            message: String(localized: .Common.errorMailNotAvailable),
            title: String(localized: .Common.error),
            closeAction: String(localized: .Common.close),
            from: view
        )
    }
}
