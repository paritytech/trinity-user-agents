import Foundation

/// Opens the room of the chat product that a notification names, as the native chat opens the chat of a contact.
final class ProductRoomPushRouteHandler: ChatPushRouteHandler {
    override func handle(_ route: PushNavigationRoute) -> Bool {
        guard case let .productRoom(productId, roomId) = route else {
            return false
        }

        let chatId = Chat.Id.chatExtension(productId, roomId: roomId)
        Task { @MainActor [weak self] in
            guard let self, visibilityReporter?.isChatVisible(for: chatId) != true else {
                return
            }

            moduleNavigator.openChat(chatId)
        }

        return true
    }
}
