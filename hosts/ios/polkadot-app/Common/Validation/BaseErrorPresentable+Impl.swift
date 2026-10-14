import Foundation
import Foundation_iOS
import UIKitExt

extension ValidationErrorPresentable {
    func presentIssue(with title: String, on view: ControllerBackedProtocol?) {
        guard let view = view as? ValidationResultPresentable else {
            return
        }

        view.didReceiveValidation(result: .issue(message: title, context: nil))
    }
}
