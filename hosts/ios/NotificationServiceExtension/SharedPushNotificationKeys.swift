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
