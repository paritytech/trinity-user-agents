import Foundation
import UIKitExt

protocol BottomSheetErrorPresentable: BottomSheetMessagePresentable, ErrorPresentable {}

@MainActor
extension BottomSheetErrorPresentable {
    @discardableResult
    func present(error: ErrorContent, from view: ControllerBackedProtocol?) -> Bool {
        guard let view else {
            return false
        }

        showMessageBottomSheet(
            from: view,
            title: error.title,
            message: error.message,
            close: String(localized: .Common.gotIt),
            preferredHeight: 0
        )

        return true
    }
}
