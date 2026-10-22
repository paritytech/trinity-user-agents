import Foundation

enum PushNotificationKeys {
    static let pushId = "pushId"
    static let pushSource = "pushSource"
    static let accountId = "accountId"
    static let message = "message"
    static let gameState = "gameState"
    static let gameIndex = "gameIndex"
    static let chatExtensionId = "chatExtensionId"
    static let deeplink = "deeplink"
    static let blockedContact = "blockedContact"
    /// The chat product of a notification of a room of the chat product.
    static let productId = "productId"
    /// The room of the chat product that a notification opens.
    static let productRoomId = "productRoomId"
}

enum PushNotificationSource: Int, Equatable {
    case chat = 0
    case products = 1
}

enum PushGameNotificationType: Int {
    case register = 0
    case waitingRoom = 1
    case start = 2
}
