import Foundation
import UIKit

protocol ChatNavigating {
    func navigateToChat(with chatId: Chat.Id, force: Bool)
}

extension ChatNavigating {
    func navigateToChat(with chatId: Chat.Id, force: Bool) {
        UIApplication.shared.open(AppConfig.DeepLink.chat(chatId, force: force))
    }
}
