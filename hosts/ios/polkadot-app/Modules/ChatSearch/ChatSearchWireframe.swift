import Foundation
import UIKit

@MainActor
final class ChatSearchWireframe: ChatSearchWireframeProtocol {
    /// Shows a message in the room screen under the search.
    private let showMessage: (Chat.MessageId) -> Void

    init(showMessage: @escaping (Chat.MessageId) -> Void) {
        self.showMessage = showMessage
    }

    func close(from view: ChatSearchViewProtocol?, showing messageId: Chat.MessageId) {
        view?.controller.navigationController?.popViewController(animated: true)
        showMessage(messageId)
    }
}
