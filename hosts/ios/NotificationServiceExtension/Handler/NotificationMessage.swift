import Foundation

enum NotificationMessage {
    case newMessage(pushId: String, text: String)
}

extension NotificationMessage {
    init(userInfo: [AnyHashable: Any]) throws {
        guard let pushId = userInfo[PushNotificationKeys.pushId] as? String else {
            throw NotificationMessageError.invalidData
        }

        guard let message = userInfo[PushNotificationKeys.message] as? String else {
            throw NotificationMessageError.invalidData
        }

        self = .newMessage(pushId: pushId, text: message)
    }
}

enum NotificationMessageError: Error {
    case invalidData
}
