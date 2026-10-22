import Foundation
import UIKitExt

extension ChatWireframe {
    /// The edit history of a message of a room that the product keeps, on the edit history screen of the native
    /// chat.
    func showEditHistory(
        from view: ControllerBackedProtocol?,
        messageId: String,
        interactor: ProductEditHistoryInteractor
    ) {
        let editHistoryView = EditHistoryViewFactory.createView(messageId: messageId, interactor: interactor)
        view?.controller.present(editHistoryView, animated: true)
    }
}
