import Foundation
import UIKit

@MainActor
final class ChatAttachmentsWireframe: ChatAttachmentsWireframeProtocol {
    func dismiss(from view: ChatAttachmentsViewProtocol?, completion: @escaping () -> Void) {
        view?.controller.dismiss(animated: true, completion: completion)
    }
}
